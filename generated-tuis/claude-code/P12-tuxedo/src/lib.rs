//! `tooll` — an interactive todo.txt task manager for the terminal.
//!
//! The crate is split so that everything except the terminal plumbing is
//! testable in-process:
//!
//! * [`task`] — todo.txt parsing, mutation and serialisation
//! * [`store`] — loading and atomically writing the task file
//! * [`date`] — the small calendar helpers the format needs
//! * [`config`] — argument, environment and config-file resolution
//! * [`input`] — the single-line editor shared by all prompts
//! * [`app`] — application state and every key binding
//! * [`keys`] — the one canonical key/documentation table
//! * [`ui`] — ratatui rendering, a pure function of [`app::App`]

pub mod app;
pub mod config;
pub mod date;
pub mod input;
pub mod keys;
pub mod store;
pub mod task;
pub mod ui;
