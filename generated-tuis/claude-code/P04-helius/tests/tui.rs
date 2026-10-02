//! End-to-end tests that drive the real application through key events and
//! assert on what is actually rendered to a terminal buffer.
//!
//! These are the tests that matter most for a TUI: they verify that a user
//! pressing documented keys reaches the documented result, that the amounts on
//! screen carry two decimal places, and that the "same-screen visibility"
//! requirement holds — income, expense and net all present in one snapshot.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use toold::app::{App, Modal, View};
use toold::config::{Config, DbSource};
use toold::db::Store;
use toold::{input, ui};

/// A wide-enough terminal that no column is truncated away.
const WIDTH: u16 = 160;
const HEIGHT: u16 = 48;

struct Harness {
    app: App,
    terminal: Terminal<TestBackend>,
    _dir: TempDir,
}

/// A temp directory removed on drop, so tests leave nothing behind.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        // Include the tag and a counter so parallel tests never collide.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "toold-it-{}-{}-{n}",
            std::process::id(),
            tag
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Harness {
    fn new(tag: &str) -> Self {
        let dir = TempDir::new(tag);
        let db_path = dir.0.join("ledger.db");
        let store = Store::open(&db_path).expect("open store");
        let config = Config {
            db_path,
            db_source: DbSource::CliFlag,
            config_path: None,
        };
        Self {
            app: App::new(store, config),
            terminal: Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("terminal"),
            _dir: dir,
        }
    }

    /// Press a key with no modifiers.
    fn key(&mut self, code: KeyCode) -> &mut Self {
        input::handle_key(&mut self.app, KeyEvent::new(code, KeyModifiers::NONE));
        self
    }

    /// Press a printable character.
    fn ch(&mut self, c: char) -> &mut Self {
        self.key(KeyCode::Char(c))
    }

    /// Type a whole string, one character at a time, as a user would.
    fn type_str(&mut self, s: &str) -> &mut Self {
        for c in s.chars() {
            self.ch(c);
        }
        self
    }

    fn enter(&mut self) -> &mut Self {
        self.key(KeyCode::Enter)
    }

    fn tab(&mut self) -> &mut Self {
        self.key(KeyCode::Tab)
    }

    fn esc(&mut self) -> &mut Self {
        self.key(KeyCode::Esc)
    }

    /// Render a frame and return it as plain text, one line per row.
    fn screen(&mut self) -> String {
        let app = &self.app;
        self.terminal
            .draw(|frame| ui::draw(frame, app))
            .expect("draw");

        let buffer = self.terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    /// The status/message line text, used to assert on operation feedback.
    fn message(&self) -> String {
        self.app
            .message
            .as_ref()
            .map(|(t, _)| t.clone())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

#[test]
fn opens_on_summary_with_all_views_reachable() {
    let mut h = Harness::new("startup");
    let screen = h.screen();

    // Every main view is advertised in the tab bar, with its jump digit.
    for view in View::ALL {
        assert!(
            screen.contains(view.title()),
            "tab bar omits {}:\n{screen}",
            view.title()
        );
    }
    assert_eq!(h.app.view, View::Summary, "opens on SUMMARY");

    // A first-run ledger shows zeroes, not blanks.
    assert!(screen.contains("Total income"), "no income label:\n{screen}");
    assert!(screen.contains("Total expense"), "no expense label:\n{screen}");
    assert!(screen.contains("Net income"), "no net label:\n{screen}");
    assert!(screen.contains("0.00"), "zero amounts not shown:\n{screen}");
}

#[test]
fn digit_keys_and_tab_switch_views() {
    let mut h = Harness::new("nav");

    for view in View::ALL {
        h.ch(view.digit());
        assert_eq!(h.app.view, view, "digit {} should open {}", view.digit(), view.title());
        let screen = h.screen();
        assert!(
            screen.contains(view.title()),
            "{} not rendered after pressing {}:\n{screen}",
            view.title(),
            view.digit()
        );
    }

    // Tab cycles forward and wraps.
    h.ch('1');
    for expected in [View::Transactions, View::Accounts, View::Categories, View::Budgets, View::Summary] {
        h.tab();
        assert_eq!(h.app.view, expected);
    }

    // Shift+Tab goes back.
    h.key(KeyCode::BackTab);
    assert_eq!(h.app.view, View::Budgets);
}

#[test]
fn help_is_reachable_and_documents_the_keys() {
    let mut h = Harness::new("help");
    h.ch('?');
    let screen = h.screen();

    assert!(matches!(h.app.modal, Modal::Help));
    assert!(screen.contains("HELP"), "no help title:\n{screen}");
    // The help must let a user discover the core verbs without a manual.
    for needle in ["Quit", "New record", "Delete the highlighted", "Search", "help"] {
        assert!(screen.contains(needle), "help omits '{needle}':\n{screen}");
    }
    // And it must say where the data lives.
    assert!(screen.contains("Database"), "help omits the db path:\n{screen}");
    assert!(screen.contains("TOOLD_DB"), "help omits the override:\n{screen}");

    // '?' closes it again.
    h.ch('?');
    assert!(matches!(h.app.modal, Modal::Browse));
}

// ---------------------------------------------------------------------------
// Accounts and categories
// ---------------------------------------------------------------------------

#[test]
fn creates_an_account_through_the_ui() {
    let mut h = Harness::new("acct");
    h.ch('3').ch('n');
    h.type_str("personal");
    h.tab().type_str("-savings"); // appended to the "checking" default
    h.enter();

    let screen = h.screen();
    assert!(screen.contains("personal"), "account not listed:\n{screen}");
    assert!(h.message().contains("Created account"), "no feedback: {}", h.message());

    // It really is in the database, not just on screen.
    let accounts = h.app.store.accounts().expect("query accounts");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].name, "personal");
}

#[test]
fn rejects_an_account_with_no_name() {
    let mut h = Harness::new("acct-empty");
    h.ch('3').ch('n').enter();

    // The form stays open with an error rather than saving a nameless account.
    assert!(matches!(h.app.modal, Modal::Form { .. }), "form closed on invalid input");
    assert!(h.message().contains("required"), "no validation message: {}", h.message());
    assert!(h.app.store.accounts().unwrap().is_empty(), "saved an invalid account");
}

#[test]
fn creates_income_and_expense_categories() {
    let mut h = Harness::new("cats");

    h.ch('4').ch('n').type_str("dining").enter();
    h.ch('4').ch('n').type_str("salary");
    h.tab().key(KeyCode::Right); // switch Kind to income
    h.enter();

    let screen = h.screen();
    assert!(screen.contains("dining"), "dining missing:\n{screen}");
    assert!(screen.contains("salary"), "salary missing:\n{screen}");

    let cats = h.app.store.categories().unwrap();
    assert_eq!(cats.len(), 2);
    let dining = cats.iter().find(|c| c.name == "dining").unwrap();
    let salary = cats.iter().find(|c| c.name == "salary").unwrap();
    assert_eq!(dining.kind, toold::db::Kind::Expense);
    assert_eq!(salary.kind, toold::db::Kind::Income, "Right should select income");
}

// ---------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------

/// Set up an account plus one income and one expense category.
fn seeded(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.ch('3').ch('n').type_str("personal").enter();
    h.ch('4').ch('n').type_str("dining").enter();
    h.ch('4').ch('n').type_str("salary").tab().key(KeyCode::Right);
    h.enter();
    h
}

#[test]
fn records_an_expense_with_two_decimal_places() {
    let mut h = seeded("expense");

    // e = new expense. Fields: Amount, Account, Category, Date, Payee, Notes.
    h.ch('e');
    h.type_str("800.05");
    h.tab(); // Account (only one, already selected)
    h.tab(); // Category
    h.key(KeyCode::Right); // pick 'dining' (first option is "(uncategorized)")
    h.tab(); // Date (pre-filled with today)
    h.tab(); // Payee
    h.type_str("Blue Bottle");
    h.tab(); // Notes
    h.type_str("team offsite");
    h.enter();

    assert!(h.message().contains("Recorded"), "no feedback: {}", h.message());

    let screen = h.screen();
    assert!(screen.contains("800.05"), "amount not shown with 2dp:\n{screen}");
    assert!(screen.contains("Blue Bottle"), "payee missing:\n{screen}");
    assert!(screen.contains("dining"), "category missing:\n{screen}");

    let txns = h.app.store.transactions(None).unwrap();
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].amount_cents, 80_005, "stored as integer cents");
    assert_eq!(txns[0].payee, "Blue Bottle");
    assert_eq!(txns[0].notes, "team offsite");
    assert_eq!(txns[0].category_name, "dining");
}

