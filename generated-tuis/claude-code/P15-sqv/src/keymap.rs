//! The single source of truth for key bindings.
//!
//! Both the help pane and the footer hint line are generated from this table, so
//! documented keys and implemented keys cannot drift apart — and the help text
//! is testable.

/// One documented binding.
pub struct Binding {
    pub keys: &'static str,
    pub description: &'static str,
}

/// A titled group of bindings.
pub struct Section {
    pub title: &'static str,
    pub bindings: &'static [Binding],
}

const fn b(keys: &'static str, description: &'static str) -> Binding {
    Binding { keys, description }
}

pub const SECTIONS: &[Section] = &[
    Section {
        title: "Views",
        bindings: &[
            b("1", "data view — rows of the selected table"),
            b("2", "schema view — CREATE statement, columns, indexes, keys"),
            b("3  :", "SQL view — run any statement"),
            b("?  F1", "this help"),
            b("Tab  Shift+Tab", "move focus between table list, content and SQL line"),
            b("r  F5", "reload tables and rows from the database"),
            b("q  Ctrl+C", "quit"),
        ],
    },
    Section {
        title: "Table list (left pane)",
        bindings: &[
            b("↑ ↓  k j", "previous / next table or view"),
            b("PgUp PgDn", "jump a page"),
            b("Home End  g G", "first / last object"),
            b("Enter  →", "open it and focus the data grid"),
            b("/", "filter the object list by name"),
            b("x  Esc", "clear the object list filter"),
        ],
    },
    Section {
        title: "Data grid",
        bindings: &[
            b("↑ ↓  k j", "previous / next row"),
            b("← →  h l", "previous / next column"),
            b("PgUp PgDn", "scroll a page of rows"),
            b("Home End  g G", "first / last row"),
            b("^  $", "first / last column"),
            b("/", "incremental search across every column"),
            b("n  N", "next / previous search match"),
        ],
    },
    Section {
        title: "Sort",
        bindings: &[
            b("s", "sort ascending by the selected column"),
            b("S", "sort descending by the selected column"),
            b("o", "toggle sort direction on the selected column"),
            b("u", "clear the sort and return to database order"),
        ],
    },
    Section {
        title: "Filter",
        bindings: &[
            b("f", "filter the selected column"),
            b("Tab", "in the filter prompt: cycle contains → equals → regex"),
            b("Ctrl+A", "in the filter prompt: toggle case sensitivity"),
            b("Enter", "apply the filter"),
            b("Esc", "cancel the filter prompt"),
            b("x", "clear the filter on the selected column"),
            b("X", "clear every filter"),
        ],
    },
    Section {
        title: "Edit a cell",
        bindings: &[
            b("Enter  e", "edit the selected cell"),
            b("Enter", "save — the row is written back and re-read"),
            b("Ctrl+N", "store SQL NULL instead of the typed text"),
            b("Ctrl+R", "restore the original value"),
            b("Esc", "cancel without writing"),
        ],
    },
    Section {
        title: "SQL view",
        bindings: &[
            b("Enter", "run the statement on the SQL line"),
            b("↑ ↓", "recall previous statements"),
            b("Tab", "move to the result grid (sort and filter it like a table)"),
            b("Ctrl+W  Ctrl+U  Ctrl+K", "delete word / to start / to end"),
            b("Esc", "return to the data view"),
        ],
    },
    Section {
        title: "Text prompts",
        bindings: &[
            b("← →", "move the cursor"),
            b("Ctrl+← Ctrl+→", "move by word"),
            b("Home End", "start / end of line"),
            b("Backspace Delete", "delete backwards / forwards"),
            b("Ctrl+W", "delete the previous word"),
            b("Ctrl+U  Ctrl+K", "delete to start / to end of line"),
            b("↑ ↓", "recall earlier entries"),
        ],
    },
];

/// Compact hints for the footer, chosen per context.
pub fn footer_hints(context: HintContext) -> &'static str {
    match context {
        HintContext::Tables => {
            "↑↓ select · Enter open · / filter list · Tab data · 2 schema · 3 SQL · ? help · q quit"
        }
        HintContext::Data => {
            "↑↓ row · ←→ col · s/S sort · f filter · x/X clear · / search · Enter edit · 2 schema · 3 SQL · ? help"
        }
        HintContext::Schema => "j/k scroll · 1 data · 3 SQL · ? help · q quit",
        HintContext::Help => "j/k scroll · Esc or ? back · q quit",
        HintContext::SqlEditor => {
            "type SQL · Enter run · ↑↓ history · Tab result grid · Esc back · ? help"
        }
        HintContext::SqlGrid => {
            "↑↓ row · ←→ col · s/S sort · f filter · / search · Tab SQL line · 1 data"
        }
        HintContext::FilterPrompt => {
            "type a pattern · Tab match mode · Ctrl+A case · Enter apply · Esc cancel"
        }
        HintContext::EditPrompt => {
            "type a value · Enter save · Ctrl+N NULL · Ctrl+R restore · Esc cancel"
        }
        HintContext::SearchPrompt => "type to search live · Enter keep · Esc cancel",
        HintContext::TableFilterPrompt => "type to filter the object list · Enter keep · Esc clear",
    }
}

/// Which hint line to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintContext {
    Tables,
    Data,
    Schema,
    Help,
    SqlEditor,
    SqlGrid,
    FilterPrompt,
    EditPrompt,
    SearchPrompt,
    TableFilterPrompt,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_documents_something() {
        assert!(!SECTIONS.is_empty());
        for s in SECTIONS {
            assert!(!s.title.is_empty());
            assert!(!s.bindings.is_empty(), "{} is empty", s.title);
            for b in s.bindings {
                assert!(!b.keys.is_empty());
                assert!(!b.description.is_empty());
            }
        }
    }

    #[test]
    fn required_capabilities_are_all_documented() {
        let all: String = SECTIONS
            .iter()
            .flat_map(|s| s.bindings.iter())
            .map(|b| format!("{} {}\n", b.keys, b.description))
            .collect();
        for needle in [
            "sort ascending",
            "sort descending",
            "contains",
            "regex",
            "equals",
            "edit the selected cell",
            "run the statement",
            "search",
            "quit",
            "schema",
        ] {
            assert!(all.contains(needle), "help never mentions {needle}");
        }
    }

    #[test]
    fn every_hint_context_has_a_non_empty_line() {
        for c in [
            HintContext::Tables,
            HintContext::Data,
            HintContext::Schema,
            HintContext::Help,
            HintContext::SqlEditor,
            HintContext::SqlGrid,
            HintContext::FilterPrompt,
            HintContext::EditPrompt,
            HintContext::SearchPrompt,
            HintContext::TableFilterPrompt,
        ] {
            assert!(!footer_hints(c).is_empty(), "{c:?} has no hints");
        }
    }
}
