//! Key dispatch. Modals are checked before the browse-mode bindings so that a
//! form's text entry can't be shadowed by a global single-letter shortcut.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{App, Modal, View};
use crate::db::Kind;
use crate::form::{FieldKind, TextInput};

/// Route one key event to the layer that currently has focus.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Ignore key-up/repeat records; on Windows and some terminals both edges
    // are reported and every action would otherwise fire twice.
    if key.kind == KeyEventKind::Release {
        return;
    }

    // Ctrl-C always quits, whatever has focus.
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
    {
        app.should_quit = true;
        return;
    }

    // The search prompt takes precedence: it is a text field.
    if app.search_input.is_some() {
        handle_search(app, key);
        return;
    }

    match &app.modal {
        Modal::Form { .. } => handle_form(app, key),
        Modal::Detail { id } => handle_detail(app, key, *id),
        Modal::Confirm(_) => handle_confirm(app, key),
        Modal::Help => handle_help(app, key),
        Modal::Browse => handle_browse(app, key),
    }
}

// ---------------------------------------------------------------------------
// Browse mode
// ---------------------------------------------------------------------------

fn handle_browse(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        // -- quitting and help
        KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,
        KeyCode::Char('?') | KeyCode::F(1) => app.toggle_help(),

        // -- view switching
        KeyCode::Tab => app.next_view(),
        KeyCode::BackTab => app.prev_view(),
        KeyCode::Char(c @ '1'..='5') => {
            if let Some(view) = View::from_digit(c) {
                app.goto(view);
            }
        }

        // -- list movement
        KeyCode::Down | KeyCode::Char('j') => movement(app, Movement::Next),
        KeyCode::Up | KeyCode::Char('k') => movement(app, Movement::Prev),
        KeyCode::PageDown | KeyCode::Char('J') => movement(app, Movement::PageDown),
        KeyCode::PageUp | KeyCode::Char('K') => movement(app, Movement::PageUp),
        KeyCode::Home | KeyCode::Char('g') => movement(app, Movement::First),
        KeyCode::End | KeyCode::Char('G') => movement(app, Movement::Last),

        // -- month stepping. The Ctrl-guarded arms come first so that Ctrl+L
        // (clear search) is not swallowed by the plain 'l' binding.
        KeyCode::Char('l') if ctrl => app.clear_search(),
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('[') => app.shift_month(-1),
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(']') => app.shift_month(1),
        KeyCode::Char('T') => app.jump_to_current_month(),

        // -- filtering
        KeyCode::Char('m') => {
            app.goto(View::Transactions);
            app.toggle_month_filter();
        }
        KeyCode::Char('/') => {
            app.goto(View::Transactions);
            app.search_input = Some(TextInput::with_value(
                app.search.clone().unwrap_or_default(),
            ));
            app.clear_message();
        }
        KeyCode::Esc => {
            if app.search.is_some() {
                app.clear_search();
            } else {
                app.clear_message();
            }
        }

        // -- creating records
        KeyCode::Char('n') | KeyCode::Char('a') => new_in_current_view(app),
        KeyCode::Char('i') => app.open_transaction_form(Kind::Income, None),
        KeyCode::Char('e') | KeyCode::Char('x') => app.open_transaction_form(Kind::Expense, None),
        KeyCode::Char('b') => {
            // In BUDGETS this edits the highlighted budget, which is what "set
            // a budget" means when one already exists for that category.
            let existing = if app.view == View::Budgets {
                app.selected_budget().cloned()
            } else {
                None
            };
            app.open_budget_form(existing);
        }

        // -- editing and deleting
        KeyCode::Char('E') | KeyCode::F(2) => edit_selected(app),
        KeyCode::Char('d') | KeyCode::Delete => app.request_delete(),

        // -- opening a row
        KeyCode::Enter => match app.view {
            View::Transactions => app.open_detail(),
            View::Summary => app.goto(View::Transactions),
            _ => edit_selected(app),
        },

        // -- reload
        KeyCode::Char('r') | KeyCode::F(5) => {
            app.reload();
            app.info("Reloaded from the database");
        }

        _ => {}
    }
}