#[test]
fn records_income_and_reflects_it_in_the_summary() {
    let mut h = seeded("income");

    h.ch('i');
    h.type_str("4303.33");
    h.tab().tab();
    h.key(KeyCode::Right); // 'salary' — income form lists income categories
    h.enter();

    let txns = h.app.store.transactions(None).unwrap();
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].kind, toold::db::Kind::Income);
    assert_eq!(txns[0].amount_cents, 430_333);
    assert_eq!(txns[0].category_name, "salary", "income form must offer income categories");

    let screen = h.ch('1').screen();
    assert!(screen.contains("4303.33"), "income not in summary:\n{screen}");
}

#[test]
fn rejects_an_amount_with_too_much_precision() {
    let mut h = seeded("precision");
    h.ch('e').type_str("12.345").enter();

    assert!(matches!(h.app.modal, Modal::Form { .. }), "form closed on invalid amount");
    assert!(
        h.message().contains("2 decimal places"),
        "unhelpful message: {}",
        h.message()
    );
    assert!(h.app.store.transactions(None).unwrap().is_empty(), "saved a bad amount");

    // The form also shows the problem inline, before submission.
    let screen = h.screen();
    assert!(screen.contains("decimal places"), "no inline hint:\n{screen}");
}

#[test]
fn rejects_an_impossible_date() {
    let mut h = seeded("baddate");
    h.ch('e').type_str("10.00");
    h.tab().tab().tab(); // to Date
    // Clear the pre-filled date and type an invalid one.
    for _ in 0..12 {
        h.key(KeyCode::Backspace);
    }
    h.type_str("2026-02-30");
    h.enter();

    assert!(matches!(h.app.modal, Modal::Form { .. }), "form closed on invalid date");
    assert!(h.message().contains("real calendar date"), "message: {}", h.message());
    assert!(h.app.store.transactions(None).unwrap().is_empty());
}

