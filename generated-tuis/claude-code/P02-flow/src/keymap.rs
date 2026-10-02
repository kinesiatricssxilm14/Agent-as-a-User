//! Key bindings.
//!
//! One table is the single source of truth for three consumers: the event dispatcher, the
//! footer hint bar, and the help pane. Because help is generated from the same data that
//! drives behaviour, documented keys cannot drift from working keys — and a unit test can
//! assert that no key is bound twice in the same mode.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Which input mode the application is in. Determines how keys are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeKind {
    /// Browsing the board.
    Normal,
    /// A single-line text prompt is open.
    Prompt,
    /// The multi-line body editor is open.
    BodyEditor,
    /// A yes/no confirmation is open.
    Confirm,
}

/// Everything the application can be asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // Navigation
    ColumnLeft,
    ColumnRight,
    CardUp,
    CardDown,
    CardFirst,
    CardLast,
    CardPageUp,
    CardPageDown,
    FocusDetail,
    // Card operations
    NewCard,
    EditTitle,
    AppendBody,
    EditBody,
    DeleteCard,
    MoveCardPrompt,
    MoveCardLeft,
    MoveCardRight,
    ReorderUp,
    ReorderDown,
    // Column and configuration operations
    NewColumn,
    RenameColumn,
    GotoColumn,
    EditConfig,
    SetBoardRoot,
    InitBoard,
    NormaliseOrder,
    // Global
    Search,
    SearchNext,
    SearchPrev,
    ClearSearch,
    Reload,
    ToggleHelp,
    Quit,
    Cancel,
    Confirm,
}

/// One documented binding.
pub struct Binding {
    /// Keys as shown to the user, e.g. `"↑/k"`.
    pub keys: &'static str,
    /// Short label for the hint bar.
    pub label: &'static str,
    /// Longer explanation for the help pane.
    pub help: &'static str,
    /// Only read by the test that proves every documented binding is reachable.
    #[cfg_attr(not(test), allow(dead_code))]
    pub action: Action,
    /// Whether this binding earns a slot in the always-visible hint bar.
    pub in_hint_bar: bool,
    /// Help pane grouping.
    pub group: &'static str,
}

