//! Application state and event handling.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::db::Db;
use crate::form::{Field, Form, FormAction};
use crate::models::*;
use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Summary,
    Accounts,
    Categories,
    Transactions,
    Budgets,
    Help,
}

impl View {
    pub const ALL: [View; 6] = [
        View::Summary,
        View::Accounts,
        View::Categories,
        View::Transactions,
        View::Budgets,
        View::Help,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            View::Summary => "SUMMARY",
            View::Accounts => "ACCOUNTS",
            View::Categories => "CATEGORIES",
            View::Transactions => "TRANSACTIONS",
            View::Budgets => "BUDGETS",
            View::Help => "HELP",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|v| v == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxFilter {
    All,
    Income,
    Expense,
}

impl TxFilter {
    pub fn as_str(&self) -> &'static str {
        match self {
            TxFilter::All => "all",
            TxFilter::Income => "income",
            TxFilter::Expense => "expense",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            TxFilter::All => TxFilter::Income,
            TxFilter::Income => TxFilter::Expense,
            TxFilter::Expense => TxFilter::All,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TxFilter::All => "all",
            TxFilter::Income => "income",
            TxFilter::Expense => "expense",
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
pub enum ConfirmAction {
    DeleteAccount(i64),
    DeleteCategory(i64),
    DeleteTransaction(i64),
    DeleteBudget(i64),
}

#[derive(Debug, Clone)]
pub struct Confirm {
    pub message: String,
    pub action: ConfirmAction,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub is_error: bool,
}

pub struct App {
    pub db: Db,
    pub db_path: PathBuf,
    pub view: View,
    pub help_return: View,
    pub selected: usize,
    pub accounts: Vec<Account>,
    pub categories: Vec<Category>,
    pub transactions: Vec<TransactionRow>,
    pub budgets: Vec<BudgetRow>,
    pub summary: Summary,
    pub breakdown: Vec<CategoryTotal>,
    pub month: (i32, u32),
    pub tx_filter: TxFilter,
    pub form: Option<Form>,
    pub detail: Option<TransactionDetail>,
    pub confirm: Option<Confirm>,
    pub status: Option<Status>,
    pub running: bool,
}

impl App {
    pub fn new(db: Db, db_path: PathBuf) -> Self {
        let month = util::current_month();
        App {
            db,
            db_path,
            view: View::Summary,
            help_return: View::Summary,
            selected: 0,
            accounts: Vec::new(),
            categories: Vec::new(),
            transactions: Vec::new(),
            budgets: Vec::new(),
            summary: Summary::default(),
            breakdown: Vec::new(),
            month,
            tx_filter: TxFilter::All,
            form: None,
            detail: None,
            confirm: None,
            status: Some(Status {
                text: "Welcome to toold — press ? for help".to_string(),
                is_error: false,
            }),
            running: true,
        }
    }

    // ---- data refresh ----------------------------------------------------

    pub fn reload(&mut self) {
        let month = util::month_str(self.month.0, self.month.1);
        let mut err: Option<String> = None;

        self.accounts = or_err(self.db.load_accounts(), &mut err);
        self.categories = or_err(self.db.load_categories(), &mut err);
        self.transactions = or_err(self.db.load_transactions(self.tx_filter.as_str()), &mut err);
        self.budgets = or_err(self.db.load_budgets(&month), &mut err);
        self.summary = or_err(self.db.load_summary(&month), &mut err);
        self.breakdown = or_err(self.db.load_breakdown(&month), &mut err);

        if let Some(e) = err {
            self.set_status(e, true);
        }
        self.clamp_selection();
    }

    fn list_len(&self) -> usize {
        match self.view {
            View::Accounts => self.accounts.len(),
            View::Categories => self.categories.len(),
            View::Transactions => self.transactions.len(),
            View::Budgets => self.budgets.len(),
            _ => 0,
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.list_len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    pub fn set_status(&mut self, text: String, is_error: bool) {
        self.status = Some(Status { text, is_error });
    }

    pub fn set_view(&mut self, v: View) {
        self.view = v;
        self.selected = 0;
        self.clamp_selection();
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.list_len() as i32;
        if len == 0 {
            return;
        }
        let s = (self.selected as i32 + delta).rem_euclid(len);
        self.selected = s as usize;
    }

    fn shift_month(&mut self, delta: i32) {
        self.month = util::shift_month(self.month.0, self.month.1, delta);
        self.reload();
    }

    // ---- top-level key routing ------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.running = false;
            return;
        }
        if self.confirm.is_some() {
            self.handle_confirm_key(key);
        } else if self.form.is_some() {
            self.handle_form_key(key);
        } else if self.detail.is_some() {
            self.handle_detail_key(key);
        } else {
            self.handle_view_key(key);
        }
    }

    fn handle_view_key(&mut self, key: KeyEvent) {
        // Global keys (active in every normal view).
        match key.code {
            KeyCode::Char('q') => {
                self.running = false;
                return;
            }
            KeyCode::Char('?') => {
                self.help_return = self.view;
                self.set_view(View::Help);
                return;
            }
            KeyCode::Tab => {
                let i = (self.view.index() + 1) % View::ALL.len();
                self.set_view(View::ALL[i]);
                return;
            }
            KeyCode::BackTab => {
                let i = (self.view.index() + View::ALL.len() - 1) % View::ALL.len();
                self.set_view(View::ALL[i]);
                return;
            }
            KeyCode::Char(c) => {
                if let Some(d) = c.to_digit(10) {
                    if (1..=6).contains(&d) {
                        self.set_view(View::ALL[d as usize - 1]);
                        return;
                    }
                }
            }
            _ => {}
        }

        match self.view {
            View::Summary => self.handle_summary_key(key),
            View::Accounts => self.handle_accounts_key(key),
            View::Categories => self.handle_categories_key(key),
            View::Transactions => self.handle_transactions_key(key),
            View::Budgets => self.handle_budgets_key(key),
            View::Help => self.handle_help_key(key),
        }
    }

    fn handle_summary_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left | KeyCode::Char('-') => self.shift_month(-1),
            KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => self.shift_month(1),
            KeyCode::Char('t') => {
                self.month = util::current_month();
                self.reload();
            }
            _ => {}
        }
    }

    fn handle_accounts_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('a') => self.open_add_account(),
            KeyCode::Char('d') | KeyCode::Delete => self.request_delete_account(),
            _ => {}
        }
    }

