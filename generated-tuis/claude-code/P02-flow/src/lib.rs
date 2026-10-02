//! toolb - a keyboard-driven kanban board TUI over a plain-text board directory.
//!
//! The board lives entirely in files: `board.txt` declares the columns, and each column is a
//! directory of Markdown cards with an `order.txt` recording their order. Every operation in the
//! interface reads or writes those files directly, so the board can be inspected, edited by hand,
//! or version-controlled with ordinary tools.
//!
//! The crate is exposed as a library as well as a binary so the integration tests can drive the
//! real application state and render real frames, rather than re-implementing either.

pub mod app;
pub mod cli;
pub mod config;
pub mod input;
pub mod keymap;
pub mod model;
pub mod slug;
pub mod store;
pub mod ui;