enum Movement {
    Next,
    Prev,
    PageDown,
    PageUp,
    First,
    Last,
}

/// In SUMMARY the arrow keys scroll the overview; elsewhere they move the
/// list cursor.
fn movement(app: &mut App, m: Movement) {
    if app.view == View::Summary {
        match m {
            Movement::Next => app.scroll_down(1),
            Movement::Prev => app.scroll_up(1),
            Movement::PageDown => app.scroll_down(10),
            Movement::PageUp => app.scroll_up(10),
            Movement::First => app.scroll = 0,
            Movement::Last => app.scroll_down(10),
        }
        return;
    }
    match m {
        Movement::Next => app.select_next(),
        Movement::Prev => app.select_prev(),
        Movement::PageDown => app.select_page(10),
        Movement::PageUp => app.select_page(-10),
        Movement::First => app.select_first(),
        Movement::Last => app.select_last(),
    }
}

/// `n` means "new whatever this view lists".
fn new_in_current_view(app: &mut App) {
    match app.view {
        View::Accounts => app.open_account_form(None),
        View::Categories => app.open_category_form(None),
        View::Transactions => app.open_transaction_form(Kind::Expense, None),
        View::Budgets => app.open_budget_form(None),
        View::Summary => app.open_transaction_form(Kind::Expense, None),
    }
}

fn edit_selected(app: &mut App) {
    match app.view {
        View::Accounts => match app.selected_account().cloned() {
            Some(a) => app.open_account_form(Some(a)),
            None => app.info("No account selected — press n to create one"),
        },
        View::Categories => match app.selected_category().cloned() {
            Some(c) => app.open_category_form(Some(c)),
            None => app.info("No category selected — press n to create one"),
        },
        View::Transactions => match app.selected_transaction().cloned() {
            Some(t) => app.open_transaction_form(t.kind, Some(t)),
            None => app.info("No transaction selected — press i or e to add one"),
        },
        View::Budgets => match app.selected_budget().cloned() {
            Some(b) => app.open_budget_form(Some(b)),
            None => app.info("No budget selected — press n to set one"),
        },
        View::Summary => app.info("SUMMARY is a read-only overview — press 2 for TRANSACTIONS"),
    }
}

// ---------------------------------------------------------------------------
// Forms
// ---------------------------------------------------------------------------

fn handle_form(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Save and cancel are handled before the field, so Enter/Esc are never
    // swallowed as text.
    match key.code {
        KeyCode::Esc => {
            app.close_modal();
            app.info("Cancelled — nothing was saved");
            return;
        }
        KeyCode::Enter => {
            app.submit_form();
            return;
        }
        KeyCode::Tab | KeyCode::Down => {
            if let Modal::Form { form, .. } = &mut app.modal {
                form.focus_next();
            }
            return;
        }
        KeyCode::BackTab | KeyCode::Up => {
            if let Modal::Form { form, .. } = &mut app.modal {
                form.focus_prev();
            }
            return;
        }
        _ => {}
    }

    if let Modal::Form { form, .. } = &mut app.modal {
        let field = form.focused_mut();

        match key.code {
            // -- choice fields
            KeyCode::Left if field.is_choice() => field.prev_option(),
            KeyCode::Right if field.is_choice() => field.next_option(),

            // -- text cursor movement
            KeyCode::Left => field.input.move_left(),
            KeyCode::Right => field.input.move_right(),
            KeyCode::Home => field.input.move_home(),
            KeyCode::End => field.input.move_end(),

            // -- editing
            KeyCode::Backspace => field.input.delete_backward(),
            KeyCode::Delete => field.input.delete_forward(),
            KeyCode::Char('w') if ctrl => field.input.delete_word_backward(),
            KeyCode::Char('u') if ctrl => field.input.delete_to_start(),
            KeyCode::Char('k') if ctrl => field.input.delete_to_end(),

            // -- stepping dates and months.
            //
            // PageUp/PageDown rather than +/- because '-' is part of the date
            // syntax itself: binding it to "step back" would make a date like
            // 2026-02-30 impossible to type.
            KeyCode::PageUp => step_period(field, 1),
            KeyCode::PageDown => step_period(field, -1),

            // -- plain text entry
            KeyCode::Char(c) if !ctrl => {
                if field.is_choice() {
                    // Let a letter jump to the next option starting with it, so
                    // long account/category lists are quick to navigate.
                    jump_choice(field, c);
                } else {
                    field.input.insert(c);
                }
            }
            _ => {}
        }
    }
}

