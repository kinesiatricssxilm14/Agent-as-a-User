//! In-memory model of a board.
//!
//! Kept separate from [`crate::store`] so the file-format logic and the shape the UI consumes
//! can be reasoned about independently.

use std::path::{Path, PathBuf};

/// One column of the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// Filesystem identifier. Determines `cols/<id>/`.
    pub id: String,
    /// Human-readable name. This is how the UI and the user address the column.
    pub display_name: String,
    /// Cards in `order.txt` order.
    pub cards: Vec<Card>,
}

impl Column {
    /// Directory backing this column: `<root>/cols/<id>/`.
    #[cfg(test)]
    pub fn dir(&self, root: &Path) -> PathBuf {
        root.join("cols").join(&self.id)
    }

    /// Order file for this column. Used by the tests that pin down the on-disk layout.
    #[cfg(test)]
    pub fn order_file(&self, root: &Path) -> PathBuf {
        self.dir(root).join("order.txt")
    }

    #[cfg(test)]
    pub fn card_index(&self, card_id: &str) -> Option<usize> {
        self.cards.iter().position(|c| c.id == card_id)
    }

    /// Label used wherever a column is named in the UI.
    ///
    /// Display names are meant to be unique, but a hand-edited `board.txt` can contain
    /// duplicates; those columns are disambiguated with their id so the user can still tell
    /// them apart.
    pub fn label(&self, ambiguous: bool) -> String {
        if ambiguous {
            format!("{} ({})", self.display_name, self.id)
        } else {
            self.display_name.clone()
        }
    }
}

/// One card, as loaded from `<card_id>.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// Filename without the `.md` extension.
    pub id: String,
    /// Title from line 1, with the leading `#` markers stripped.
    pub title: String,
    /// Everything after line 1, verbatim. May be empty or span many lines.
    pub body: String,
    /// Set when the file has no `# ` header line; writing a title will prepend one rather than
    /// overwrite the first line of body text.
    pub missing_header: bool,
}

impl Card {
    pub fn path(&self, root: &Path, column_id: &str) -> PathBuf {
        root.join("cols").join(column_id).join(format!("{}.md", self.id))
    }

    /// Body split into lines. An empty body yields no lines.
    #[cfg(test)]
    pub fn body_lines(&self) -> Vec<&str> {
        if self.body.is_empty() {
            Vec::new()
        } else {
            self.body.split('\n').collect()
        }
    }

    /// What to show when a card has no title text.
    pub fn display_title(&self) -> &str {
        if self.title.trim().is_empty() {
            "(untitled)"
        } else {
            &self.title
        }
    }
}

/// A whole board: the column list plus everything needed to write it back.
#[derive(Debug, Clone)]
pub struct Board {
    pub root: PathBuf,
    pub columns: Vec<Column>,
    /// Non-fatal problems found while loading (duplicate ids, unreadable cards, ...).
    /// Surfaced in the UI instead of being silently swallowed.
    pub warnings: Vec<String>,
}

impl Board {
    pub fn column_index(&self, id: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.id == id)
    }

    /// Find a column by display name, case-sensitively first, then case-insensitively.
    ///
    /// The specification addresses columns by display name, so this is the lookup the UI uses
    /// for "go to column" and "move card to column".
    pub fn column_by_display_name(&self, name: &str) -> Option<usize> {
        let name = name.trim();
        self.columns
            .iter()
            .position(|c| c.display_name == name)
            .or_else(|| {
                self.columns
                    .iter()
                    .position(|c| c.display_name.eq_ignore_ascii_case(name))
            })
    }

    /// True when more than one column shares this display name, so labels need the id.
    pub fn is_display_name_ambiguous(&self, name: &str) -> bool {
        self.columns.iter().filter(|c| c.display_name == name).count() > 1
    }

    pub fn total_cards(&self) -> usize {
        self.columns.iter().map(|c| c.cards.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, title: &str, body: &str) -> Card {
        Card { id: id.into(), title: title.into(), body: body.into(), missing_header: false }
    }

    fn board(cols: Vec<Column>) -> Board {
        Board { root: PathBuf::from("/b"), columns: cols, warnings: Vec::new() }
    }

    fn col(id: &str, name: &str, cards: Vec<Card>) -> Column {
        Column { id: id.into(), display_name: name.into(), cards }
    }

    #[test]
    fn paths_use_column_id_not_display_name() {
        let c = col("todo", "TO DO", vec![]);
        let root = Path::new("/bench/data/board");
        assert_eq!(c.dir(root), PathBuf::from("/bench/data/board/cols/todo"));
        assert_eq!(c.order_file(root), PathBuf::from("/bench/data/board/cols/todo/order.txt"));
        assert_eq!(
            card("item-1", "t", "").path(root, "todo"),
            PathBuf::from("/bench/data/board/cols/todo/item-1.md")
        );
    }

    #[test]
    fn lookup_by_display_name_is_case_insensitive_fallback() {
        let b = board(vec![col("todo", "TO DO", vec![]), col("done", "Done", vec![])]);
        assert_eq!(b.column_by_display_name("TO DO"), Some(0));
        assert_eq!(b.column_by_display_name("  Done  "), Some(1));
        assert_eq!(b.column_by_display_name("to do"), Some(0), "case-insensitive fallback");
        assert_eq!(b.column_by_display_name("missing"), None);
    }

    #[test]
    fn exact_match_wins_over_case_insensitive_one() {
        let b = board(vec![col("a", "done", vec![]), col("b", "Done", vec![])]);
        assert_eq!(b.column_by_display_name("Done"), Some(1));
        assert_eq!(b.column_by_display_name("done"), Some(0));
    }

    #[test]
    fn duplicate_display_names_are_flagged_and_labelled_with_ids() {
        let b = board(vec![col("a", "Same", vec![]), col("b", "Same", vec![])]);
        assert!(b.is_display_name_ambiguous("Same"));
        assert_eq!(b.columns[0].label(true), "Same (a)");
        assert_eq!(b.columns[0].label(false), "Same");
    }

    #[test]
    fn body_lines_handles_empty_and_multiline() {
        assert!(card("i", "t", "").body_lines().is_empty());
        assert_eq!(card("i", "t", "one").body_lines(), vec!["one"]);
        assert_eq!(card("i", "t", "a\nb\nc").body_lines(), vec!["a", "b", "c"]);
        // A blank line inside the body is a real line and must be preserved.
        assert_eq!(card("i", "t", "a\n\nb").body_lines(), vec!["a", "", "b"]);
    }

    #[test]
    fn untitled_cards_get_a_placeholder() {
        assert_eq!(card("i", "", "").display_title(), "(untitled)");
        assert_eq!(card("i", "   ", "").display_title(), "(untitled)");
        assert_eq!(card("i", "Real", "").display_title(), "Real");
    }

    #[test]
    fn counts_and_indices() {
        let b = board(vec![
            col("a", "A", vec![card("x", "X", ""), card("y", "Y", "")]),
            col("b", "B", vec![card("z", "Z", "")]),
        ]);
        assert_eq!(b.total_cards(), 3);
        assert_eq!(b.column_index("b"), Some(1));
        assert_eq!(b.columns[0].card_index("y"), Some(1));
    }
}
