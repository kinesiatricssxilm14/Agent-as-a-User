//! Application state: which view is active, what is selected in it, and which
//! modal (form, detail, confirm, help) is layered on top.
//!
//! Every mutation that touches the ledger goes straight to [`crate::db::Store`]
//! and is followed by a reload, so the state here is a cache of the last query
//! rather than a second source of truth.

use crate::config::Config;
use crate::date;
use crate::db::{Account, BudgetRow, Category, CategoryTotal, Kind, MonthlySummary, Store, Transaction};
use crate::form::{Field, Form};

/// The main views, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Summary,
    Transactions,
    Accounts,
    Categories,
    Budgets,
}

impl View {
    pub const ALL: [View; 5] = [
        View::Summary,
        View::Transactions,
        View::Accounts,
        View::Categories,
        View::Budgets,
    ];

    /// Uppercase names match the wording used by task descriptions.
    pub fn title(self) -> &'static str {
        match self {
            View::Summary => "SUMMARY",
            View::Transactions => "TRANSACTIONS",
            View::Accounts => "ACCOUNTS",
            View::Categories => "CATEGORIES",
            View::Budgets => "BUDGETS",
        }
    }

    /// The digit that jumps straight to this view.
    pub fn digit(self) -> char {
        match self {
            View::Summary => '1',
            View::Transactions => '2',
            View::Accounts => '3',
            View::Categories => '4',
            View::Budgets => '5',
        }
    }

    pub fn index(self) -> usize {
        View::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }

    pub fn from_digit(c: char) -> Option<View> {
        View::ALL.iter().copied().find(|v| v.digit() == c)
    }

    pub fn next(self) -> View {
        View::ALL[(self.index() + 1) % View::ALL.len()]
    }

    pub fn prev(self) -> View {
        let i = self.index();
        View::ALL[if i == 0 { View::ALL.len() - 1 } else { i - 1 }]
    }
}

/// Which record a form is creating or editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormTarget {
    Account { id: Option<i64> },
    Category { id: Option<i64> },
    Transaction { id: Option<i64>, kind: Kind },
    Budget { id: Option<i64> },
}

impl FormTarget {
    pub fn title(self) -> String {
        let editing = match self {
            FormTarget::Account { id }
            | FormTarget::Category { id }
            | FormTarget::Transaction { id, .. }
            | FormTarget::Budget { id } => id.is_some(),
        };
        let verb = if editing { "Edit" } else { "New" };
        match self {
            FormTarget::Account { .. } => format!("{verb} account"),
            FormTarget::Category { .. } => format!("{verb} category"),
            FormTarget::Transaction { kind, .. } => format!("{verb} {}", kind.as_str()),
            FormTarget::Budget { .. } => format!("{verb} budget"),
        }
    }
}

/// A pending destructive action, held until the user confirms.
#[derive(Debug, Clone)]
pub struct Confirm {
    pub prompt: String,
    pub detail: String,
    pub action: ConfirmAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    DeleteAccount(i64),
    DeleteCategory(i64),
    DeleteTransaction(i64),
    DeleteBudget(i64),
}

/// The layer currently on top. `Browse` means a main view has focus.
pub enum Modal {
    Browse,
    /// A create/edit form. Boxed to keep `Modal` small.
    Form { target: FormTarget, form: Box<Form> },
    /// Full detail of one transaction, rendered in the main area (not an
    /// overlay) so every field stays visible on the same screen.
    Detail { id: i64 },
    Confirm(Confirm),
    Help,
}

/// Severity of the status-bar message, which selects its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Success,
    Error,
}

/// Everything loaded for the current month/filter, refreshed by [`App::reload`].
#[derive(Default)]
pub struct Data {
    pub accounts: Vec<Account>,
    pub categories: Vec<Category>,
    pub transactions: Vec<Transaction>,
    pub budgets: Vec<BudgetRow>,
    pub summary: MonthlySummary,
    pub category_totals: Vec<CategoryTotal>,
}

pub struct App {
    pub store: Store,
    pub config: Config,
    pub currency: String,

    pub view: View,
    pub modal: Modal,
    pub should_quit: bool,

