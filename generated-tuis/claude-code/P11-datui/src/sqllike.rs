//! Parser for the `SQL-Like` query mode.
//!
//! Grammar (case-insensitive keywords):
//!
//! ```text
//! query      := [ "select" [ projection ] ] [ "where" ] expr
//! projection := "*" | ident { "," ident }
//! expr       := or_expr
//! or_expr    := and_expr { "or" and_expr }
//! and_expr   := not_expr { "and" not_expr }
//! not_expr   := "not" not_expr | primary
//! primary    := "(" expr ")" | comparison
//! comparison := ident op literal
//!             | ident "is" [ "not" ] "null"
//!             | ident [ "not" ] "between" literal "and" literal
//!             | ident [ "not" ] "in" "(" literal { "," literal } ")"
//!             | ident [ "not" ] ( "like" | "contains" ) literal
//! op         := "=" | "==" | "!=" | "<>" | ">" | "<" | ">=" | "<="
//! ```
//!
//! The `select`/`where` keywords are both optional so that a bare condition
//! (`age > 40`) also works, but the documented `select where age > 40` form is
//! the canonical one.

use polars::prelude::*;

use crate::data::{resolve_column, ColumnKind};

/// A parsed SQL-Like query: an optional projection plus a filter predicate.
#[derive(Debug, Clone)]
pub struct LikeQuery {
    /// Selected columns; empty means "all columns".
    pub projection: Vec<String>,
    /// Filter predicate; `None` means "no filtering".
    pub predicate: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    /// A quoted string literal — always a string, never re-parsed as a number.
    Str(String),
    Num(f64),
    Op(String),
    LParen,
    RParen,
    Comma,
}

impl Tok {
    fn as_keyword(&self) -> Option<String> {
        match self {
            Tok::Ident(s) => Some(s.to_ascii_lowercase()),
            _ => None,
        }
    }
}

