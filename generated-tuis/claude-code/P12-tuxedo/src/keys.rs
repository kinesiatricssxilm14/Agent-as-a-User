//! The single source of truth for key bindings, used by the help screen, the
//! footer hint bar and the `--help` text.
//!
//! Keeping the documentation next to nothing but data means the help screen can
//! never drift out of sync with what the footer advertises.

/// One documented binding.
pub struct Binding {
    pub keys: &'static str,
    pub what: &'static str,
}

/// A titled group of bindings.
pub struct Section {
    pub title: &'static str,
    pub bindings: &'static [Binding],
}

macro_rules! b {
    ($keys:expr, $what:expr) => {
        Binding {
            keys: $keys,
            what: $what,
        }
    };
}

pub static SECTIONS: &[Section] = &[
    Section {
        title: "Moving around",
        bindings: &[
            b!("↑ / k", "previous item in the focused panel"),
            b!("↓ / j", "next item in the focused panel"),
            b!("PgUp / PgDn", "move a screen at a time"),
            b!("Home / g", "jump to the first item"),
            b!("End / G", "jump to the last item"),
            b!("Tab / →", "focus the next panel (tasks → projects → contexts)"),
            b!("Shift-Tab / ←", "focus the previous panel"),
            b!("1 / 2 / 3", "focus tasks / projects / contexts directly"),
            b!("> / <", "scroll long task lines right / left"),
            b!("Enter", "on a task: edit it; in a side panel: apply that filter"),
        ],
    },
    Section {
        title: "Changing tasks",
        bindings: &[
            b!("a", "add a new task (form with every field on one screen)"),
            b!("e / Enter", "edit the selected task"),
            b!("Space / x", "toggle complete — writes or removes the `x ` prefix"),
            b!("p", "set priority by typing a letter (A-Z, or - to clear)"),
            b!("+ / = / A", "raise priority one step (none → Z … → B → A)"),
            b!("- / _", "lower priority one step (A → B … → Z → none)"),
            b!("c", "set the @context tags of the selected task"),
            b!("P", "set the +project tags of the selected task"),
            b!("d", "set the due:YYYY-MM-DD date"),
            b!("D / Del", "delete the selected task (asks first)"),
            b!("u", "undo the last change"),
        ],
    },
    Section {
        title: "Filtering and searching",
        bindings: &[
            b!("/", "search as you type across the whole task line"),
            b!("f", "filter by a project name you type"),
            b!("@", "filter by a context name you type"),
            b!("t", "filter by the selected task's first +project"),
            b!("T", "filter by the selected task's first @context"),
            b!("v", "cycle visibility: all → open only → done only"),
            b!("s", "cycle sort: file order → priority → due date"),
            b!("F / Esc", "clear every filter and the search term"),
        ],
    },
    Section {
        title: "The file",
        bindings: &[
            b!("Ctrl-K / Ctrl-J", "move the selected task up/down a line in the file"),
            b!("S", "sort the file itself (open first, then priority, then due)"),
            b!("X", "archive completed tasks to done.txt (asks first)"),
            b!("r / F5", "re-read the file from disk"),
        ],
    },
    Section {
        title: "Getting out",
        bindings: &[
            b!("? / F1", "open or close this help"),
            b!("q", "quit (every change is already saved)"),
            b!("Ctrl-C", "quit immediately"),
        ],
    },
    Section {
        title: "While typing in a prompt or form",
        bindings: &[
            b!("Enter", "confirm"),
            b!("Esc", "cancel and go back"),
            b!("Tab / Shift-Tab", "next / previous form field"),
            b!("← → Home End", "move the cursor"),
            b!("Backspace / Del", "delete a character"),
            b!("Ctrl-W", "delete the word before the cursor"),
            b!("Ctrl-U", "clear to the start of the line"),
        ],
    },
];

/// Compact bindings shown in the footer during normal browsing.
pub static FOOTER_NORMAL: &[(&str, &str)] = &[
    ("↑↓", "move"),
    ("Tab", "panel"),
    ("a", "add"),
    ("e", "edit"),
    ("Space", "done"),
    ("p", "priority"),
    ("c", "context"),
    ("P", "project"),
    ("d", "due"),
    ("/", "search"),
    ("f", "filter"),
    ("F", "clear"),
    ("u", "undo"),
    ("?", "help"),
    ("q", "quit"),
];

/// Footer bindings shown while a text prompt is open.
pub static FOOTER_PROMPT: &[(&str, &str)] = &[
    ("Enter", "confirm"),
    ("Esc", "cancel"),
    ("Ctrl-W", "del word"),
    ("Ctrl-U", "clear"),
];

/// Footer bindings shown while a confirmation is pending.
pub static FOOTER_CONFIRM: &[(&str, &str)] = &[("y", "yes"), ("n / Esc", "no")];

/// Footer bindings shown while the add/edit form is open.
pub static FOOTER_FORM: &[(&str, &str)] = &[
    ("Tab", "next field"),
    ("Shift-Tab", "previous field"),
    ("Enter", "save"),
    ("Esc", "cancel"),
];

/// Footer bindings shown on the help screen.
pub static FOOTER_HELP: &[(&str, &str)] = &[
    ("↑↓/PgUp/PgDn", "scroll"),
    ("? / Esc / q", "back to the list"),
];

/// A short orientation paragraph shown at the top of the help screen.
pub static HELP_INTRO: &[&str] = &[
    "tooll edits a todo.txt file in place. Every change is written to disk",
    "immediately — there is no separate save step, and what you see in the list",
    "is exactly what the file contains.",
    "",
    "todo.txt line format:  x (A) 2026-01-02 2026-01-01 Pay rent +finance @computer due:2026-01-05",
    "  x            completed marker (only at the start of the line)",
    "  (A)          priority, A is highest",
    "  dates        completion date then creation date, both YYYY-MM-DD",
    "  +finance     project tag",
    "  @computer    context tag",
    "  due:...      due date",
];

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
                assert!(!b.what.is_empty(), "{} in {}", b.keys, s.title);
            }
        }
    }

    #[test]
    fn footer_hints_are_populated() {
        for set in [
            FOOTER_NORMAL,
            FOOTER_PROMPT,
            FOOTER_CONFIRM,
            FOOTER_FORM,
            FOOTER_HELP,
        ] {
            assert!(!set.is_empty());
            for (k, v) in set {
                assert!(!k.is_empty() && !v.is_empty());
            }
        }
    }

    /// The footer is the user's first look at the key map, so each key it
    /// advertises must also appear in the full help.
    #[test]
    fn footer_keys_are_documented_in_help() {
        let all: String = SECTIONS
            .iter()
            .flat_map(|s| s.bindings.iter())
            .map(|b| b.keys)
            .collect::<Vec<_>>()
            .join(" | ");
        for (key, _) in FOOTER_NORMAL {
            let needle = key.trim();
            assert!(
                all.contains(needle) || needle == "↑↓",
                "footer key `{needle}` is not in the help screen"
            );
        }
    }
}
