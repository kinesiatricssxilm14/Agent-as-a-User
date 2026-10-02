//! The key map, declared once and used for both dispatch documentation and the
//! rendered help. Keeping the table next to the handler is what makes the
//! shortcuts discoverable in-app rather than only in a README.

use crate::app::View;

/// One documented binding: the keys, what it does, and where it applies.
pub struct Binding {
    pub keys: &'static str,
    pub action: &'static str,
}

/// A titled group of bindings in the help view.
pub struct Section {
    pub title: &'static str,
    pub bindings: &'static [Binding],
}

pub const GLOBAL: Section = Section {
    title: "Global",
    bindings: &[
        Binding { keys: "?  F1", action: "Open or close this help" },
        Binding { keys: "1 2 3 4 5", action: "Jump to SUMMARY / TRANSACTIONS / ACCOUNTS / CATEGORIES / BUDGETS" },
        Binding { keys: "Tab  Shift+Tab", action: "Next / previous view" },
        Binding { keys: "q  Ctrl+C", action: "Quit toold" },
        Binding { keys: "Esc", action: "Close a form, detail, prompt or dialog" },
        Binding { keys: "r  F5", action: "Reload everything from the database" },
    ],
};

pub const NAVIGATION: Section = Section {
    title: "Moving around a list",
    bindings: &[
        Binding { keys: "↑ ↓  k j", action: "Previous / next row" },
        Binding { keys: "PgUp PgDn", action: "Jump ten rows" },
        Binding { keys: "Home End  g G", action: "First / last row" },
        Binding { keys: "Enter", action: "Open the highlighted row (full detail in TRANSACTIONS)" },
    ],
};

pub const RECORDS: Section = Section {
    title: "Creating and changing records",
    bindings: &[
        Binding { keys: "n  a", action: "New record in the current view" },
        Binding { keys: "i", action: "New income record (from any view)" },
        Binding { keys: "e  x", action: "New expense record (from any view)" },
        Binding { keys: "b", action: "New or edit budget (from any view)" },
        Binding { keys: "E  F2", action: "Edit the highlighted record" },
        Binding { keys: "d  Delete", action: "Delete the highlighted record (asks first)" },
    ],
};

pub const PERIOD: Section = Section {
    title: "Month and filtering",
    bindings: &[
        Binding { keys: "← →  h l", action: "Previous / next month" },
        Binding { keys: "[ ]", action: "Previous / next month (alternate keys)" },
        Binding { keys: "T", action: "Jump to the current month" },
        Binding { keys: "m", action: "Limit TRANSACTIONS to the shown month (toggle)" },
        Binding { keys: "/", action: "Search transactions by payee, note, category, account, date or amount" },
        Binding { keys: "Esc  Ctrl+L", action: "Clear the search filter" },
    ],
};

pub const FORMS: Section = Section {
    title: "Inside a form",
    bindings: &[
        Binding { keys: "Tab  ↓ / Shift+Tab  ↑", action: "Next / previous field" },
        Binding { keys: "← →", action: "Change the value of a selection field" },
        Binding { keys: "Enter", action: "Save the record" },
        Binding { keys: "Esc", action: "Cancel without saving" },
        Binding { keys: "PgUp PgDn", action: "Step a date by a day, or a month by a month" },
        Binding { keys: "Ctrl+W  Ctrl+U  Ctrl+K", action: "Delete previous word / to start / to end" },
        Binding { keys: "Home End  ← →", action: "Move the text cursor" },
    ],
};

pub const CONFIRM: Section = Section {
    title: "Confirmation dialog",
    bindings: &[
        Binding { keys: "y  Enter", action: "Confirm the deletion" },
        Binding { keys: "n  Esc", action: "Keep the record" },
    ],
};

pub const ALL_SECTIONS: &[&Section] =
    &[&GLOBAL, &NAVIGATION, &RECORDS, &PERIOD, &FORMS, &CONFIRM];

/// The compact hint strip under each view. Short enough to fit one line at a
/// typical width, and tailored to what the focused view can actually do.
pub fn footer_hint(view: View) -> &'static str {
    match view {
        View::Summary => "←/→ month · T today · i income · e expense · b budget · Tab view · ? help · q quit",
        View::Transactions => "↑/↓ move · Enter detail · i income · e expense · E edit · d delete · / search · m month · ? help",
        View::Accounts => "↑/↓ move · n new · E edit · d delete · i income · e expense · Tab view · ? help",
        View::Categories => "↑/↓ move · n new · E edit · d delete · b budget · Tab view · ? help",
        View::Budgets => "↑/↓ move · n new · E edit · d delete · ←/→ month · T today · ? help",
    }
}

/// The hint strip shown while a modal has focus.
pub fn modal_hint(kind: ModalHint) -> &'static str {
    match kind {
        ModalHint::Form => {
            "Tab/↑↓ field · ←/→ choose · PgUp/PgDn step date · Enter save · Esc cancel · Ctrl+W delete word"
        }
        ModalHint::Detail => "E edit · d delete · Esc back to list · ↑/↓ scroll · ? help",
        ModalHint::Confirm => "y or Enter confirm · n or Esc cancel",
        ModalHint::Help => "↑/↓ or PgUp/PgDn scroll · ? or Esc close",
        ModalHint::Search => "type to filter · Enter apply · Esc cancel",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalHint {
    Form,
    Detail,
    Confirm,
    Help,
    Search,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_view_has_a_footer_hint_mentioning_help() {
        for view in View::ALL {
            let hint = footer_hint(view);
            assert!(!hint.is_empty(), "{} has no hint", view.title());
            assert!(hint.contains("? help"), "{} hint omits help", view.title());
        }
    }

    #[test]
    fn help_sections_are_populated() {
        assert!(!ALL_SECTIONS.is_empty());
        for section in ALL_SECTIONS {
            assert!(!section.title.is_empty());
            assert!(!section.bindings.is_empty(), "{} is empty", section.title);
            for binding in section.bindings {
                assert!(!binding.keys.is_empty());
                assert!(!binding.action.is_empty());
            }
        }
    }

    #[test]
    fn help_documents_the_keys_that_reach_every_view() {
        let text: String = ALL_SECTIONS
            .iter()
            .flat_map(|s| s.bindings.iter())
            .map(|b| format!("{} {}", b.keys, b.action))
            .collect::<Vec<_>>()
            .join("\n");
        for view in View::ALL {
            assert!(
                text.contains(view.title()),
                "help never names the {} view",
                view.title()
            );
        }
        // Core verbs a first-time user has to be able to find.
        for needle in ["Quit", "New record", "Edit the highlighted", "Delete the highlighted", "Search"] {
            assert!(text.contains(needle), "help omits '{needle}'");
        }
    }
}