    /// Month driving SUMMARY, BUDGETS and (when enabled) the transaction list.
    pub month: String,
    /// When true the transaction list is limited to `month`.
    pub filter_by_month: bool,
    /// Case-insensitive substring filter for the transaction list.
    pub search: Option<String>,
    /// Set while the search prompt is open, so keystrokes go to the query.
    pub search_input: Option<crate::form::TextInput>,

    pub data: Data,
    /// Selected row per view, kept independently so switching tabs preserves
    /// where the user was.
    pub selection: [usize; 5],
    /// Vertical scroll offset for the SUMMARY and help views.
    pub scroll: u16,

    pub message: Option<(String, MessageKind)>,

    // A write happens before the reload, so the row to highlight afterwards is
    // recorded here and resolved by `apply_pending_selection`.
    pending_select_account: Option<String>,
    pending_select_category: Option<(String, Kind)>,
    pending_select_transaction: Option<i64>,
    pending_select_budget: Option<i64>,
}

impl App {
    pub fn new(store: Store, config: Config) -> Self {
        let currency = store.currency();
        let mut app = Self {
            store,
            config,
            currency,
            view: View::Summary,
            modal: Modal::Browse,
            should_quit: false,
            month: date::current_month(),
            filter_by_month: false,
            search: None,
            search_input: None,
            data: Data::default(),
            selection: [0; 5],
            scroll: 0,
            message: None,
            pending_select_account: None,
            pending_select_category: None,
            pending_select_transaction: None,
            pending_select_budget: None,
        };
        app.reload();
        app
    }