#[test]
fn escape_cancels_a_form_without_saving() {
    let mut h = seeded("cancel");
    h.ch('e').type_str("999.99").esc();

    assert!(matches!(h.app.modal, Modal::Browse));
    assert!(h.message().contains("Cancelled"), "message: {}", h.message());
    assert!(h.app.store.transactions(None).unwrap().is_empty(), "cancelled form still saved");
}

// ---------------------------------------------------------------------------
// Transaction detail — the same-screen visibility requirement
// ---------------------------------------------------------------------------

#[test]
fn detail_shows_every_field_on_one_screen() {
    let mut h = seeded("detail");

    h.ch('e');
    h.type_str("1234.56");
    h.tab().tab().key(KeyCode::Right); // category = dining
    h.tab(); // Date
    for _ in 0..12 {
        h.key(KeyCode::Backspace);
    }
    h.type_str("2026-08-13");
    h.tab().type_str("Corner Store");
    h.tab().type_str("weekly groceries");
    h.enter();

    // Open the highlighted transaction.
    h.ch('2').enter();
    assert!(matches!(h.app.modal, Modal::Detail { .. }), "Enter did not open the detail");

    // Every field the spec names must appear in this single snapshot.
    let screen = h.screen();
    for (label, value) in [
        ("Amount", "1234.56"),
        ("Account", "personal"),
        ("Category", "dining"),
        ("Date", "2026-08-13"),
        ("Payee", "Corner Store"),
        ("Notes", "weekly groceries"),
    ] {
        assert!(screen.contains(label), "detail omits the {label} label:\n{screen}");
        assert!(screen.contains(value), "detail omits the {label} value '{value}':\n{screen}");
    }
    // And the direction, since amount alone is ambiguous.
    assert!(screen.contains("Expense"), "detail omits the kind:\n{screen}");

    // Esc returns to the list.
    h.esc();
    assert!(matches!(h.app.modal, Modal::Browse));
}

#[test]
fn detail_survives_a_missing_payee_and_notes() {
    let mut h = seeded("detail-sparse");
    h.ch('e').type_str("5.00").enter();
    h.ch('2').enter();

    let screen = h.screen();
    assert!(screen.contains("5.00"), "amount missing:\n{screen}");
    // Empty optional fields are still labelled, so the layout is stable.
    assert!(screen.contains("Payee"), "payee label missing:\n{screen}");
    assert!(screen.contains("Notes"), "notes label missing:\n{screen}");
    assert!(screen.contains("(uncategorized)"), "no category placeholder:\n{screen}");
}