    fn handle_categories_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('a') => self.open_add_category(),
            KeyCode::Char('d') | KeyCode::Delete => self.request_delete_category(),
            _ => {}
        }
    }

    fn handle_transactions_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('i') => self.open_add_transaction("income"),
            KeyCode::Char('e') => self.open_add_transaction("expense"),
            KeyCode::Enter => self.open_detail(),
            KeyCode::Char('d') | KeyCode::Delete => self.request_delete_transaction(),
            KeyCode::Char('f') => {
                self.tx_filter = self.tx_filter.next();
                self.selected = 0;
                self.reload();
            }
            _ => {}
        }
    }

    fn handle_budgets_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('a') => self.open_add_budget(),
            KeyCode::Char('e') => self.open_edit_budget(),
            KeyCode::Char('d') | KeyCode::Delete => self.request_delete_budget(),
            KeyCode::Left | KeyCode::Char('-') => self.shift_month(-1),
            KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => self.shift_month(1),
            KeyCode::Char('t') => {
                self.month = util::current_month();
                self.reload();
            }
            _ => {}
        }
    }

    fn handle_help_key(&mut self, _key: KeyEvent) {
        let v = self.help_return;
        self.set_view(v);
    }

    // ---- detail screen ---------------------------------------------------

    fn open_detail(&mut self) {
        let Some(tx) = self.transactions.get(self.selected) else {
            return;
        };
        match self.db.transaction_detail(tx.id) {
            Ok(d) => self.detail = Some(d),
            Err(e) => self.set_status(e, true),
        }
    }

    fn handle_detail_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                self.detail = None;
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(d) = &self.detail {
                    self.confirm = Some(Confirm {
                        message: format!("Delete transaction #{}?", d.id),
                        action: ConfirmAction::DeleteTransaction(d.id),
                    });
                }
            }
            _ => {}
        }
    }

    // ---- confirm dialog --------------------------------------------------

    fn handle_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(c) = self.confirm.take() {
                    self.execute_confirm(c);
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm = None;
            }
            _ => {}
        }
    }

    fn execute_confirm(&mut self, c: Confirm) {
        let is_tx = matches!(c.action, ConfirmAction::DeleteTransaction(_));
        let result = match c.action {
            ConfirmAction::DeleteAccount(id) => self
                .db
                .delete_account(id)
                .map(|_| "Account deleted".to_string()),
            ConfirmAction::DeleteCategory(id) => self
                .db
                .delete_category(id)
                .map(|_| "Category deleted".to_string()),
            ConfirmAction::DeleteTransaction(id) => self
                .db
                .delete_transaction(id)
                .map(|_| "Transaction deleted".to_string()),
            ConfirmAction::DeleteBudget(id) => self
                .db
                .delete_budget(id)
                .map(|_| "Budget deleted".to_string()),
        };
        match result {
            Ok(msg) => {
                if is_tx {
                    self.detail = None;
                }
                self.reload();
                self.set_status(msg, false);
            }
            Err(e) => self.set_status(e, true),
        }
    }

    // ---- forms -----------------------------------------------------------

    fn open_add_account(&mut self) {
        let mut form = Form::new("New Account", FormAction::AddAccount);
        form.fields.push(Field::text("Name", ""));
        form.fields.push(Field::choice(
            "Type",
            vec!["checking", "savings", "credit", "cash", "business", "other"]
                .into_iter()
                .map(String::from)
                .collect(),
            0,
        ));
        self.form = Some(form);
    }

    fn open_add_category(&mut self) {
        let mut form = Form::new("New Category", FormAction::AddCategory);
        form.fields.push(Field::text("Name", ""));
        form.fields.push(Field::choice(
            "Kind",
            vec!["expense".to_string(), "income".to_string()],
            0,
        ));
        self.form = Some(form);
    }

    fn open_add_transaction(&mut self, kind: &str) {
        let is_income = kind == "income";
        let account_options: Vec<String> = self.accounts.iter().map(|a| a.name.clone()).collect();
        let category_options: Vec<String> = self
            .categories
            .iter()
            .filter(|c| c.kind == kind)
            .map(|c| c.name.clone())
            .collect();

        let mut form = Form::new(
            if is_income {
                "New Income"
            } else {
                "New Expense"
            },
            if is_income {
                FormAction::AddIncome
            } else {
                FormAction::AddExpense
            },
        );
        form.fields
            .push(Field::choice("Account", account_options, 0));
        form.fields
            .push(Field::choice("Category", category_options, 0));
        form.fields.push(Field::money("Amount (¥)", ""));
        form.fields.push(Field::date("Date", &util::today()));
        if !is_income {
            form.fields.push(Field::text("Payee", ""));
        }
        form.fields.push(Field::text("Notes", ""));
        self.form = Some(form);
    }

    fn open_add_budget(&mut self) {
        let account_options: Vec<String> = self.accounts.iter().map(|a| a.name.clone()).collect();
        let category_options: Vec<String> = self
            .categories
            .iter()
            .filter(|c| c.kind == "expense")
            .map(|c| c.name.clone())
            .collect();
        let month = util::month_str(self.month.0, self.month.1);

        let mut form = Form::new("New Budget", FormAction::AddBudget);
        form.fields
            .push(Field::choice("Account", account_options, 0));
        form.fields
            .push(Field::choice("Category", category_options, 0));
        form.fields.push(Field::money("Amount (¥)", ""));
        form.fields.push(Field::text("Month", &month));
        self.form = Some(form);
    }

    fn open_edit_budget(&mut self) {
        let Some(b) = self.budgets.get(self.selected) else {
            return;
        };
        let mut form = Form::new("Edit Budget", FormAction::EditBudget(b.id));
        form.fields
            .push(Field::money("Amount (¥)", &util::fmt_cents(b.amount_cents)));
        self.form = Some(form);
    }

    fn request_delete_account(&mut self) {
        if let Some(a) = self.accounts.get(self.selected) {
            self.confirm = Some(Confirm {
                message: format!("Delete account \"{}\"?", a.name),
                action: ConfirmAction::DeleteAccount(a.id),
            });
        }
    }

    fn request_delete_category(&mut self) {
        if let Some(c) = self.categories.get(self.selected) {
            self.confirm = Some(Confirm {
                message: format!("Delete category \"{}\"?", c.name),
                action: ConfirmAction::DeleteCategory(c.id),
            });
        }
    }

    fn request_delete_transaction(&mut self) {
        if let Some(t) = self.transactions.get(self.selected) {
            self.confirm = Some(Confirm {
                message: format!(
                    "Delete transaction #{} ({} ¥{})?",
                    t.id,
                    t.kind,
                    util::fmt_cents(t.amount_cents)
                ),
                action: ConfirmAction::DeleteTransaction(t.id),
            });
        }
    }

    fn request_delete_budget(&mut self) {
        if let Some(b) = self.budgets.get(self.selected) {
            self.confirm = Some(Confirm {
                message: format!("Delete budget for {} / {}?", b.account, b.category),
                action: ConfirmAction::DeleteBudget(b.id),
            });
        }
    }

    fn handle_form_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.form = None;
                return;
            }
            KeyCode::Enter => {
                let submit = self
                    .form
                    .as_ref()
                    .map(|f| f.focus + 1 >= f.fields.len())
                    .unwrap_or(false);
                if submit {
                    if let Some(form) = self.form.take() {
                        self.submit_form(form);
                    }
                } else if let Some(form) = self.form.as_mut() {
                    form.focus += 1;
                }
                return;
            }
            KeyCode::Tab => {
                if let Some(form) = self.form.as_mut() {
                    let n = form.fields.len();
                    if n > 0 {
                        form.focus = (form.focus + 1) % n;
                    }
                }
                return;
            }
            KeyCode::BackTab => {
                if let Some(form) = self.form.as_mut() {
                    let n = form.fields.len();
                    if n > 0 {
                        form.focus = (form.focus + n - 1) % n;
                    }
                }
                return;
            }
            KeyCode::Up => {
                if let Some(form) = self.form.as_mut() {
                    if form.focus > 0 {
                        form.focus -= 1;
                    }
                }
                return;
            }
            KeyCode::Down => {
                if let Some(form) = self.form.as_mut() {
                    if form.focus + 1 < form.fields.len() {
                        form.focus += 1;
                    }
                }
                return;
            }
            _ => {}
        }

        // Field editing keys.
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let field = &mut form.fields[form.focus];
        match key.code {
            KeyCode::Left => {
                if field.is_choice() {
                    field.choice_prev();
                } else {
                    field.cursor_left();
                }
            }
            KeyCode::Right => {
                if field.is_choice() {
                    field.choice_next();
                } else {
                    field.cursor_right();
                }
            }
            KeyCode::Backspace => field.backspace(),
            KeyCode::Char(c) => {
                if field.is_date() {
                    match c {
                        '+' => {
                            field.date_shift(1);
                            return;
                        }
                        '-' => {
                            field.date_shift(-1);
                            return;
                        }
                        _ => {}
                    }
                }
                field.insert_char(c);
            }
            _ => {}
        }
        form.error = None;
    }

    fn submit_form(&mut self, form: Form) {
        match self.apply_form(&form) {
            Ok(msg) => {
                self.reload();
                self.set_status(msg, false);
                // form intentionally dropped on success
            }
            Err(e) => {
                let mut f = form;
                f.error = Some(e);
                self.form = Some(f);
            }
        }
    }

    fn apply_form(&self, form: &Form) -> Result<String, String> {
        let vals: Vec<String> = form.fields.iter().map(|f| f.value()).collect();
        match &form.action {
            FormAction::AddAccount => {
                let name = vals[0].trim().to_string();
                let atype = vals.get(1).cloned().unwrap_or_default();
                if name.is_empty() {
                    return Err("Account name is required".into());
                }
                self.db.add_account(&name, &atype)?;
                Ok(format!("Created account \"{}\"", name))
            }
            FormAction::AddCategory => {
                let name = vals[0].trim().to_string();
                let kind = vals.get(1).cloned().unwrap_or_default();
                if name.is_empty() {
                    return Err("Category name is required".into());
                }
                self.db.add_category(&name, &kind)?;
                Ok(format!("Created {} category \"{}\"", kind, name))
            }
            FormAction::AddIncome | FormAction::AddExpense => {
                let kind = match &form.action {
                    FormAction::AddIncome => "income",
                    _ => "expense",
                };
                let account_name = vals.first().cloned().unwrap_or_default();
                let category_name = vals.get(1).cloned().unwrap_or_default();
                let amount = util::parse_money(vals.get(2).map(String::as_str).unwrap_or(""))?;
                if amount <= 0 {
                    return Err("Amount must be greater than zero".into());
                }
                let date = util::parse_date(vals.get(3).map(String::as_str).unwrap_or(""))?;

                if account_name.is_empty() {
                    return Err("No accounts yet — create one in ACCOUNTS first".into());
                }
                if category_name.is_empty() {
                    return Err("No matching categories — create one in CATEGORIES first".into());
                }

                let (payee, notes) = if kind == "expense" {
                    (
                        vals.get(4).cloned().unwrap_or_default(),
                        vals.get(5).cloned().unwrap_or_default(),
                    )
                } else {
                    (String::new(), vals.get(4).cloned().unwrap_or_default())
                };
                let payee = payee.trim().to_string();
                let notes = notes.trim().to_string();

                let account_id = self.db.account_id_by_name(&account_name)?;
                let category_id = self.db.category_id_by_name(&category_name)?;
                self.db.add_transaction(
                    account_id,
                    category_id,
                    kind,
                    amount,
                    &date,
                    &payee,
                    &notes,
                )?;
                Ok(format!("Recorded {} ¥{}", kind, util::fmt_cents(amount)))
            }
            FormAction::AddBudget => {
                let account_name = vals.first().cloned().unwrap_or_default();
                let category_name = vals.get(1).cloned().unwrap_or_default();
                let amount = util::parse_money(vals.get(2).map(String::as_str).unwrap_or(""))?;
                let month = util::parse_month(vals.get(3).map(String::as_str).unwrap_or(""))?;

                if account_name.is_empty() {
                    return Err("No accounts yet — create one in ACCOUNTS first".into());
                }
                if category_name.is_empty() {
                    return Err("No expense categories — create one in CATEGORIES first".into());
                }

                let account_id = self.db.account_id_by_name(&account_name)?;
                let category_id = self.db.category_id_by_name(&category_name)?;
                self.db
                    .add_budget(account_id, category_id, &month, amount)?;
                Ok(format!(
                    "Set budget ¥{} for {} / {} ({})",
                    util::fmt_cents(amount),
                    account_name,
                    category_name,
                    month
                ))
            }
            FormAction::EditBudget(id) => {
                let amount = util::parse_money(vals.first().map(String::as_str).unwrap_or(""))?;
                self.db.update_budget(*id, amount)?;
                Ok(format!("Updated budget to ¥{}", util::fmt_cents(amount)))
            }
        }
    }
}

fn or_err<T: Default>(r: Result<T, String>, err: &mut Option<String>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => {
            *err = Some(e);
            T::default()
        }
    }
}