/// PageUp/PageDown on a Date field steps one day; on a Month field, one month.
/// Other field kinds ignore it.
fn step_period(field: &mut crate::form::Field, delta: i32) {
    match field.kind {
        FieldKind::Date => {
            if let Ok(current) = crate::date::parse_date(field.input.value()) {
                if let Some(next) = crate::date::shift_day(&current, delta) {
                    field.input.set_value(next);
                }
            }
        }
        FieldKind::Month => {
            if let Ok(current) = crate::date::parse_month(field.input.value()) {
                field
                    .input
                    .set_value(crate::date::shift_month(&current, delta));
            }
        }
        _ => {}
    }
}

/// Type-to-select within a choice field.
fn jump_choice(field: &mut crate::form::Field, c: char) {
    let needle = c.to_lowercase().next().unwrap_or(c);
    let len = field.options.len();
    if len == 0 {
        return;
    }
    // Start after the current selection so repeated presses cycle matches.
    for step in 1..=len {
        let idx = (field.selected + step) % len;
        let label = field.options[idx].1.to_lowercase();
        if label.starts_with(needle) {
            field.selected = idx;
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Detail, confirm, help, search
// ---------------------------------------------------------------------------

fn handle_detail(app: &mut App, key: KeyEvent, id: i64) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
            app.close_modal();
        }
        KeyCode::Char('?') | KeyCode::F(1) => app.toggle_help(),
        KeyCode::Char('E') | KeyCode::F(2) => {
            if let Some(t) = app.detail_transaction(id) {
                app.open_transaction_form(t.kind, Some(t));
            } else {
                app.close_modal();
                app.error("That transaction no longer exists");
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            app.close_modal();
            app.request_delete();
        }
        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(1),
        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(1),
        KeyCode::PageDown => app.scroll_down(10),
        KeyCode::PageUp => app.scroll_up(10),
        // Step through the list without going back to it first.
        KeyCode::Char('n') => {
            app.close_modal();
            app.select_next();
            app.open_detail();
        }
        KeyCode::Char('p') => {
            app.close_modal();
            app.select_prev();
            app.open_detail();
        }
        _ => {}
    }
}

fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => app.confirm_delete(),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.close_modal();
            app.info("Cancelled — nothing was deleted");
        }
        _ => {}
    }
}

fn handle_help(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::F(1)
        | KeyCode::Enter => {
            app.close_modal();
        }
        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(1),
        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(1),
        KeyCode::PageDown => app.scroll_down(10),
        KeyCode::PageUp => app.scroll_up(10),
        KeyCode::Home | KeyCode::Char('g') => app.scroll = 0,
        _ => {}
    }
}

fn handle_search(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let Some(input) = app.search_input.as_mut() else { return };

    match key.code {
        KeyCode::Esc => {
            app.search_input = None;
            app.info("Search cancelled");
        }
        KeyCode::Enter => {
            let query = input.trimmed().to_string();
            app.search_input = None;
            if query.is_empty() {
                app.search = None;
                app.info("Search cleared");
            } else {
                app.search = Some(query.clone());
                app.select_first();
                let hits = app.visible_transactions().len();
                app.info(format!("{hits} transaction(s) match '{query}'"));
            }
            app.reload();
        }
        KeyCode::Backspace => input.delete_backward(),
        KeyCode::Delete => input.delete_forward(),
        KeyCode::Left => input.move_left(),
        KeyCode::Right => input.move_right(),
        KeyCode::Home => input.move_home(),
        KeyCode::End => input.move_end(),
        KeyCode::Char('w') if ctrl => input.delete_word_backward(),
        KeyCode::Char('u') if ctrl => input.delete_to_start(),
        KeyCode::Char(c) if !ctrl => input.insert(c),
        _ => {}
    }
}