// ---------------------------------------------------------------------------
// SUMMARY
// ---------------------------------------------------------------------------

#[test]
fn summary_shows_income_expense_and_net_together() {
    let mut h = seeded("summary");

    // Income 4303.33, expense 714.91 → net 3588.42.
    h.ch('i').type_str("4303.33").tab().tab().key(KeyCode::Right);
    h.enter();
    h.ch('e').type_str("714.91").tab().tab().key(KeyCode::Right);
    h.enter();

    let screen = h.ch('1').screen();

    // All three figures in one snapshot, each with two decimals.
    assert!(screen.contains("4303.33"), "income missing:\n{screen}");
    assert!(screen.contains("714.91"), "expense missing:\n{screen}");
    assert!(screen.contains("3588.42"), "net missing:\n{screen}");

    // Labelled, so the numbers are identifiable.
    assert!(screen.contains("Total income"));
    assert!(screen.contains("Total expense"));
    assert!(screen.contains("Net income"));

    // The database agrees with the screen.
    let sum = h.app.store.monthly_summary(&h.app.month).unwrap();
    assert_eq!(sum.income_cents, 430_333);
    assert_eq!(sum.expense_cents, 71_491);
    assert_eq!(sum.net_cents(), 358_842);
}

#[test]
fn summary_shows_a_negative_net_when_overspending() {
    let mut h = seeded("summary-neg");
    h.ch('i').type_str("100.00").tab().tab().key(KeyCode::Right);
    h.enter();
    h.ch('e').type_str("250.50").tab().tab().key(KeyCode::Right);
    h.enter();

    let screen = h.ch('1').screen();
    assert!(screen.contains("-150.50"), "negative net not shown:\n{screen}");
}

// ---------------------------------------------------------------------------
// BUDGETS
// ---------------------------------------------------------------------------

#[test]
fn budget_shows_total_spent_and_remaining_together() {
    let mut h = seeded("budget");

    // Budget 1200.00 for dining; spend 156.78 → remaining 1043.22.
    h.ch('b');
    // Fields: Category, Amount, Month, Account, Notes.
    h.type_str("d"); // type-to-select the 'dining' option
    h.tab().type_str("1200.00");
    h.enter();
    assert!(h.message().contains("Set"), "no feedback: {}", h.message());

    h.ch('e').type_str("156.78").tab().tab().key(KeyCode::Right);
    h.enter();

    let screen = h.ch('5').screen();
    assert!(screen.contains("1200.00"), "budget total missing:\n{screen}");
    assert!(screen.contains("156.78"), "spent missing:\n{screen}");
    assert!(screen.contains("1043.22"), "remaining missing:\n{screen}");

    // Column headers make the three figures unambiguous.
    assert!(screen.contains("BUDGET"), "no BUDGET column:\n{screen}");
    assert!(screen.contains("SPENT"), "no SPENT column:\n{screen}");
    assert!(screen.contains("REMAINING"), "no REMAINING column:\n{screen}");

    let budgets = h.app.store.budgets(&h.app.month).unwrap();
    assert_eq!(budgets.len(), 1);
    assert_eq!(budgets[0].amount_cents, 120_000);
    assert_eq!(budgets[0].spent_cents, 15_678);
    assert_eq!(budgets[0].remaining_cents(), 104_322);
}

#[test]
fn overspent_budget_is_flagged_with_a_negative_remainder() {
    let mut h = seeded("budget-over");
    h.ch('b').type_str("d").tab().type_str("100.00");
    h.enter();
    h.ch('e').type_str("150.50").tab().tab().key(KeyCode::Right);
    h.enter();

    let screen = h.ch('5').screen();
    assert!(screen.contains("-50.50"), "negative remainder missing:\n{screen}");
    assert!(screen.contains("OVER"), "overspend not flagged:\n{screen}");
}

#[test]
fn resetting_a_budget_updates_it_in_place() {
    let mut h = seeded("budget-upsert");
    h.ch('b').type_str("d").tab().type_str("100.00").enter();

    // Saving lands on BUDGETS, where 'b' edits the highlighted budget. The
    // amount field arrives pre-filled, so clear it before typing the new value.
    h.ch('b').tab();
    for _ in 0..10 {
        h.key(KeyCode::Backspace);
    }
    h.type_str("500.00").enter();

    let budgets = h.app.store.budgets(&h.app.month).unwrap();
    assert_eq!(budgets.len(), 1, "re-setting a budget must not duplicate it");
    assert_eq!(budgets[0].amount_cents, 50_000);

    let screen = h.ch('5').screen();
    assert!(screen.contains("500.00"), "updated amount missing:\n{screen}");
}

