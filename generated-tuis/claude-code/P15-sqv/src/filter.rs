//! Column filtering: the three match modes required by the spec, plus the
//! compiled-filter type the grid holds.

use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};

use crate::value::Value;

/// How a filter needle is compared against a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    /// Cell text contains the needle as a substring.
    Contains,
    /// Cell text equals the needle in full.
    Exact,
    /// Cell text matches the needle interpreted as a regular expression.
    Regex,
}

impl MatchMode {
    pub const ALL: [MatchMode; 3] = [MatchMode::Contains, MatchMode::Exact, MatchMode::Regex];

    pub fn label(self) -> &'static str {
        match self {
            MatchMode::Contains => "contains",
            MatchMode::Exact => "equals",
            MatchMode::Regex => "regex",
        }
    }

    /// Short form for the compact filter badge in the header.
    pub fn sigil(self) -> &'static str {
        match self {
            MatchMode::Contains => "~",
            MatchMode::Exact => "=",
            MatchMode::Regex => "re",
        }
    }

    /// Cycle to the next mode; wired to Tab in the filter prompt.
    pub fn next(self) -> MatchMode {
        match self {
            MatchMode::Contains => MatchMode::Exact,
            MatchMode::Exact => MatchMode::Regex,
            MatchMode::Regex => MatchMode::Contains,
        }
    }
}

/// A filter ready to be applied to rows: needle already compiled/case-folded.
#[derive(Debug, Clone)]
pub struct Filter {
    /// Index of the column this filter applies to.
    pub column: usize,
    pub column_name: String,
    pub mode: MatchMode,
    /// The needle exactly as the user typed it (for display).
    pub needle: String,
    case_sensitive: bool,
    /// Lower-cased needle, precomputed for case-insensitive literal modes.
    folded: String,
    regex: Option<Regex>,
}

impl Filter {
    /// Build a filter, compiling the regex up front so a bad pattern is reported
    /// while the prompt is still open rather than during rendering.
    pub fn new(
        column: usize,
        column_name: impl Into<String>,
        mode: MatchMode,
        needle: impl Into<String>,
        case_sensitive: bool,
    ) -> Result<Filter> {
        let needle: String = needle.into();
        let regex = if mode == MatchMode::Regex {
            Some(
                RegexBuilder::new(&needle)
                    .case_insensitive(!case_sensitive)
                    .size_limit(1 << 22)
                    .build()
                    .with_context(|| format!("invalid regular expression: {needle}"))?,
            )
        } else {
            None
        };
        Ok(Filter {
            column,
            column_name: column_name.into(),
            mode,
            folded: needle.to_lowercase(),
            needle,
            case_sensitive,
            regex,
        })
    }

    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    /// Does this cell pass the filter?
    pub fn matches(&self, cell: &Value) -> bool {
        let hay = cell.filter_text();
        match self.mode {
            MatchMode::Regex => self.regex.as_ref().is_some_and(|re| re.is_match(&hay)),
            MatchMode::Contains => {
                if self.case_sensitive {
                    hay.contains(&self.needle)
                } else {
                    hay.to_lowercase().contains(&self.folded)
                }
            }
            MatchMode::Exact => {
                if self.case_sensitive {
                    hay == self.needle
                } else {
                    hay.to_lowercase() == self.folded
                }
            }
        }
    }

    /// One-line description for the header badge, e.g. `dept ~ "eng"`.
    pub fn summary(&self) -> String {
        format!(
            "{} {} \"{}\"{}",
            self.column_name,
            self.mode.sigil(),
            self.needle,
            if self.case_sensitive { " Aa" } else { "" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(mode: MatchMode, needle: &str) -> Filter {
        Filter::new(0, "c", mode, needle, false).unwrap()
    }

    #[test]
    fn contains_matches_substring() {
        let flt = f(MatchMode::Contains, "ngi");
        assert!(flt.matches(&Value::Text("engineering".into())));
        assert!(!flt.matches(&Value::Text("ops".into())));
    }

    #[test]
    fn exact_requires_whole_value() {
        let flt = f(MatchMode::Exact, "eng");
        assert!(flt.matches(&Value::Text("eng".into())));
        assert!(!flt.matches(&Value::Text("engineering".into())));
    }

    #[test]
    fn regex_anchors_and_classes_work() {
        let flt = f(MatchMode::Regex, r"^\d{3}-\d{4}$");
        assert!(flt.matches(&Value::Text("555-1234".into())));
        assert!(!flt.matches(&Value::Text("5551234".into())));
    }

    #[test]
    fn regex_is_a_search_not_a_full_match() {
        let flt = f(MatchMode::Regex, "ee");
        assert!(flt.matches(&Value::Text("engineering".into())));
    }

    #[test]
    fn invalid_regex_is_reported_at_construction() {
        let err = Filter::new(0, "c", MatchMode::Regex, "a(b", false).unwrap_err();
        assert!(err.to_string().contains("invalid regular expression"));
    }

    #[test]
    fn case_insensitive_by_default_in_all_modes() {
        assert!(f(MatchMode::Contains, "ENG").matches(&Value::Text("eng".into())));
        assert!(f(MatchMode::Exact, "ENG").matches(&Value::Text("eng".into())));
        assert!(f(MatchMode::Regex, "^ENG$").matches(&Value::Text("eng".into())));
    }

    #[test]
    fn case_sensitive_mode_respects_case() {
        let flt = Filter::new(0, "c", MatchMode::Exact, "ENG", true).unwrap();
        assert!(!flt.matches(&Value::Text("eng".into())));
        assert!(flt.matches(&Value::Text("ENG".into())));
    }

    #[test]
    fn numbers_are_matched_by_their_display_text() {
        assert!(f(MatchMode::Contains, "45").matches(&Value::Int(1450)));
        assert!(f(MatchMode::Exact, "1.5").matches(&Value::Real(1.5)));
        assert!(f(MatchMode::Exact, "3.0").matches(&Value::Real(3.0)));
    }

    #[test]
    fn null_is_empty_so_contains_never_falsely_matches() {
        assert!(!f(MatchMode::Contains, "NULL").matches(&Value::Null));
        assert!(f(MatchMode::Exact, "").matches(&Value::Null));
        assert!(f(MatchMode::Regex, "^$").matches(&Value::Null));
    }

    #[test]
    fn empty_contains_needle_matches_everything() {
        let flt = f(MatchMode::Contains, "");
        assert!(flt.matches(&Value::Text("anything".into())));
        assert!(flt.matches(&Value::Null));
    }

    #[test]
    fn mode_cycles_through_all_three() {
        let mut m = MatchMode::Contains;
        let mut seen = vec![m];
        for _ in 0..2 {
            m = m.next();
            seen.push(m);
        }
        assert_eq!(seen, MatchMode::ALL.to_vec());
        assert_eq!(m.next(), MatchMode::Contains);
    }

    #[test]
    fn summary_is_human_readable() {
        let flt = Filter::new(2, "dept", MatchMode::Contains, "eng", false).unwrap();
        assert_eq!(flt.summary(), "dept ~ \"eng\"");
    }
}