fn tokenize(input: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '\'' | '"' => {
                // Quoted literal. A doubled quote is an escaped quote.
                let quote = c;
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= chars.len() {
                        return Err(format!("unterminated string literal (missing closing {quote})"));
                    }
                    if chars[i] == quote {
                        if i + 1 < chars.len() && chars[i + 1] == quote {
                            s.push(quote);
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        // Accept common backslash escapes inside literals.
                        let n = chars[i + 1];
                        s.push(match n {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                        i += 2;
                        continue;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                out.push(Tok::Str(s));
            }
            '`' | '[' => {
                // Quoted identifier, for column names containing spaces.
                let close = if c == '[' { ']' } else { '`' };
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != close {
                    s.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(format!("unterminated identifier (missing closing {close})"));
                }
                i += 1;
                out.push(Tok::Ident(s));
            }
            '*' => {
                out.push(Tok::Op("*".to_string()));
                i += 1;
            }
            '>' | '<' | '=' | '!' => {
                let mut op = String::new();
                op.push(c);
                i += 1;
                if i < chars.len() && (chars[i] == '=' || (c == '<' && chars[i] == '>')) {
                    op.push(chars[i]);
                    i += 1;
                }
                if op == "!" {
                    return Err("expected `!=` but found `!`".to_string());
                }
                out.push(Tok::Op(op));
            }
            _ => {
                if c.is_ascii_digit()
                    || (c == '-'
                        && i + 1 < chars.len()
                        && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '.'))
                    || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
                {
                    let start = i;
                    if chars[i] == '-' {
                        i += 1;
                    }
                    let mut seen_dot = false;
                    let mut seen_exp = false;
                    while i < chars.len() {
                        let d = chars[i];
                        if d.is_ascii_digit() {
                            i += 1;
                        } else if d == '.' && !seen_dot && !seen_exp {
                            seen_dot = true;
                            i += 1;
                        } else if (d == 'e' || d == 'E') && !seen_exp && i + 1 < chars.len() {
                            let n = chars[i + 1];
                            if n.is_ascii_digit() || n == '+' || n == '-' {
                                seen_exp = true;
                                i += 2;
                            } else {
                                break;
                            }
                        } else if d == '_' {
                            // Allow 1_000 style separators.
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    let raw: String = chars[start..i].iter().filter(|c| **c != '_').collect();
                    let v: f64 = raw
                        .parse()
                        .map_err(|_| format!("invalid number `{raw}`"))?;
                    out.push(Tok::Num(v));
                } else if c.is_alphanumeric() || c == '_' || c == '.' || c == '-' || c == '/' || c == ':' {
                    let start = i;
                    while i < chars.len() {
                        let d = chars[i];
                        if d.is_alphanumeric() || d == '_' || d == '.' || d == '-' || d == '/' || d == ':' {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    let word: String = chars[start..i].iter().collect();
                    out.push(Tok::Ident(word));
                } else {
                    return Err(format!("unexpected character `{c}`"));
                }
            }
        }
    }
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    /// Column name + kind, used to coerce literals to the column's type.
    columns: &'a [(String, ColumnKind)],
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    /// Consume the next token if it is the given keyword.
    fn eat_keyword(&mut self, kw: &str) -> bool {
        if let Some(t) = self.peek() {
            if t.as_keyword().as_deref() == Some(kw) {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<(), String> {
        if self.eat_keyword(kw) {
            Ok(())
        } else {
            Err(format!("expected `{kw}`{}", self.at_hint()))
        }
    }

    fn at_hint(&self) -> String {
        match self.peek() {
            Some(Tok::Ident(s)) => format!(" but found `{s}`"),
            Some(Tok::Str(s)) => format!(" but found `'{s}'`"),
            Some(Tok::Num(n)) => format!(" but found `{n}`"),
            Some(Tok::Op(o)) => format!(" but found `{o}`"),
            Some(Tok::LParen) => " but found `(`".to_string(),
            Some(Tok::RParen) => " but found `)`".to_string(),
            Some(Tok::Comma) => " but found `,`".to_string(),
            None => " but the query ended".to_string(),
        }
    }

    fn or_expr(&mut self) -> Result<Expr, String> {
        let mut lhs = self.and_expr()?;
        while self.eat_keyword("or") {
            let rhs = self.and_expr()?;
            lhs = lhs.or(rhs);
        }
        Ok(lhs)
    }

    fn and_expr(&mut self) -> Result<Expr, String> {
        let mut lhs = self.not_expr()?;
        while self.eat_keyword("and") {
            let rhs = self.not_expr()?;
            lhs = lhs.and(rhs);
        }
        Ok(lhs)
    }

    fn not_expr(&mut self) -> Result<Expr, String> {
        if self.eat_keyword("not") {
            let inner = self.not_expr()?;
            // Treat a null result as "did not match" so `not` never silently
            // drops rows through null propagation.
            return Ok(inner.fill_null(lit(false)).not());
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, String> {
        if let Some(Tok::LParen) = self.peek() {
            self.pos += 1;
            let e = self.or_expr()?;
            match self.next() {
                Some(Tok::RParen) => Ok(e),
                _ => Err("expected `)` to close the group".to_string()),
            }
        } else {
            self.comparison()
        }
    }

    /// Read a column reference and return its canonical name and kind.
    fn column(&mut self) -> Result<(String, ColumnKind), String> {
        let name = match self.next() {
            Some(Tok::Ident(s)) => s,
            other => {
                return Err(format!(
                    "expected a column name but found {}",
                    describe(other.as_ref())
                ))
            }
        };
        // Strip a `df.` style qualifier if the user typed one.
        let bare = match name.rsplit_once('.') {
            Some((prefix, rest))
                if !rest.is_empty()
                    && resolve_column(self.columns, &name).is_none()
                    && !prefix.is_empty() =>
            {
                rest.to_string()
            }
            _ => name.clone(),
        };
        match resolve_column(self.columns, &bare) {
            Some(found) => Ok(found),
            None => Err(format!("unknown column `{name}`")),
        }
    }

    fn literal(&mut self) -> Result<Lit, String> {
        match self.next() {
            Some(Tok::Num(v)) => Ok(Lit::Num(v)),
            Some(Tok::Str(s)) => Ok(Lit::Str(s)),
            Some(Tok::Ident(s)) => {
                let low = s.to_ascii_lowercase();
                match low.as_str() {
                    "true" => Ok(Lit::Bool(true)),
                    "false" => Ok(Lit::Bool(false)),
                    "null" => Ok(Lit::Null),
                    // A bare word is a string, so `department = Engineering`
                    // works without quotes.
                    _ => Ok(Lit::Str(s)),
                }
            }
            other => Err(format!(
                "expected a value but found {}",
                describe(other.as_ref())
            )),
        }
    }

    fn comparison(&mut self) -> Result<Expr, String> {
        let (name, kind) = self.column()?;
        let c = col(name.as_str());

        // `is null` / `is not null`
        if self.eat_keyword("is") {
            let negated = self.eat_keyword("not");
            self.expect_keyword("null")?;
            return Ok(if negated { c.is_not_null() } else { c.is_null() });
        }

        // A leading `not` applies to between / in / like.
        let mut negated = self.eat_keyword("not");

        if self.eat_keyword("between") {
            let lo = self.literal()?;
            self.expect_keyword("and")?;
            let hi = self.literal()?;
            let e = compare(&c, kind, ">=", &lo)?.and(compare(&c, kind, "<=", &hi)?);
            return Ok(maybe_not(e, negated));
        }

        if self.eat_keyword("in") {
            match self.next() {
                Some(Tok::LParen) => {}
                _ => return Err("expected `(` after `in`".to_string()),
            }
            let mut acc: Option<Expr> = None;
            loop {
                let l = self.literal()?;
                let eq = compare(&c, kind, "=", &l)?;
                acc = Some(match acc {
                    Some(a) => a.or(eq),
                    None => eq,
                });
                match self.next() {
                    Some(Tok::Comma) => continue,
                    Some(Tok::RParen) => break,
                    other => {
                        return Err(format!(
                            "expected `,` or `)` in the `in` list but found {}",
                            describe(other.as_ref())
                        ))
                    }
                }
            }
            let e = acc.ok_or_else(|| "the `in` list is empty".to_string())?;
            return Ok(maybe_not(e, negated));
        }

        if self.eat_keyword("like") {
            let pat = self.literal()?;
            let e = like_expr(&c, &pat.to_text())?;
            return Ok(maybe_not(e, negated));
        }

        if self.eat_keyword("contains") {
            let pat = self.literal()?;
            let e = c
                .cast(DataType::String)
                .str()
                .to_lowercase()
                .str()
                .contains_literal(lit(pat.to_text().to_lowercase()));
            return Ok(maybe_not(e, negated));
        }

        if negated {
            return Err("`not` must be followed by `between`, `in`, `like` or `contains`".to_string());
        }
        negated = false;
        let _ = negated;

        // Plain comparison operator.
        let op = match self.next() {
            Some(Tok::Op(o)) => o,
            other => {
                return Err(format!(
                    "expected a comparison operator after `{name}` but found {}",
                    describe(other.as_ref())
                ))
            }
        };
        let rhs = self.literal()?;
        compare(&c, kind, &op, &rhs)
    }
}

fn maybe_not(e: Expr, negated: bool) -> Expr {
    if negated {
        e.fill_null(lit(false)).not()
    } else {
        e
    }
}

fn describe(t: Option<&Tok>) -> String {
    match t {
        Some(Tok::Ident(s)) => format!("`{s}`"),
        Some(Tok::Str(s)) => format!("`'{s}'`"),
        Some(Tok::Num(n)) => format!("`{n}`"),
        Some(Tok::Op(o)) => format!("`{o}`"),
        Some(Tok::LParen) => "`(`".to_string(),
        Some(Tok::RParen) => "`)`".to_string(),
        Some(Tok::Comma) => "`,`".to_string(),
        None => "the end of the query".to_string(),
    }
}

/// A literal value from the query text.
#[derive(Debug, Clone)]
enum Lit {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
}

impl Lit {
    fn to_text(&self) -> String {
        match self {
            Lit::Num(v) => crate::fmtnum::auto_float(*v),
            Lit::Str(s) => s.clone(),
            Lit::Bool(b) => b.to_string(),
            Lit::Null => "null".to_string(),
        }
    }

    /// A literal that came from a bare/quoted word but is really a number.
    fn as_number(&self) -> Option<f64> {
        match self {
            Lit::Num(v) => Some(*v),
            Lit::Str(s) => s.trim().parse::<f64>().ok(),
            Lit::Bool(_) | Lit::Null => None,
        }
    }
}

/// Build a comparison expression, coercing the literal to the column's type.
///
/// The coercion is what makes `salary > 10000` work whether the CSV column was
/// inferred as an integer, a float, or a string.
fn compare(c: &Expr, kind: ColumnKind, op: &str, rhs: &Lit) -> Result<Expr, String> {
    if let Lit::Null = rhs {
        return match op {
            "=" | "==" => Ok(c.clone().is_null()),
            "!=" | "<>" => Ok(c.clone().is_not_null()),
            _ => Err(format!("cannot use `{op}` with `null`; use `is null` instead")),
        };
    }

    // Decide which side to coerce. Numeric comparisons win whenever both the
    // column and the literal can be read as numbers, so ordering works
    // numerically rather than lexicographically.
    let numeric = match kind {
        ColumnKind::Numeric => rhs.as_number().is_some(),
        ColumnKind::Temporal => false,
        ColumnKind::Boolean => false,
        ColumnKind::Text | ColumnKind::Other => {
            // Only treat a text column numerically for ordering operators,
            // where a lexicographic result would be surprising.
            matches!(op, ">" | "<" | ">=" | "<=") && rhs.as_number().is_some()
        }
    };

    let (lhs, value) = if numeric {
        let n = rhs.as_number().unwrap();
        let lhs = if matches!(kind, ColumnKind::Numeric) {
            c.clone().cast(DataType::Float64)
        } else {
            // Non-strict cast: unparsable text becomes null and drops out.
            c.clone().cast(DataType::String).cast(DataType::Float64)
        };
        (lhs, lit(n))
    } else if matches!(kind, ColumnKind::Boolean) {
        let b = match rhs {
            Lit::Bool(b) => *b,
            Lit::Num(v) => *v != 0.0,
            Lit::Str(s) => match s.to_ascii_lowercase().as_str() {
                "true" | "t" | "yes" | "y" | "1" => true,
                "false" | "f" | "no" | "n" | "0" => false,
                _ => return Err(format!("`{s}` is not a boolean value")),
            },
            Lit::Null => unreachable!("handled above"),
        };
        (c.clone(), lit(b))
    } else {
        // String comparison. Equality is case-insensitive so that
        // `department = 'engineering'` matches `Engineering`; ordering keeps the
        // original case to preserve a stable collation.
        let text = rhs.to_text();
        if matches!(op, "=" | "==" | "!=" | "<>") {
            let lhs = c.clone().cast(DataType::String).str().to_lowercase();
            (lhs, lit(text.to_lowercase()))
        } else {
            (c.clone().cast(DataType::String), lit(text))
        }
    };

    Ok(match op {
        "=" | "==" => lhs.eq(value),
        "!=" | "<>" => lhs.neq(value),
        ">" => lhs.gt(value),
        "<" => lhs.lt(value),
        ">=" => lhs.gt_eq(value),
        "<=" => lhs.lt_eq(value),
        other => return Err(format!("unsupported operator `{other}`")),
    })
}

/// Translate a SQL `LIKE` pattern (`%`, `_`) into a case-insensitive regex.
fn like_expr(c: &Expr, pattern: &str) -> Result<Expr, String> {
    let mut re = String::from("(?is)^");
    for ch in pattern.chars() {
        match ch {
            '%' => re.push_str(".*"),
            '_' => re.push('.'),
            c => {
                if "\\.+*?()|[]{}^$".contains(c) {
                    re.push('\\');
                }
                re.push(c);
            }
        }
    }
    re.push('$');
    Ok(c.clone().cast(DataType::String).str().contains(lit(re), true))
}

/// Parse a SQL-Like query against the given columns.
pub fn parse(input: &str, columns: &[(String, ColumnKind)]) -> Result<LikeQuery, String> {
    let trimmed = input.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return Ok(LikeQuery {
            projection: Vec::new(),
            predicate: None,
        });
    }

    let toks = tokenize(trimmed)?;
    let mut p = Parser {
        toks,
        pos: 0,
        columns,
    };

    let mut projection: Vec<String> = Vec::new();

    // Optional `select [cols]`.
    if p.eat_keyword("select") {
        // A `*` projection, or an explicit column list, or nothing before `where`.
        if let Some(Tok::Op(o)) = p.peek() {
            if o == "*" {
                p.pos += 1;
            }
        }
        loop {
            let is_where = p
                .peek()
                .and_then(|t| t.as_keyword())
                .map(|k| k == "where")
                .unwrap_or(false);
            if is_where || p.peek().is_none() {
                break;
            }
            // `select from df where ...` — tolerate the SQL-ish extras.
            if p.eat_keyword("from") {
                let _ = p.next(); // table name
                continue;
            }
            match p.peek() {
                Some(Tok::Ident(_)) => {
                    let (name, _) = p.column()?;
                    projection.push(name);
                    if p.eat_keyword("as") {
                        let _ = p.next();
                    }
                    if let Some(Tok::Comma) = p.peek() {
                        p.pos += 1;
                        continue;
                    }
                }
                _ => break,
            }
            let next_is_where = p
                .peek()
                .and_then(|t| t.as_keyword())
                .map(|k| k == "where")
                .unwrap_or(false);
            if next_is_where || p.peek().is_none() {
                break;
            }
        }
    }

    // `where` is optional; a bare condition is accepted too.
    p.eat_keyword("where");

    if p.peek().is_none() {
        return Ok(LikeQuery {
            projection,
            predicate: None,
        });
    }

    let e = p.or_expr()?;
    if p.pos < p.toks.len() {
        return Err(format!(
            "unexpected trailing input{}",
            p.at_hint()
        ));
    }

    Ok(LikeQuery {
        projection,
        predicate: Some(e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols() -> Vec<(String, ColumnKind)> {
        vec![
            ("name".to_string(), ColumnKind::Text),
            ("age".to_string(), ColumnKind::Numeric),
            ("department".to_string(), ColumnKind::Text),
            ("salary".to_string(), ColumnKind::Numeric),
            ("active".to_string(), ColumnKind::Boolean),
        ]
    }

    fn frame() -> DataFrame {
        df![
            "name" => ["Ann", "Bob", "Cid", "Dee"],
            "age" => [35i64, 45, 52, 29],
            "department" => ["Engineering", "Sales", "Engineering", "Sales"],
            "salary" => [12000i64, 9000, 20000, 5000],
            "active" => [true, false, true, true],
        ]
        .unwrap()
    }

    fn run(q: &str) -> Vec<String> {
        let parsed = parse(q, &cols()).expect("query should parse");
        let df = frame();
        let out = match parsed.predicate {
            Some(p) => df.lazy().filter(p).collect().unwrap(),
            None => df,
        };
        let c = out.column("name").unwrap();
        (0..out.height())
            .map(|i| c.get(i).unwrap().str_value().to_string())
            .collect()
    }

    #[test]
    fn documented_examples_parse_and_filter() {
        assert_eq!(run("select where age > 40"), vec!["Bob", "Cid"]);
        assert_eq!(
            run("select where department = 'Engineering' and salary > 10000"),
            vec!["Ann", "Cid"]
        );
    }

    #[test]
    fn or_and_precedence_matches_sql() {
        // `and` binds tighter than `or`.
        assert_eq!(
            run("select where department = 'Sales' and age > 40 or age < 30"),
            vec!["Bob", "Dee"]
        );
        assert_eq!(
            run("select where (department = 'Sales' or department = 'Engineering') and salary >= 12000"),
            vec!["Ann", "Cid"]
        );
    }

    #[test]
    fn optional_keywords_and_operators() {
        assert_eq!(run("age > 40"), vec!["Bob", "Cid"]);
        assert_eq!(run("where age >= 45"), vec!["Bob", "Cid"]);
        assert_eq!(run("select * where age <= 29"), vec!["Dee"]);
        assert_eq!(run("select where age <> 35"), vec!["Bob", "Cid", "Dee"]);
        assert_eq!(run("select where age != 35"), vec!["Bob", "Cid", "Dee"]);
    }

    #[test]
    fn string_equality_is_case_insensitive() {
        assert_eq!(run("select where department = engineering"), vec!["Ann", "Cid"]);
        assert_eq!(run("select where DEPARTMENT = 'ENGINEERING'"), vec!["Ann", "Cid"]);
    }

    #[test]
    fn between_in_like_and_not() {
        assert_eq!(run("select where age between 30 and 46"), vec!["Ann", "Bob"]);
        assert_eq!(run("select where department in ('Sales')"), vec!["Bob", "Dee"]);
        assert_eq!(run("select where name like 'A%'"), vec!["Ann"]);
        assert_eq!(run("select where name contains 'o'"), vec!["Bob"]);
        assert_eq!(run("select where not department = 'Sales'"), vec!["Ann", "Cid"]);
        assert_eq!(run("select where department not in ('Sales')"), vec!["Ann", "Cid"]);
    }

    #[test]
    fn booleans_and_nulls() {
        assert_eq!(run("select where active = false"), vec!["Bob"]);
        assert_eq!(run("select where age is not null").len(), 4);
        assert_eq!(run("select where age is null").len(), 0);
    }

    #[test]
    fn projection_is_captured() {
        let q = parse("select name, salary where age > 40", &cols()).unwrap();
        assert_eq!(q.projection, vec!["name".to_string(), "salary".to_string()]);
        let q = parse("select * where age > 40", &cols()).unwrap();
        assert!(q.projection.is_empty());
    }

    #[test]
    fn errors_are_actionable() {
        let e = parse("select where nope > 1", &cols()).unwrap_err();
        assert!(e.contains("unknown column"), "{e}");
        let e = parse("select where age >", &cols()).unwrap_err();
        assert!(e.contains("expected a value"), "{e}");
        let e = parse("select where age 40", &cols()).unwrap_err();
        assert!(e.contains("comparison operator"), "{e}");
        let e = parse("select where (age > 1", &cols()).unwrap_err();
        assert!(e.contains(')'), "{e}");
    }

    #[test]
    fn empty_query_matches_everything() {
        let q = parse("   ", &cols()).unwrap();
        assert!(q.predicate.is_none());
    }
}