#[test]
fn appending_to_a_prefilled_amount_is_rejected_not_silently_saved() {
    // Guards the edit-form pre-fill behaviour: typing over a pre-filled amount
    // without clearing produces an invalid amount, which must be refused.
    let mut h = seeded("budget-append");
    h.ch('b').type_str("d").tab().type_str("100.00").enter();

    h.ch('b').tab().type_str("500.00").enter();
    assert!(
        matches!(h.app.modal, Modal::Form { .. }),
        "an unparseable amount must keep the form open"
    );
    assert_eq!(
        h.app.store.budgets(&h.app.month).unwrap()[0].amount_cents,
        10_000,
        "the stored budget must be untouched"
    );
}

// ---------------------------------------------------------------------------
// Editing, deleting, filtering
// ---------------------------------------------------------------------------

#[test]
fn edits_a_transaction_amount() {
    let mut h = seeded("edit");
    h.ch('e').type_str("10.00").enter();

    h.ch('2').ch('E');
    assert!(matches!(h.app.modal, Modal::Form { .. }), "E did not open the edit form");
    // The form is pre-filled with the stored value.
    let screen = h.screen();
    assert!(screen.contains("10.00"), "edit form not pre-filled:\n{screen}");

    for _ in 0..8 {
        h.key(KeyCode::Backspace);
    }
    h.type_str("42.50").enter();

    let txns = h.app.store.transactions(None).unwrap();
    assert_eq!(txns.len(), 1, "edit must not create a second record");
    assert_eq!(txns[0].amount_cents, 4250);
}

#[test]
fn deletes_a_transaction_only_after_confirmation() {
    let mut h = seeded("delete");
    h.ch('e').type_str("10.00").enter();
    h.ch('2');

    // Declining keeps the record.
    h.ch('d');
    assert!(matches!(h.app.modal, Modal::Confirm(_)), "d did not ask for confirmation");
    let screen = h.screen();
    assert!(screen.contains("CONFIRM DELETE"), "no confirmation prompt:\n{screen}");
    h.ch('n');
    assert_eq!(h.app.store.transactions(None).unwrap().len(), 1, "declining still deleted");

    // Confirming removes it.
    h.ch('d').ch('y');
    assert!(h.app.store.transactions(None).unwrap().is_empty(), "confirmed delete did nothing");
    assert!(h.message().contains("deleted"), "message: {}", h.message());
}

#[test]
fn searching_filters_the_transaction_list() {
    let mut h = seeded("search");
    h.ch('e').type_str("11.00").tab().tab().tab().tab().type_str("Alpha Cafe");
    h.enter();
    h.ch('e').type_str("22.00").tab().tab().tab().tab().type_str("Beta Market");
    h.enter();

    h.ch('2').ch('/');
    h.type_str("alpha"); // case-insensitive
    h.enter();

    let screen = h.screen();
    assert!(screen.contains("Alpha Cafe"), "match filtered out:\n{screen}");
    assert!(!screen.contains("Beta Market"), "non-match still shown:\n{screen}");
    assert_eq!(h.app.visible_transactions().len(), 1);

    // Both records are still in the database; only the view is filtered.
    assert_eq!(h.app.store.transactions(None).unwrap().len(), 2);

    // Esc clears the filter.
    h.esc();
    assert_eq!(h.app.visible_transactions().len(), 2, "Esc did not clear the search");
}

#[test]
fn month_stepping_changes_the_period_shown() {
    let mut h = seeded("months");
    let start = h.app.month.clone();

    h.key(KeyCode::Left);
    assert_ne!(h.app.month, start, "Left did not step the month");
    let stepped = h.app.month.clone();
    assert!(h.screen().contains(&stepped), "new month not rendered");

    h.key(KeyCode::Right);
    assert_eq!(h.app.month, start, "Right did not step back");

    // T returns to the current month from anywhere.
    h.key(KeyCode::Left).key(KeyCode::Left).ch('T');
    assert_eq!(h.app.month, toold::date::current_month());
}

