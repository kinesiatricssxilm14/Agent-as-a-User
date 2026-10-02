//! The in-program key reference.
//!
//! Every binding the application implements is listed here, so a user can
//! discover the whole interface with F1 and never need external documentation.

/// One line of the help pane.
pub enum Entry {
    /// A section heading.
    Section(&'static str),
    /// A key (or key list) and what it does.
    Key(&'static str, &'static str),
    /// A paragraph of prose.
    Note(&'static str),
    /// Vertical space.
    Blank,
}

use Entry::{Blank, Key, Note, Section};

/// The complete help document.
pub const HELP: &[Entry] = &[
    Note("toole — interactive regular expression tester.  Type a pattern; matches are highlighted in the text and listed with their 0-indexed character offsets as you type."),
    Blank,
    Section("Getting around"),
    Key("Tab / Shift+Tab", "move focus: Pattern → Replace → Matches → Source"),
    Key("Esc", "close help, leave a field, then quit"),
    Key("Ctrl+Q / Ctrl+C", "quit immediately"),
    Key("F1 / ?", "toggle this help (? only outside a text field)"),
    Blank,
    Section("Pattern and replacement fields"),
    Key("any character", "insert it; matching re-runs on every keystroke"),
    Key("← →  Home End", "move the cursor"),
    Key("Ctrl+←/→, Alt+B/F", "move by word"),
    Key("Backspace / Delete", "delete left / right"),
    Key("Ctrl+W", "delete the word before the cursor"),
    Key("Ctrl+U / Ctrl+K", "delete to start / end of the field"),
    Key("Ctrl+A / Ctrl+E", "jump to start / end of the field"),
    Key("Ctrl+L", "clear the pattern field"),
    Key("Enter", "jump to the match list"),
    Blank,
    Section("Match list  (focus: Matches)"),
    Key("↑ ↓  k j", "previous / next match"),
    Key("PageUp / PageDown", "move a screenful"),
    Key("Home End  g G", "first / last match"),
    Key("Enter", "centre the match in the source pane and focus it"),
    Key("Ctrl+N / Ctrl+P", "next / previous match — works from any focus"),
    Key("i or /", "jump to the pattern field"),
    Key("r", "jump to the replacement field"),
    Blank,
    Section("Source pane  (focus: Source)"),
    Key("↑ ↓  k j", "scroll one line"),
    Key("PageUp / PageDown", "scroll one screen"),
    Key("Home End  g G", "top / bottom of the file"),
    Key("← →", "scroll horizontally (when soft wrap is off)"),
    Key("Shift+↑ / Shift+↓", "scroll the source from any focus"),
    Key("K / J", "scroll the replacement preview up / down"),
    Key("n / p", "next / previous match"),
    Blank,
    Section("Regex flags — toggle any time, the scan re-runs at once"),
    Key("F2", "(?i) case-insensitive matching"),
    Key("F3", "(?m) ^ and $ match at line boundaries"),
    Key("F4", "(?s) . also matches a newline"),
    Key("F5", "(?x) ignore whitespace in the pattern"),
    Key("F6", "literal — treat the pattern as plain text"),
    Key("Ctrl+Y", "force the backtracking engine (look-around, back-references)"),
    Note("Inline flags work too: typing (?i)error is equivalent to pressing F2 with the pattern error.  A case-insensitive match may sit in the middle of a word, e.g. (?i)err matches the ERR inside xxERRORxx."),
    Blank,
    Section("Replacement"),
    Key("Tab to Replace", "then type the replacement template"),
    Key("$1 $2 … / ${1}", "insert a numbered capture group ($0 = whole match)"),
    Key("$name / ${name}", "insert a named capture group"),
    Key("$$", "a literal dollar sign"),
    Key("\\n \\t \\r \\\\ \\$", "escape sequences"),
    Key("F7", "show / hide the replacement preview"),
    Key("F8", "replace every match / only the first"),
    Key("Ctrl+S", "write the replaced content to a file"),
    Note("The preview always shows the complete file content, including the lines no match touched."),
    Blank,
    Section("View and files"),
    Key("F9", "soft wrap on / off"),
    Key("F10", "line numbers on / off"),
    Key("F11 / Shift+F11", "next / previous built-in pattern preset"),
    Key("[ / ]", "previous / next preset (outside a text field)"),
    Key("Ctrl+G", "right panel: capture groups ↔ pattern presets"),
    Key("Ctrl+O", "open a different file"),
    Key("Ctrl+R / F12", "reload the current file from disk"),
    Blank,
    Section("Reading the results"),
    Note("Each row of the match list shows: index, the 0-indexed start and end character offsets, the 0-indexed line:column of the start, and the matched text.  end is exclusive, so a match of length n spans start..end with end = start + n."),
    Note("A zero-width match (\\b, x*) is drawn as a coloured caret in the text and listed with start equal to end."),
    Note("Highlight colours — amber: a match; bright amber: the selected match; green: text inserted by the replacement template; purple: a zero-width match."),
    Blank,
    Section("Prompts"),
    Key("Enter", "confirm the path"),
    Key("Esc", "cancel"),
    Key("y / n", "answer a confirmation"),
];

/// Number of lines the help document occupies, for scroll bounds.
pub fn line_count() -> usize {
    HELP.len()
}