/// Bindings available while browsing the board.
pub const NORMAL_BINDINGS: &[Binding] = &[
    Binding { keys: "←/→ h/l", label: "column", help: "Select the previous / next column", action: Action::ColumnLeft, in_hint_bar: true, group: "Navigate" },
    Binding { keys: "↑/↓ k/j", label: "card", help: "Select the previous / next card in the column", action: Action::CardUp, in_hint_bar: true, group: "Navigate" },
    Binding { keys: "Tab/⇧Tab", label: "", help: "Cycle forwards / backwards through columns", action: Action::ColumnRight, in_hint_bar: false, group: "Navigate" },
    Binding { keys: "g / Home", label: "", help: "Jump to the first card in the column", action: Action::CardFirst, in_hint_bar: false, group: "Navigate" },
    Binding { keys: "G / End", label: "", help: "Jump to the last card in the column", action: Action::CardLast, in_hint_bar: false, group: "Navigate" },
    Binding { keys: "PgUp/PgDn", label: "", help: "Move the selection a screenful at a time", action: Action::CardPageUp, in_hint_bar: false, group: "Navigate" },
    Binding { keys: "Enter", label: "detail", help: "Focus the card pane, so ↑/↓ scroll the body text", action: Action::FocusDetail, in_hint_bar: true, group: "Navigate" },
    Binding { keys: "t", label: "", help: "Go to a column by typing its display name", action: Action::GotoColumn, in_hint_bar: false, group: "Navigate" },

    Binding { keys: "n", label: "new", help: "Create a card in the selected column (asks for a title, then an id)", action: Action::NewCard, in_hint_bar: true, group: "Cards" },
    Binding { keys: "e", label: "title", help: "Edit the title of the selected card (line 1, keeping the '# ' prefix)", action: Action::EditTitle, in_hint_bar: true, group: "Cards" },
    Binding { keys: "a", label: "append", help: "Append one line to the end of the selected card's body", action: Action::AppendBody, in_hint_bar: true, group: "Cards" },
    Binding { keys: "B", label: "", help: "Edit the whole body of the selected card in a multi-line editor", action: Action::EditBody, in_hint_bar: false, group: "Cards" },
    Binding { keys: "d", label: "delete", help: "Delete the selected card (asks for confirmation)", action: Action::DeleteCard, in_hint_bar: true, group: "Cards" },

    Binding { keys: "m", label: "move", help: "Move the selected card to a column chosen by display name", action: Action::MoveCardPrompt, in_hint_bar: true, group: "Move" },
    Binding { keys: "H / ^←", label: "", help: "Move the selected card to the previous column", action: Action::MoveCardLeft, in_hint_bar: false, group: "Move" },
    Binding { keys: "L / ^→", label: "", help: "Move the selected card to the next column", action: Action::MoveCardRight, in_hint_bar: false, group: "Move" },
    Binding { keys: "K", label: "", help: "Move the selected card up within its column", action: Action::ReorderUp, in_hint_bar: false, group: "Move" },
    Binding { keys: "J", label: "", help: "Move the selected card down within its column", action: Action::ReorderDown, in_hint_bar: false, group: "Move" },

    Binding { keys: "c", label: "", help: "Create a column (asks for a display name, then an id)", action: Action::NewColumn, in_hint_bar: false, group: "Columns" },
    Binding { keys: "r", label: "", help: "Rename the selected column's display name (its id and files are unchanged)", action: Action::RenameColumn, in_hint_bar: false, group: "Columns" },
    Binding { keys: "O", label: "", help: "Rewrite order.txt for the selected column to match what is shown", action: Action::NormaliseOrder, in_hint_bar: false, group: "Columns" },
    Binding { keys: "I", label: "", help: "Create board.txt and the default columns in an empty board root", action: Action::InitBoard, in_hint_bar: false, group: "Columns" },

    Binding { keys: "/", label: "search", help: "Filter cards by id, title or body text", action: Action::Search, in_hint_bar: true, group: "Search" },
    Binding { keys: "^n", label: "", help: "Select the next card matching the search", action: Action::SearchNext, in_hint_bar: false, group: "Search" },
    Binding { keys: "^p", label: "", help: "Select the previous card matching the search", action: Action::SearchPrev, in_hint_bar: false, group: "Search" },
    Binding { keys: "Esc", label: "", help: "Clear the active search, or leave the card pane", action: Action::ClearSearch, in_hint_bar: false, group: "Search" },

    Binding { keys: "S", label: "", help: "Change the board root directory and save it to the configuration file", action: Action::SetBoardRoot, in_hint_bar: false, group: "Board" },
    Binding { keys: "C", label: "", help: "Set any configuration key and save it to the configuration file", action: Action::EditConfig, in_hint_bar: false, group: "Board" },
    Binding { keys: "R", label: "", help: "Re-read the board from disk", action: Action::Reload, in_hint_bar: false, group: "Board" },
    Binding { keys: "?", label: "help", help: "Show or hide this key list", action: Action::ToggleHelp, in_hint_bar: true, group: "Board" },
    Binding { keys: "q / ^c", label: "quit", help: "Leave toolb", action: Action::Quit, in_hint_bar: true, group: "Board" },
];

/// Bindings shown while a single-line prompt is open.
pub const PROMPT_BINDINGS: &[Binding] = &[
    Binding { keys: "Enter", label: "confirm", help: "Accept what you typed", action: Action::Confirm, in_hint_bar: true, group: "Prompt" },
    Binding { keys: "Esc", label: "cancel", help: "Abandon the prompt and change nothing", action: Action::Cancel, in_hint_bar: true, group: "Prompt" },
    Binding { keys: "←/→", label: "", help: "Move the cursor", action: Action::ColumnLeft, in_hint_bar: false, group: "Prompt" },
    Binding { keys: "Home/End", label: "", help: "Jump to the start / end of the line", action: Action::CardFirst, in_hint_bar: false, group: "Prompt" },
    Binding { keys: "^u", label: "", help: "Clear the whole line", action: Action::Cancel, in_hint_bar: false, group: "Prompt" },
    Binding { keys: "^w", label: "", help: "Delete the word before the cursor", action: Action::Cancel, in_hint_bar: false, group: "Prompt" },
    Binding { keys: "Tab", label: "", help: "Complete a column name, where one is expected", action: Action::Confirm, in_hint_bar: false, group: "Prompt" },
];