#[test]
fn month_filter_toggle_limits_the_transaction_list() {
    let mut h = seeded("month-filter");

    // One record this month, one in a different month.
    h.ch('e').type_str("10.00").enter();
    h.ch('e').type_str("20.00");
    h.tab().tab().tab(); // to Date
    for _ in 0..12 {
        h.key(KeyCode::Backspace);
    }
    h.type_str("2020-01-15").enter();

    h.ch('2');
    assert_eq!(h.app.visible_transactions().len(), 2, "all months shown by default");

    h.ch('m'); // limit to the browsed month
    assert!(h.app.filter_by_month);
    assert_eq!(h.app.visible_transactions().len(), 1, "month filter had no effect");

    h.ch('m');
    assert_eq!(h.app.visible_transactions().len(), 2, "toggle did not restore all months");
}

// ---------------------------------------------------------------------------
// Persistence and robustness
// ---------------------------------------------------------------------------

#[test]
fn data_written_through_the_ui_persists_to_sqlite() {
    let dir = TempDir::new("persist");
    let db_path = dir.0.join("ledger.db");

    {
        let store = Store::open(&db_path).unwrap();
        let config = Config {
            db_path: db_path.clone(),
            db_source: DbSource::CliFlag,
            config_path: None,
        };
        let mut app = App::new(store, config);
        let mut term = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();

        // Drive the UI, not the store, so this covers the whole path.
        for key in ['3', 'n'] {
            input::handle_key(&mut app, KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
        }
        for c in "personal".chars() {
            input::handle_key(&mut app, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        input::handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        term.draw(|f| ui::draw(f, &app)).unwrap();
    }

    // A brand-new process would see the same file.
    let store = Store::open(&db_path).unwrap();
    let accounts = store.accounts().unwrap();
    assert_eq!(accounts.len(), 1, "the account did not reach the database file");
    assert_eq!(accounts[0].name, "personal");
    // The file is a real SQLite database on disk.
    assert!(db_path.exists(), "no database file was created");
}

#[test]
fn renders_at_a_small_terminal_size_without_panicking() {
    let mut h = seeded("small");
    h.ch('e').type_str("1234.56").tab().tab().key(KeyCode::Right);
    h.enter();
    h.ch('b').type_str("d").tab().type_str("100.00").enter();

    // Every view and modal must survive a cramped terminal.
    for (w, hgt) in [(40u16, 12u16), (20, 8), (80, 24), (200, 60)] {
        h.terminal = Terminal::new(TestBackend::new(w, hgt)).unwrap();
        for digit in ['1', '2', '3', '4', '5'] {
            h.ch(digit);
            h.screen();
        }
        h.ch('2').enter();
        h.screen(); // detail
        h.esc();
        h.ch('e');
        h.screen(); // form
        h.esc();
        h.ch('?');
        h.screen(); // help
        h.ch('?');
        h.ch('d');
        h.screen(); // confirm
        h.ch('n');
    }
}

#[test]
fn quits_on_q_and_ctrl_c() {
    let mut h = Harness::new("quit");
    assert!(!h.app.should_quit);
    h.ch('q');
    assert!(h.app.should_quit, "q did not quit");

    let mut h = Harness::new("quit-ctrlc");
    input::handle_key(
        &mut h.app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    );
    assert!(h.app.should_quit, "Ctrl+C did not quit");
}

#[test]
fn guides_the_user_when_no_account_exists_yet() {
    let mut h = Harness::new("guide");
    // Recording an expense with no account must explain what to do, not crash.
    h.ch('e');
    assert!(matches!(h.app.modal, Modal::Browse), "opened a form with no account");
    assert!(
        h.message().contains("account"),
        "unhelpful message: {}",
        h.message()
    );

    // The empty ACCOUNTS view names the key that creates one.
    let screen = h.ch('3').screen();
    assert!(screen.contains("Press n"), "no onboarding hint:\n{screen}");
}

#[test]
fn footer_hint_is_always_present() {
    let mut h = seeded("footer");
    // Whatever has focus, the bottom of the screen documents some keys.
    for setup in ["1", "2", "3", "4", "5"] {
        h.ch(setup.chars().next().unwrap());
        let screen = h.screen();
        assert!(screen.contains("? help"), "no help hint on {setup}:\n{screen}");
    }

    h.ch('e');
    let screen = h.screen();
    assert!(screen.contains("Esc cancel"), "form hint missing:\n{screen}");
    h.esc();

    h.ch('?');
    let screen = h.screen();
    assert!(screen.contains("scroll"), "help hint missing:\n{screen}");
}