    // -- messages ----------------------------------------------------------

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), MessageKind::Info));
    }

    pub fn success(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), MessageKind::Success));
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), MessageKind::Error));
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    // -- data --------------------------------------------------------------

    /// Re-query everything the current screen needs. Cheap enough at personal
    /// ledger scale to run after every mutation, which keeps the UI honest.
    pub fn reload(&mut self) {
        let month_filter = if self.filter_by_month {
            Some(self.month.clone())
        } else {
            None
        };

        match self.store.accounts() {
            Ok(v) => self.data.accounts = v,
            Err(e) => self.error(format!("could not load accounts: {e}")),
        }
        match self.store.categories() {
            Ok(v) => self.data.categories = v,
            Err(e) => self.error(format!("could not load categories: {e}")),
        }
        match self.store.transactions(month_filter.as_deref()) {
            Ok(v) => self.data.transactions = v,
            Err(e) => self.error(format!("could not load transactions: {e}")),
        }
        match self.store.budgets(&self.month) {
            Ok(v) => self.data.budgets = v,
            Err(e) => self.error(format!("could not load budgets: {e}")),
        }
        match self.store.monthly_summary(&self.month) {
            Ok(v) => self.data.summary = v,
            Err(e) => self.error(format!("could not load summary: {e}")),
        }
        match self.store.category_totals(&self.month) {
            Ok(v) => self.data.category_totals = v,
            Err(e) => self.error(format!("could not load category totals: {e}")),
        }

        self.apply_pending_selection();
        self.clamp_selection();
    }

    /// Transactions after the search filter, which is applied in memory so the
    /// query stays a plain month filter.
    pub fn visible_transactions(&self) -> Vec<&Transaction> {
        let needle = self.search.as_deref().map(str::to_lowercase);
        self.data
            .transactions
            .iter()
            .filter(|t| match needle.as_deref() {
                None => true,
                Some(n) => {
                    t.payee.to_lowercase().contains(n)
                        || t.notes.to_lowercase().contains(n)
                        || t.category_name.to_lowercase().contains(n)
                        || t.account_name.to_lowercase().contains(n)
                        || t.date.contains(n)
                        || crate::money::format_cents(t.amount_cents).contains(n)
                }
            })
            .collect()
    }

    /// Number of rows in the list of the active view.
    pub fn row_count(&self) -> usize {
        match self.view {
            View::Summary => self.data.category_totals.len(),
            View::Transactions => self.visible_transactions().len(),
            View::Accounts => self.data.accounts.len(),
            View::Categories => self.data.categories.len(),
            View::Budgets => self.data.budgets.len(),
        }
    }

    pub fn selected(&self) -> usize {
        self.selection[self.view.index()]
    }

    pub fn set_selected(&mut self, index: usize) {
        let idx = self.view.index();
        self.selection[idx] = index;
    }

    fn clamp_selection(&mut self) {
        for view in View::ALL {
            let count = match view {
                View::Summary => self.data.category_totals.len(),
                View::Transactions => self.visible_transactions().len(),
                View::Accounts => self.data.accounts.len(),
                View::Categories => self.data.categories.len(),
                View::Budgets => self.data.budgets.len(),
            };
            let i = view.index();
            if count == 0 {
                self.selection[i] = 0;
            } else if self.selection[i] >= count {
                self.selection[i] = count - 1;
            }
        }
    }

    // -- selection helpers -------------------------------------------------

    pub fn select_next(&mut self) {
        let count = self.row_count();
        if count == 0 {
            return;
        }
        let next = (self.selected() + 1) % count;
        self.set_selected(next);
    }

    pub fn select_prev(&mut self) {
        let count = self.row_count();
        if count == 0 {
            return;
        }
        let cur = self.selected();
        self.set_selected(if cur == 0 { count - 1 } else { cur - 1 });
    }

    pub fn select_first(&mut self) {
        self.set_selected(0);
    }

    pub fn select_last(&mut self) {
        let count = self.row_count();
        self.set_selected(count.saturating_sub(1));
    }

    pub fn select_page(&mut self, delta: i32) {
        let count = self.row_count() as i32;
        if count == 0 {
            return;
        }
        let target = (self.selected() as i32 + delta).clamp(0, count - 1);
        self.set_selected(target as usize);
    }

    pub fn selected_account(&self) -> Option<&Account> {
        self.data.accounts.get(self.selection[View::Accounts.index()])
    }

    pub fn selected_category(&self) -> Option<&Category> {
        self.data.categories.get(self.selection[View::Categories.index()])
    }

    pub fn selected_transaction(&self) -> Option<&Transaction> {
        let idx = self.selection[View::Transactions.index()];
        self.visible_transactions().get(idx).copied()
    }

    pub fn selected_budget(&self) -> Option<&BudgetRow> {
        self.data.budgets.get(self.selection[View::Budgets.index()])
    }

    // -- month / filters ---------------------------------------------------

    pub fn shift_month(&mut self, delta: i32) {
        self.month = date::shift_month(&self.month, delta);
        self.reload();
        let label = date::month_label(&self.month);
        self.info(format!("Showing {label} ({})", self.month));
    }

    pub fn set_month(&mut self, month: String) {
        self.month = month;
        self.reload();
    }

    pub fn jump_to_current_month(&mut self) {
        self.month = date::current_month();
        self.reload();
        self.info(format!("Jumped to {}", date::month_label(&self.month)));
    }

    pub fn toggle_month_filter(&mut self) {
        self.filter_by_month = !self.filter_by_month;
        self.reload();
        if self.filter_by_month {
            self.info(format!("Transactions limited to {}", self.month));
        } else {
            self.info("Showing transactions from all months");
        }
    }

    pub fn clear_search(&mut self) {
        if self.search.take().is_some() {
            self.clamp_selection();
            self.info("Search filter cleared");
        }
    }

    // -- view switching ----------------------------------------------------

    pub fn goto(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.scroll = 0;
            self.clear_message();
        }
        self.modal = Modal::Browse;
    }

    pub fn next_view(&mut self) {
        let v = self.view.next();
        self.goto(v);
    }

    pub fn prev_view(&mut self) {
        let v = self.view.prev();
        self.goto(v);
    }

    // -- option lists for forms -------------------------------------------

    fn account_options(&self) -> Vec<(Option<i64>, String)> {
        self.data
            .accounts
            .iter()
            .map(|a| (Some(a.id), format!("{} ({})", a.name, a.account_type)))
            .collect()
    }

    fn account_options_with_all(&self) -> Vec<(Option<i64>, String)> {
        let mut opts = vec![(None, "All accounts".to_string())];
        opts.extend(self.account_options());
        opts
    }

    fn category_options(&self, kind: Kind) -> Vec<(Option<i64>, String)> {
        let mut opts = vec![(None, "(uncategorized)".to_string())];
        opts.extend(
            self.data
                .categories
                .iter()
                .filter(|c| c.kind == kind)
                .map(|c| (Some(c.id), c.name.clone())),
        );
        opts
    }

    fn all_category_options(&self) -> Vec<(Option<i64>, String)> {
        self.data
            .categories
            .iter()
            .map(|c| (Some(c.id), format!("{} [{}]", c.name, c.kind.as_str())))
            .collect()
    }

    // -- opening forms -----------------------------------------------------

    pub fn open_account_form(&mut self, existing: Option<Account>) {
        let (id, name, ty, notes) = match existing {
            Some(a) => (Some(a.id), a.name, a.account_type, a.notes),
            None => (None, String::new(), "checking".to_string(), String::new()),
        };
        let form = Form::new(vec![
            Field::text("Name", true, "Account name, e.g. personal or business")
                .with_value(&name),
            Field::text("Type", false, "Account type, e.g. checking, savings, cash, credit")
                .with_value(&ty),
            Field::text("Notes", false, "Optional note about this account").with_value(&notes),
        ]);
        self.modal = Modal::Form { target: FormTarget::Account { id }, form: Box::new(form) };
        self.clear_message();
    }

    pub fn open_category_form(&mut self, existing: Option<Category>) {
        let (id, name, kind, notes) = match existing {
            Some(c) => (Some(c.id), c.name, c.kind, c.notes),
            None => (None, String::new(), Kind::Expense, String::new()),
        };
        let form = Form::new(vec![
            Field::text("Name", true, "Category name, e.g. dining, transport, salary")
                .with_value(&name),
            Field::choice(
                "Kind",
                vec![
                    (Some(0), "expense".to_string()),
                    (Some(1), "income".to_string()),
                ],
                true,
                "←/→ to switch between expense and income",
            )
            .select_id(Some(match kind {
                Kind::Expense => 0,
                Kind::Income => 1,
            })),
            Field::text("Notes", false, "Optional note about this category").with_value(&notes),
        ]);
        self.modal = Modal::Form { target: FormTarget::Category { id }, form: Box::new(form) };
        self.clear_message();
    }

    /// Open the transaction form. `existing` edits a record; otherwise a new
    /// record of `kind` is created.
    pub fn open_transaction_form(&mut self, kind: Kind, existing: Option<Transaction>) {
        if self.data.accounts.is_empty() {
            self.error("Create an account first (press 3 for ACCOUNTS, then n)");
            return;
        }

        let kind = existing.as_ref().map(|t| t.kind).unwrap_or(kind);
        let payee_label = if kind == Kind::Income { "Source" } else { "Payee" };
        let payee_hint = if kind == Kind::Income {
            "Who the money came from, e.g. an employer or client"
        } else {
            "Who was paid, e.g. a shop or landlord"
        };

        let (id, amount, account_id, category_id, date_value, payee, notes) = match &existing {
            Some(t) => (
                Some(t.id),
                crate::money::format_cents(t.amount_cents),
                Some(t.account_id),
                t.category_id,
                t.date.clone(),
                t.payee.clone(),
                t.notes.clone(),
            ),
            None => (
                None,
                String::new(),
                self.data.accounts.first().map(|a| a.id),
                None,
                self.default_date(),
                String::new(),
                String::new(),
            ),
        };

        let form = Form::new(vec![
            Field::amount("Amount", "Amount in yuan, e.g. 1234.56 (two decimal places)")
                .with_value(&amount),
            Field::choice(
                "Account",
                self.account_options(),
                true,
                "←/→ to pick the account this record belongs to",
            )
            .select_id(account_id),
            Field::choice(
                "Category",
                self.category_options(kind),
                false,
                "←/→ to pick a category (optional)",
            )
            .select_id(category_id),
            Field::date("Date", &date_value, "YYYY-MM-DD; PgUp/PgDn steps a day, t = today")
                .with_value(&date_value),
            Field::text(payee_label, false, payee_hint).with_value(&payee),
            Field::text("Notes", false, "Optional free-text note").with_value(&notes),
        ]);

        self.modal = Modal::Form {
            target: FormTarget::Transaction { id, kind },
            form: Box::new(form),
        };
        self.clear_message();
    }

    /// Default date for a new transaction: today when viewing the current
    /// month, otherwise the first of the month being browsed.
    fn default_date(&self) -> String {
        if self.month == date::current_month() {
            date::today()
        } else {
            format!("{}-01", self.month)
        }
    }

    pub fn open_budget_form(&mut self, existing: Option<BudgetRow>) {
        if self.data.categories.is_empty() {
            self.error("Create a category first (press 4 for CATEGORIES, then n)");
            return;
        }

        let (id, category_id, account_id, month, amount, notes) = match &existing {
            Some(b) => (
                Some(b.id),
                Some(b.category_id),
                b.account_id,
                b.month.clone(),
                crate::money::format_cents(b.amount_cents),
                b.notes.clone(),
            ),
            None => (
                None,
                self.selected_category_id_for_budget(),
                None,
                self.month.clone(),
                String::new(),
                String::new(),
            ),
        };

        let form = Form::new(vec![
            Field::choice(
                "Category",
                self.all_category_options(),
                true,
                "←/→ to pick the category this budget covers",
            )
            .select_id(category_id),
            Field::amount("Amount", "Monthly budget in yuan, e.g. 1200.00")
                .with_value(&amount),
            Field::month("Month", &month, "YYYY-MM; PgUp/PgDn steps a month").with_value(&month),
            Field::choice(
                "Account",
                self.account_options_with_all(),
                false,
                "←/→ to limit the budget to one account, or leave on All accounts",
            )
            .select_id(account_id),
            Field::text("Notes", false, "Optional note about this budget").with_value(&notes),
        ]);

        self.modal = Modal::Form { target: FormTarget::Budget { id }, form: Box::new(form) };
        self.clear_message();
    }

    /// Pre-pick a sensible category when opening a new budget: whatever is
    /// highlighted in CATEGORIES, else the first expense category.
    fn selected_category_id_for_budget(&self) -> Option<i64> {
        if self.view == View::Categories {
            if let Some(c) = self.selected_category() {
                return Some(c.id);
            }
        }
        self.data
            .categories
            .iter()
            .find(|c| c.kind == Kind::Expense)
            .or_else(|| self.data.categories.first())
            .map(|c| c.id)
    }

    // -- submitting forms --------------------------------------------------

    /// Validate and persist the open form. On success the modal closes, the
    /// data reloads, and the affected row is reselected.
    pub fn submit_form(&mut self) {
        let (target, mut form) = match std::mem::replace(&mut self.modal, Modal::Browse) {
            Modal::Form { target, form } => (target, form),
            other => {
                self.modal = other;
                return;
            }
        };

        if let Err(e) = form.validate() {
            self.error(e);
            self.modal = Modal::Form { target, form };
            return;
        }

        let outcome = self.persist_form(target, &form);
        match outcome {
            Ok(message) => {
                // Land on the view that lists what was just saved, so the new
                // record is visible rather than filed away out of sight.
                self.view = match target {
                    FormTarget::Account { .. } => View::Accounts,
                    FormTarget::Category { .. } => View::Categories,
                    FormTarget::Transaction { .. } => View::Transactions,
                    FormTarget::Budget { .. } => View::Budgets,
                };
                self.scroll = 0;
                self.reload();
                self.success(message);
            }
            Err(e) => {
                self.error(e);
                self.modal = Modal::Form { target, form };
            }
        }
    }

    /// Write one submitted form to the database, returning the status message.
    fn persist_form(&mut self, target: FormTarget, form: &Form) -> Result<String, String> {
        match target {
            FormTarget::Account { id } => {
                let name = form.field(0).input.trimmed().to_string();
                let ty = form.field(1).input.trimmed().to_string();
                let notes = form.field(2).input.trimmed().to_string();
                match id {
                    Some(id) => {
                        self.store.update_account(id, &name, &ty, &notes)?;
                        self.select_account_by_name(&name);
                        Ok(format!("Updated account '{name}'"))
                    }
                    None => {
                        self.store.create_account(&name, &ty, &notes)?;
                        self.select_account_by_name(&name);
                        Ok(format!("Created account '{name}'"))
                    }
                }
            }

            FormTarget::Category { id } => {
                let name = form.field(0).input.trimmed().to_string();
                let kind = match form.field(1).selected_id() {
                    Some(1) => Kind::Income,
                    _ => Kind::Expense,
                };
                let notes = form.field(2).input.trimmed().to_string();
                match id {
                    Some(id) => {
                        self.store.update_category(id, &name, kind, &notes)?;
                        self.select_category_by_name(&name, kind);
                        Ok(format!("Updated {} category '{name}'", kind.as_str()))
                    }
                    None => {
                        self.store.create_category(&name, kind, &notes)?;
                        self.select_category_by_name(&name, kind);
                        Ok(format!("Created {} category '{name}'", kind.as_str()))
                    }
                }
            }

            FormTarget::Transaction { id, kind } => {
                let amount = form.field(0).amount_cents()?;
                let account_id = form
                    .field(1)
                    .selected_id()
                    .ok_or_else(|| "select an account for this record".to_string())?;
                let category_id = form.field(2).selected_id();
                let date = form.field(3).date_value()?;
                let payee = form.field(4).input.trimmed().to_string();
                let notes = form.field(5).input.trimmed().to_string();

                let new_id = match id {
                    Some(id) => {
                        self.store.update_transaction(
                            id, kind, amount, account_id, category_id, &date, &payee, &notes,
                        )?;
                        id
                    }
                    None => self.store.create_transaction(
                        kind, amount, account_id, category_id, &date, &payee, &notes,
                    )?,
                };

                // Make the saved record visible: a date outside the current
                // month filter would otherwise vanish from the list.
                if self.filter_by_month && date::month_of(&date) != self.month {
                    self.month = date::month_of(&date);
                }
                self.search = None;
                self.pending_select_transaction = Some(new_id);

                let verb = if id.is_some() { "Updated" } else { "Recorded" };
                Ok(format!(
                    "{verb} {} of {} {} on {date}",
                    kind.as_str(),
                    self.currency,
                    crate::money::format_cents(amount)
                ))
            }

            FormTarget::Budget { .. } => {
                let category_id = form
                    .field(0)
                    .selected_id()
                    .ok_or_else(|| "select a category for this budget".to_string())?;
                let amount = form.field(1).amount_cents()?;
                let month = form.field(2).month_value()?;
                let account_id = form.field(3).selected_id();
                let notes = form.field(4).input.trimmed().to_string();

                self.store
                    .set_budget(category_id, account_id, &month, amount, &notes)?;

                // Budgets are per-month, so show the month just budgeted.
                self.month = month.clone();
                self.pending_select_budget = Some(category_id);

                let name = self
                    .data
                    .categories
                    .iter()
                    .find(|c| c.id == category_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| "category".to_string());
                Ok(format!(
                    "Set {month} budget for '{name}' to {} {}",
                    self.currency,
                    crate::money::format_cents(amount)
                ))
            }
        }
    }

    fn select_account_by_name(&mut self, name: &str) {
        self.pending_select_account = Some(name.to_string());
    }

    fn select_category_by_name(&mut self, name: &str, kind: Kind) {
        self.pending_select_category = Some((name.to_string(), kind));
    }

    /// True when a modal is open, i.e. the main views do not have focus.
    pub fn is_modal_open(&self) -> bool {
        !matches!(self.modal, Modal::Browse)
    }

    /// Close whatever modal is open and return to browsing.
    pub fn close_modal(&mut self) {
        if self.is_modal_open() {
            self.modal = Modal::Browse;
            self.scroll = 0;
        }
    }

    pub fn toggle_help(&mut self) {
        if matches!(self.modal, Modal::Help) {
            self.modal = Modal::Browse;
        } else {
            self.modal = Modal::Help;
        }
        self.scroll = 0;
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_add(amount);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    // -- deletion ----------------------------------------------------------

    /// Ask for confirmation before deleting whatever is selected in the
    /// current view.
    pub fn request_delete(&mut self) {
        let confirm = match self.view {
            View::Accounts => self.selected_account().map(|a| Confirm {
                prompt: format!("Delete account '{}'?", a.name),
                detail: if a.txn_count > 0 {
                    format!(
                        "This also deletes its {} transaction(s), worth a net {} {}.",
                        a.txn_count,
                        self.currency,
                        crate::money::format_cents(a.balance_cents)
                    )
                } else {
                    "This account has no transactions.".to_string()
                },
                action: ConfirmAction::DeleteAccount(a.id),
            }),
            View::Categories => self.selected_category().map(|c| Confirm {
                prompt: format!("Delete {} category '{}'?", c.kind.as_str(), c.name),
                detail: if c.txn_count > 0 {
                    format!(
                        "{} transaction(s) become uncategorized; their amounts are kept.",
                        c.txn_count
                    )
                } else {
                    "No transactions use this category.".to_string()
                },
                action: ConfirmAction::DeleteCategory(c.id),
            }),
            View::Transactions => self.selected_transaction().map(|t| Confirm {
                prompt: format!(
                    "Delete {} of {} {}?",
                    t.kind.as_str(),
                    self.currency,
                    crate::money::format_cents(t.amount_cents)
                ),
                detail: format!(
                    "{} · {} · {}{}",
                    t.date,
                    t.account_name,
                    t.category_name,
                    if t.payee.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", t.payee)
                    }
                ),
                action: ConfirmAction::DeleteTransaction(t.id),
            }),
            View::Budgets => self.selected_budget().map(|b| Confirm {
                prompt: format!("Delete {} budget for '{}'?", b.month, b.category_name),
                detail: format!(
                    "Budget {} {}, spent {} {}. Transactions are not affected.",
                    self.currency,
                    crate::money::format_cents(b.amount_cents),
                    self.currency,
                    crate::money::format_cents(b.spent_cents)
                ),
                action: ConfirmAction::DeleteBudget(b.id),
            }),
            View::Summary => {
                self.info("Nothing to delete here — SUMMARY is a read-only overview");
                return;
            }
        };

        match confirm {
            Some(c) => {
                self.modal = Modal::Confirm(c);
                self.clear_message();
            }
            None => self.info("Nothing selected to delete"),
        }
    }

    /// Carry out a confirmed deletion.
    pub fn confirm_delete(&mut self) {
        let action = match &self.modal {
            Modal::Confirm(c) => c.action,
            _ => return,
        };
        self.modal = Modal::Browse;

        let result = match action {
            ConfirmAction::DeleteAccount(id) => self.store.delete_account(id).map(|_| "Account deleted"),
            ConfirmAction::DeleteCategory(id) => {
                self.store.delete_category(id).map(|_| "Category deleted")
            }
            ConfirmAction::DeleteTransaction(id) => {
                self.store.delete_transaction(id).map(|_| "Transaction deleted")
            }
            ConfirmAction::DeleteBudget(id) => self.store.delete_budget(id).map(|_| "Budget deleted"),
        };

        match result {
            Ok(msg) => {
                self.reload();
                self.success(msg);
            }
            Err(e) => self.error(format!("Delete failed: {e}")),
        }
    }

    // -- deferred selection ------------------------------------------------

    /// Resolve any row recorded by a write into an actual selection index.
    /// Called at the end of [`Self::reload`], once the fresh data is loaded.
    fn apply_pending_selection(&mut self) {
        if let Some(name) = self.pending_select_account.take() {
            if let Some(i) = self.data.accounts.iter().position(|a| a.name == name) {
                self.selection[View::Accounts.index()] = i;
            }
        }
        if let Some((name, kind)) = self.pending_select_category.take() {
            if let Some(i) = self
                .data
                .categories
                .iter()
                .position(|c| c.name == name && c.kind == kind)
            {
                self.selection[View::Categories.index()] = i;
            }
        }
        if let Some(id) = self.pending_select_transaction.take() {
            if let Some(i) = self.visible_transactions().iter().position(|t| t.id == id) {
                self.selection[View::Transactions.index()] = i;
            }
        }
        if let Some(category_id) = self.pending_select_budget.take() {
            if let Some(i) = self
                .data
                .budgets
                .iter()
                .position(|b| b.category_id == category_id)
            {
                self.selection[View::Budgets.index()] = i;
            }
        }
    }

    // -- transaction detail ------------------------------------------------

    /// Open the full detail of the highlighted transaction.
    pub fn open_detail(&mut self) {
        match self.selected_transaction() {
            Some(t) => {
                self.modal = Modal::Detail { id: t.id };
                self.scroll = 0;
                self.clear_message();
            }
            None => self.info("No transaction selected"),
        }
    }

    /// Re-read the transaction shown in the detail view straight from the
    /// database, so what is displayed is what is stored.
    pub fn detail_transaction(&self, id: i64) -> Option<Transaction> {
        self.store.transaction(id).ok().flatten()
    }
}