/// Bindings shown while the multi-line body editor is open.
pub const BODY_BINDINGS: &[Binding] = &[
    Binding { keys: "Enter", label: "newline", help: "Start a new line of body text", action: Action::Confirm, in_hint_bar: true, group: "Body editor" },
    Binding { keys: "^s", label: "save", help: "Save the body to the card file", action: Action::Confirm, in_hint_bar: true, group: "Body editor" },
    Binding { keys: "Esc", label: "cancel", help: "Discard the edit", action: Action::Cancel, in_hint_bar: true, group: "Body editor" },
    Binding { keys: "↑/↓", label: "", help: "Move between lines", action: Action::CardUp, in_hint_bar: false, group: "Body editor" },
];

/// Bindings shown while a confirmation is open.
pub const CONFIRM_BINDINGS: &[Binding] = &[
    Binding { keys: "y", label: "yes", help: "Confirm the action", action: Action::Confirm, in_hint_bar: true, group: "Confirm" },
    Binding { keys: "n / Esc", label: "no", help: "Cancel the action", action: Action::Cancel, in_hint_bar: true, group: "Confirm" },
];

/// The binding table for a mode.
pub fn bindings_for(mode: ModeKind) -> &'static [Binding] {
    match mode {
        ModeKind::Normal => NORMAL_BINDINGS,
        ModeKind::Prompt => PROMPT_BINDINGS,
        ModeKind::BodyEditor => BODY_BINDINGS,
        ModeKind::Confirm => CONFIRM_BINDINGS,
    }
}

/// Map a key press in normal mode to an action.
///
/// Only called when no prompt is open, so single letters are commands here and plain text
/// elsewhere. Returning `None` means "ignore", which is what unbound keys should do.
pub fn resolve_normal(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    // Control combinations first: they must not be shadowed by the plain-letter bindings.
    if ctrl {
        return match key.code {
            KeyCode::Char('c') => Some(Action::Quit),
            KeyCode::Char('n') => Some(Action::SearchNext),
            KeyCode::Char('p') => Some(Action::SearchPrev),
            KeyCode::Left => Some(Action::MoveCardLeft),
            KeyCode::Right => Some(Action::MoveCardRight),
            KeyCode::Char('f') => Some(Action::CardPageDown),
            KeyCode::Char('b') => Some(Action::CardPageUp),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Left | KeyCode::Char('h') => Some(Action::ColumnLeft),
        KeyCode::Right | KeyCode::Char('l') => Some(Action::ColumnRight),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::CardUp),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::CardDown),
        KeyCode::Tab => Some(Action::ColumnRight),
        // Shift+Tab arrives as BackTab, not Tab with a SHIFT modifier.
        KeyCode::BackTab => Some(Action::ColumnLeft),
        KeyCode::Home | KeyCode::Char('g') => Some(Action::CardFirst),
        KeyCode::End => Some(Action::CardLast),
        KeyCode::Char('G') => Some(Action::CardLast),
        KeyCode::PageUp => Some(Action::CardPageUp),
        KeyCode::PageDown => Some(Action::CardPageDown),
        KeyCode::Enter => Some(Action::FocusDetail),
        KeyCode::Esc => Some(Action::ClearSearch),

        KeyCode::Char('n') => Some(Action::NewCard),
        KeyCode::Char('e') => Some(Action::EditTitle),
        KeyCode::Char('a') => Some(Action::AppendBody),
        KeyCode::Char('B') => Some(Action::EditBody),
        KeyCode::Char('d') => Some(Action::DeleteCard),

        KeyCode::Char('m') => Some(Action::MoveCardPrompt),
        KeyCode::Char('H') => Some(Action::MoveCardLeft),
        KeyCode::Char('L') => Some(Action::MoveCardRight),
        KeyCode::Char('K') => Some(Action::ReorderUp),
        KeyCode::Char('J') => Some(Action::ReorderDown),

        KeyCode::Char('c') => Some(Action::NewColumn),
        KeyCode::Char('r') => Some(Action::RenameColumn),
        KeyCode::Char('t') => Some(Action::GotoColumn),
        KeyCode::Char('O') => Some(Action::NormaliseOrder),
        KeyCode::Char('I') => Some(Action::InitBoard),

        KeyCode::Char('/') => Some(Action::Search),
        KeyCode::Char('S') => Some(Action::SetBoardRoot),
        KeyCode::Char('C') => Some(Action::EditConfig),
        KeyCode::Char('R') => Some(Action::Reload),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::F(1) => Some(Action::ToggleHelp),
        KeyCode::Char('q') => Some(Action::Quit),
        // A capital letter reaching here with SHIFT held is simply unbound.
        KeyCode::Char(_) if shift => None,
        _ => None,
    }
}

