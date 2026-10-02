//! SQLite dynamic value representation and the ordering / formatting rules that
//! go with it.

use std::cmp::Ordering;
use std::fmt;

/// One cell as it came out of SQLite. Mirrors the five storage classes.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    /// Storage-class rank, used for cross-type ordering (SQLite's own rule:
    /// NULL < INTEGER/REAL < TEXT < BLOB).
    fn class_rank(&self) -> u8 {
        match self {
            Value::Null => 0,
            Value::Int(_) | Value::Real(_) => 1,
            Value::Text(_) => 2,
            Value::Blob(_) => 3,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Real(r) => Some(*r),
            _ => None,
        }
    }

    /// Total order over values, following SQLite's comparison semantics closely
    /// enough that sorting a column here matches `ORDER BY`.
    pub fn sqlite_cmp(&self, other: &Value) -> Ordering {
        let (a, b) = (self.class_rank(), other.class_rank());
        if a != b {
            return a.cmp(&b);
        }
        match (self, other) {
            (Value::Null, Value::Null) => Ordering::Equal,
            (Value::Text(x), Value::Text(y)) => x.cmp(y),
            (Value::Blob(x), Value::Blob(y)) => x.cmp(y),
            _ => {
                // Both numeric. Compare exactly when both are integers so that
                // large i64 values do not collide after an f64 cast.
                if let (Value::Int(x), Value::Int(y)) = (self, other) {
                    return x.cmp(y);
                }
                let x = self.as_f64().unwrap_or(f64::NAN);
                let y = other.as_f64().unwrap_or(f64::NAN);
                x.partial_cmp(&y).unwrap_or(Ordering::Equal)
            }
        }
    }

    /// How the value is rendered in the grid and the row detail pane.
    pub fn display(&self) -> String {
        match self {
            Value::Null => "NULL".to_string(),
            Value::Int(i) => i.to_string(),
            Value::Real(r) => format_real(*r),
            Value::Text(t) => t.replace('\n', "\\n").replace('\t', "    "),
            Value::Blob(b) => format!("«blob {} bytes»", b.len()),
        }
    }

    /// The text a filter is matched against. NULL deliberately matches the empty
    /// string rather than the literal "NULL" so that a `contains` filter never
    /// picks up empty cells by accident.
    pub fn filter_text(&self) -> String {
        match self {
            Value::Null => String::new(),
            other => other.display(),
        }
    }

    /// Value shown when a cell is opened for editing (round-trips losslessly for
    /// text, unlike [`Value::display`]).
    pub fn edit_text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Text(t) => t.clone(),
            Value::Real(r) => format_real(*r),
            Value::Int(i) => i.to_string(),
            Value::Blob(_) => String::new(),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn is_blob(&self) -> bool {
        matches!(self, Value::Blob(_))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Int(_) => "integer",
            Value::Real(_) => "real",
            Value::Text(_) => "text",
            Value::Blob(_) => "blob",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}

/// Format a float the way the `sqlite3` shell does: whole numbers keep one
/// decimal place so they stay visibly distinct from integers.
fn format_real(r: f64) -> String {
    if r.is_nan() {
        return "NaN".to_string();
    }
    if r.is_infinite() {
        return if r > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    if r.fract() == 0.0 && r.abs() < 1e15 {
        format!("{r:.1}")
    } else {
        let s = format!("{r}");
        if s.contains(['.', 'e', 'E']) {
            s
        } else {
            format!("{s}.0")
        }
    }
}

impl rusqlite::ToSql for Value {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, ValueRef};
        Ok(match self {
            Value::Null => ToSqlOutput::Borrowed(ValueRef::Null),
            Value::Int(i) => ToSqlOutput::Borrowed(ValueRef::Integer(*i)),
            Value::Real(r) => ToSqlOutput::Borrowed(ValueRef::Real(*r)),
            Value::Text(t) => ToSqlOutput::Borrowed(ValueRef::Text(t.as_bytes())),
            Value::Blob(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
        })
    }
}

impl<'a> From<rusqlite::types::ValueRef<'a>> for Value {
    fn from(v: rusqlite::types::ValueRef<'a>) -> Self {
        use rusqlite::types::ValueRef;
        match v {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(i) => Value::Int(i),
            ValueRef::Real(r) => Value::Real(r),
            ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
            ValueRef::Blob(b) => Value::Blob(b.to_vec()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn cross_type_order_follows_sqlite() {
        let mut vs = vec![
            Value::Text("a".into()),
            Value::Null,
            Value::Blob(vec![1]),
            Value::Int(5),
        ];
        vs.sort_by(|a, b| a.sqlite_cmp(b));
        assert_eq!(
            vs,
            vec![
                Value::Null,
                Value::Int(5),
                Value::Text("a".into()),
                Value::Blob(vec![1]),
            ]
        );
    }

    #[test]
    fn int_and_real_compare_numerically() {
        assert_eq!(Value::Int(2).sqlite_cmp(&Value::Real(10.5)), Ordering::Less);
        assert_eq!(Value::Real(2.5).sqlite_cmp(&Value::Int(2)), Ordering::Greater);
    }

    #[test]
    fn large_integers_do_not_collide() {
        let a = Value::Int(i64::MAX);
        let b = Value::Int(i64::MAX - 1);
        assert_eq!(a.sqlite_cmp(&b), Ordering::Greater);
    }

    #[test]
    fn reals_render_with_decimal_point() {
        assert_eq!(Value::Real(3.0).display(), "3.0");
        assert_eq!(Value::Real(3.25).display(), "3.25");
        assert_eq!(Value::Int(3).display(), "3");
    }

    #[test]
    fn null_filters_as_empty_string() {
        assert_eq!(Value::Null.filter_text(), "");
        assert_eq!(Value::Null.display(), "NULL");
    }
}