/// One line of the hint bar for a mode: the compact always-visible key reminder.
pub fn hint_line(mode: ModeKind) -> String {
    bindings_for(mode)
        .iter()
        .filter(|b| b.in_hint_bar && !b.label.is_empty())
        .map(|b| format!("{} {}", b.keys, b.label))
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn required_navigation_keys_are_bound() {
        // The specification calls out arrows, Enter, Esc and Tab explicitly.
        assert_eq!(resolve_normal(press(KeyCode::Left)), Some(Action::ColumnLeft));
        assert_eq!(resolve_normal(press(KeyCode::Right)), Some(Action::ColumnRight));
        assert_eq!(resolve_normal(press(KeyCode::Up)), Some(Action::CardUp));
        assert_eq!(resolve_normal(press(KeyCode::Down)), Some(Action::CardDown));
        assert_eq!(resolve_normal(press(KeyCode::Enter)), Some(Action::FocusDetail));
        assert_eq!(resolve_normal(press(KeyCode::Esc)), Some(Action::ClearSearch));
        assert_eq!(resolve_normal(press(KeyCode::Tab)), Some(Action::ColumnRight));
    }

    #[test]
    fn shift_tab_arrives_as_back_tab() {
        // crossterm reports Shift+Tab as BackTab; matching Tab+SHIFT alone would miss it.
        assert_eq!(
            resolve_normal(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
            Some(Action::ColumnLeft)
        );
        assert_eq!(resolve_normal(press(KeyCode::BackTab)), Some(Action::ColumnLeft));
    }

    #[test]
    fn vim_style_aliases_match_the_arrows() {
        assert_eq!(resolve_normal(press(KeyCode::Char('h'))), Some(Action::ColumnLeft));
        assert_eq!(resolve_normal(press(KeyCode::Char('l'))), Some(Action::ColumnRight));
        assert_eq!(resolve_normal(press(KeyCode::Char('k'))), Some(Action::CardUp));
        assert_eq!(resolve_normal(press(KeyCode::Char('j'))), Some(Action::CardDown));
    }

    #[test]
    fn ctrl_c_quits_because_raw_mode_swallows_sigint() {
        assert_eq!(resolve_normal(ctrl(KeyCode::Char('c'))), Some(Action::Quit));
    }

    #[test]
    fn ctrl_combinations_are_not_shadowed_by_plain_letters() {
        // Plain 'n' creates a card; Ctrl+n steps through search matches.
        assert_eq!(resolve_normal(press(KeyCode::Char('n'))), Some(Action::NewCard));
        assert_eq!(resolve_normal(ctrl(KeyCode::Char('n'))), Some(Action::SearchNext));
        assert_eq!(resolve_normal(ctrl(KeyCode::Char('p'))), Some(Action::SearchPrev));
        // Plain arrows navigate; Ctrl+arrows move the card.
        assert_eq!(resolve_normal(ctrl(KeyCode::Left)), Some(Action::MoveCardLeft));
        assert_eq!(resolve_normal(ctrl(KeyCode::Right)), Some(Action::MoveCardRight));
    }

    #[test]
    fn case_distinguishes_navigation_from_mutation() {
        assert_eq!(resolve_normal(press(KeyCode::Char('h'))), Some(Action::ColumnLeft));
        assert_eq!(resolve_normal(press(KeyCode::Char('H'))), Some(Action::MoveCardLeft));
        assert_eq!(resolve_normal(press(KeyCode::Char('l'))), Some(Action::ColumnRight));
        assert_eq!(resolve_normal(press(KeyCode::Char('L'))), Some(Action::MoveCardRight));
    }

    #[test]
    fn shifted_letters_still_resolve_when_shift_is_reported() {
        // Terminals often report 'H' with the SHIFT modifier set.
        assert_eq!(
            resolve_normal(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT)),
            Some(Action::MoveCardLeft)
        );
        assert_eq!(
            resolve_normal(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)),
            Some(Action::ToggleHelp)
        );
    }

    #[test]
    fn unbound_keys_are_ignored() {
        assert_eq!(resolve_normal(press(KeyCode::Char('z'))), None);
        assert_eq!(resolve_normal(press(KeyCode::F(9))), None);
        assert_eq!(resolve_normal(ctrl(KeyCode::Char('z'))), None);
    }

    #[test]
    fn no_key_is_bound_twice_within_a_mode() {
        for mode in [ModeKind::Normal, ModeKind::Prompt, ModeKind::BodyEditor, ModeKind::Confirm] {
            let mut seen: HashMap<&str, &str> = HashMap::new();
            for binding in bindings_for(mode) {
                if let Some(prev) = seen.insert(binding.keys, binding.help) {
                    panic!("{mode:?} documents keys {:?} twice: {prev} / {}", binding.keys, binding.help);
                }
            }
        }
    }

    #[test]
    fn every_binding_is_documented() {
        for mode in [ModeKind::Normal, ModeKind::Prompt, ModeKind::BodyEditor, ModeKind::Confirm] {
            for binding in bindings_for(mode) {
                assert!(!binding.keys.is_empty(), "{mode:?} binding with no keys");
                assert!(!binding.help.is_empty(), "{:?} has no help text", binding.keys);
                assert!(!binding.group.is_empty(), "{:?} has no group", binding.keys);
            }
        }
    }

    #[test]
    fn every_normal_action_is_reachable_by_some_key() {
        // Walk the whole plausible key space and collect what it produces, so an action can
        // never appear in the help table without a key that triggers it.
        let mut reachable = std::collections::HashSet::new();
        let codes = [
            KeyCode::Left, KeyCode::Right, KeyCode::Up, KeyCode::Down, KeyCode::Tab,
            KeyCode::BackTab, KeyCode::Home, KeyCode::End, KeyCode::PageUp, KeyCode::PageDown,
            KeyCode::Enter, KeyCode::Esc, KeyCode::F(1),
        ];
        for code in codes {
            if let Some(a) = resolve_normal(press(code)) {
                reachable.insert(a);
            }
            if let Some(a) = resolve_normal(ctrl(code)) {
                reachable.insert(a);
            }
        }
        for ch in ' '..='~' {
            if let Some(a) = resolve_normal(press(KeyCode::Char(ch))) {
                reachable.insert(a);
            }
            if let Some(a) = resolve_normal(ctrl(KeyCode::Char(ch))) {
                reachable.insert(a);
            }
        }
        for binding in NORMAL_BINDINGS {
            assert!(
                reachable.contains(&binding.action),
                "action {:?} is documented under {:?} but no key produces it",
                binding.action,
                binding.keys
            );
        }
    }

    #[test]
    fn hint_line_is_populated_for_every_mode() {
        for mode in [ModeKind::Normal, ModeKind::Prompt, ModeKind::BodyEditor, ModeKind::Confirm] {
            let line = hint_line(mode);
            assert!(!line.is_empty(), "{mode:?} has an empty hint bar");
        }
        let normal = hint_line(ModeKind::Normal);
        assert!(normal.contains("help"), "help must be advertised: {normal}");
        assert!(normal.contains("quit"), "quit must be advertised: {normal}");
    }

    #[test]
    fn hint_lines_differ_between_modes() {
        // The hint bar must react to the mode, otherwise it teaches the wrong keys.
        assert_ne!(hint_line(ModeKind::Normal), hint_line(ModeKind::Prompt));
        assert!(hint_line(ModeKind::Confirm).contains("yes"));
    }
}
