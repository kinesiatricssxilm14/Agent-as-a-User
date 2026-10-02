mod db;
mod util;

use anyhow::{Context, Result};
use chrono::{Datelike, Local, NaiveDate};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use db::{Category, Database, Kind, NewTransaction, Transaction};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame, Terminal,
};
use std::{env, io, path::PathBuf, time::Duration};
use util::{money, parse_money, valid_date, valid_month};

const CURRENCY: &str = "CNY";

#[derive(Clone, Copy, PartialEq)]
enum View {
    Summary,
    Accounts,
    Categories,
    Transactions,
    Budgets,
    Help,
}
impl View {
    fn title(self) -> &'static str {
        match self {
            Self::Summary => "SUMMARY",
            Self::Accounts => "ACCOUNTS",
            Self::Categories => "CATEGORIES",
            Self::Transactions => "TRANSACTIONS",
            Self::Budgets => "BUDGETS",
            Self::Help => "HELP",
        }
    }
    fn all() -> [View; 6] {
        [
            Self::Summary,
            Self::Accounts,
            Self::Categories,
            Self::Transactions,
            Self::Budgets,
            Self::Help,
        ]
    }
}

#[derive(Clone)]
enum Form {
    Account {
        name: String,
        typ: String,
        field: usize,
    },
    Category {
        name: String,
        kind: Kind,
        field: usize,
    },
    Transaction {
        kind: Kind,
        amount: String,
        account: usize,
        category: usize,
        date: String,
        payee: String,
        notes: String,
        field: usize,
    },
    Budget {
        account: usize,
        category: usize,
        month: String,
        amount: String,
        field: usize,
    },
}

struct App {
    db: Database,
    db_path: PathBuf,
    view: View,
    selected: usize,
    month: String,
    form: Option<Form>,
    detail: Option<Transaction>,
    status: String,
    quit: bool,
}

impl App {
    fn new(db: Database, path: PathBuf) -> Self {
        let now = Local::now();
        Self {
            db,
            db_path: path,
            view: View::Summary,
            selected: 0,
            month: format!("{:04}-{:02}", now.year(), now.month()),
            form: None,
            detail: None,
            status: "Welcome. Press ? for help; a/i/e add records.".into(),
            quit: false,
        }
    }
    fn set_view(&mut self, v: View) {
        self.view = v;
        self.selected = 0;
        self.form = None;
        self.detail = None;
        self.status = format!("{} view", v.title());
    }
    fn list_len(&self) -> usize {
        match self.view {
            View::Accounts => self.db.accounts().map(|x| x.len()).unwrap_or(0),
            View::Categories => self.db.categories().map(|x| x.len()).unwrap_or(0),
            View::Transactions => self.db.transactions().map(|x| x.len()).unwrap_or(0),
            View::Budgets => self.db.budgets(&self.month).map(|x| x.len()).unwrap_or(0),
            _ => 0,
        }
    }
    fn move_selection(&mut self, delta: isize) {
        let n = self.list_len();
        if n == 0 {
            self.selected = 0
        } else {
            self.selected = (self.selected as isize + delta).clamp(0, n as isize - 1) as usize
        }
    }
    fn shift_month(&mut self, delta: i32) {
        if let Ok(d) = NaiveDate::parse_from_str(&format!("{}-01", self.month), "%Y-%m-%d") {
            let mut y = d.year();
            let mut m = d.month() as i32 + delta;
            while m < 1 {
                m += 12;
                y -= 1
            }
            while m > 12 {
                m -= 12;
                y += 1
            }
            self.month = format!("{:04}-{:02}", y, m);
            self.selected = 0;
            self.status = format!("Showing {}", self.month);
        }
    }
    fn begin_add(&mut self, kind: Option<Kind>) -> Result<()> {
        let accounts = self.db.accounts()?;
        let categories = self.db.categories()?;
        self.form = match self.view {
            View::Accounts => Some(Form::Account {
                name: String::new(),
                typ: "checking".into(),
                field: 0,
            }),
            View::Categories => Some(Form::Category {
                name: String::new(),
                kind: kind.unwrap_or(Kind::Expense),
                field: 0,
            }),
            View::Transactions => {
                if accounts.is_empty() {
                    self.status = "Create an account before adding transactions.".into();
                    None
                } else {
                    let k = kind.unwrap_or(Kind::Expense);
                    if !categories.iter().any(|c| c.kind == k) {
                        self.status =
                            format!("Create an {} category first.", k.label().to_lowercase());
                        None
                    } else {
                        Some(Form::Transaction {
                            kind: k,
                            amount: String::new(),
                            account: 0,
                            category: 0,
                            date: Local::now().format("%Y-%m-%d").to_string(),
                            payee: String::new(),
                            notes: String::new(),
                            field: 0,
                        })
                    }
                }
            }
            View::Budgets => {
                if accounts.is_empty() || !categories.iter().any(|c| c.kind == Kind::Expense) {
                    self.status = "Create an account and expense category first.".into();
                    None
                } else {
                    Some(Form::Budget {
                        account: 0,
                        category: 0,
                        month: self.month.clone(),
                        amount: String::new(),
                        field: 0,
                    })
                }
            }
            _ => None,
        };
        Ok(())
    }
    fn delete_selected(&mut self) -> Result<()> {
        match self.view {
            View::Accounts => {
                if let Some(x) = self.db.accounts()?.get(self.selected) {
                    self.db.delete_account(x.id)?;
                    self.status = format!("Deleted account {}", x.name)
                }
            }
            View::Categories => {
                if let Some(x) = self.db.categories()?.get(self.selected) {
                    self.db.delete_category(x.id)?;
                    self.status = format!("Deleted category {}", x.name)
                }
            }
            View::Transactions => {
                if let Some(x) = self.db.transactions()?.get(self.selected) {
                    self.db.delete_transaction(x.id)?;
                    self.status = format!("Deleted transaction #{}", x.id)
                }
            }
            View::Budgets => {
                if let Some(x) = self.db.budgets(&self.month)?.get(self.selected) {
                    self.db.delete_budget(x.id)?;
                    self.status = format!("Deleted budget for {}", x.category)
                }
            }
            _ => {}
        }
        let n = self.list_len();
        if self.selected >= n && n > 0 {
            self.selected = n - 1
        };
        Ok(())
    }
    fn open_selected(&mut self) -> Result<()> {
        if self.view == View::Transactions {
            self.detail = self.db.transactions()?.get(self.selected).cloned();
        }
        Ok(())
    }
    fn save_form(&mut self) -> Result<()> {
        let form = self.form.clone().context("no form")?;
        match form {
            Form::Account { name, typ, .. } => {
                if name.trim().is_empty() {
                    anyhow::bail!("account name is required")
                };
                if typ.trim().is_empty() {
                    anyhow::bail!("account type is required")
                };
                self.db.add_account(&name, &typ)?;
                self.status = format!("Account '{}' created.", name.trim());
            }
            Form::Category { name, kind, .. } => {
                if name.trim().is_empty() {
                    anyhow::bail!("category name is required")
                };
                self.db.add_category(&name, &kind)?;
                self.status = format!("{} category '{}' created.", kind.label(), name.trim());
            }
            Form::Transaction {
                kind,
                amount,
                account,
                category,
                date,
                payee,
                notes,
                ..
            } => {
                if !valid_date(&date) {
                    anyhow::bail!("date must be YYYY-MM-DD")
                };
                let cents = parse_money(&amount)?;
                let aa = self.db.accounts()?;
                let cc: Vec<_> = self
                    .db
                    .categories()?
                    .into_iter()
                    .filter(|c| c.kind == kind)
                    .collect();
                let a = aa.get(account).context("select an account")?;
                let c = cc.get(category).context("select a category")?;
                self.db.add_transaction(NewTransaction {
                    kind: &kind,
                    amount_cents: cents,
                    account_id: a.id,
                    category_id: c.id,
                    date: &date,
                    payee: &payee,
                    notes: &notes,
                })?;
                self.status = format!(
                    "{} of {} {} recorded.",
                    kind.label(),
                    CURRENCY,
                    money(cents)
                );
            }
            Form::Budget {
                account,
                category,
                month,
                amount,
                ..
            } => {
                if !valid_month(&month) {
                    anyhow::bail!("month must be YYYY-MM")
                };
                let cents = parse_money(&amount)?;
                let aa = self.db.accounts()?;
                let cc: Vec<_> = self
                    .db
                    .categories()?
                    .into_iter()
                    .filter(|c| c.kind == Kind::Expense)
                    .collect();
                let a = aa.get(account).context("select an account")?;
                let c = cc.get(category).context("select a category")?;
                self.db.set_budget(a.id, c.id, &month, cents)?;
                self.month = month;
                self.status = format!("Budget set for {}: {} {}.", c.name, CURRENCY, money(cents));
            }
        }
        self.form = None;
        Ok(())
    }

    fn on_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        if self.form.is_some() {
            return self.form_key(key);
        }
        if self.detail.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Backspace) {
                self.detail = None
            };
            return Ok(());
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.set_view(View::Help),
            KeyCode::Char('1') => self.set_view(View::Summary),
            KeyCode::Char('2') => self.set_view(View::Accounts),
            KeyCode::Char('3') => self.set_view(View::Categories),
            KeyCode::Char('4') => self.set_view(View::Transactions),
            KeyCode::Char('5') => self.set_view(View::Budgets),
            KeyCode::Char('6') => self.set_view(View::Help),
            KeyCode::Tab => {
                let all = View::all();
                let p = all.iter().position(|v| *v == self.view).unwrap();
                self.set_view(all[(p + 1) % all.len()])
            }
            KeyCode::BackTab => {
                let all = View::all();
                let p = all.iter().position(|v| *v == self.view).unwrap();
                self.set_view(all[(p + all.len() - 1) % all.len()])
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Char('a') => self.begin_add(None)?,
            KeyCode::Char('i') if self.view == View::Transactions => {
                self.begin_add(Some(Kind::Income))?
            }
            KeyCode::Char('e') if self.view == View::Transactions => {
                self.begin_add(Some(Kind::Expense))?
            }
            KeyCode::Enter => self.open_selected()?,
            KeyCode::Char('d') => {
                if let Err(e) = self.delete_selected() {
                    self.status = format!("Cannot delete: {e}")
                }
            }
            KeyCode::Char('[') | KeyCode::Left => {
                if matches!(self.view, View::Summary | View::Budgets) {
                    self.shift_month(-1)
                }
            }
            KeyCode::Char(']') | KeyCode::Right => {
                if matches!(self.view, View::Summary | View::Budgets) {
                    self.shift_month(1)
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn form_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.code == KeyCode::Esc {
            self.form = None;
            self.status = "Entry cancelled.".into();
            return Ok(());
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            if let Err(e) = self.save_form() {
                self.status = format!("Error: {e}")
            };
            return Ok(());
        }
        let max = match self.form.as_ref().unwrap() {
            Form::Account { .. } => 2,
            Form::Category { .. } => 2,
            Form::Transaction { .. } => 7,
            Form::Budget { .. } => 4,
        };
        let current = match self.form.as_ref().unwrap() {
            Form::Account { field, .. }
            | Form::Category { field, .. }
            | Form::Transaction { field, .. }
            | Form::Budget { field, .. } => *field,
        };
        if key.code == KeyCode::Enter && current + 1 == max {
            if let Err(e) = self.save_form() {
                self.status = format!("Error: {e}")
            };
            return Ok(());
        }
        match key.code {
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter => {
                self.set_form_field((current + 1) % max)
            }
            KeyCode::BackTab | KeyCode::Up => self.set_form_field((current + max - 1) % max),
            KeyCode::Left => self.adjust_form_select(-1),
            KeyCode::Right => self.adjust_form_select(1),
            KeyCode::Backspace => self.edit_form_text(None, true),
            KeyCode::Char(c) => self.edit_form_text(Some(c), false),
            _ => {}
        }
        Ok(())
    }
    fn set_form_field(&mut self, n: usize) {
        match self.form.as_mut().unwrap() {
            Form::Account { field, .. }
            | Form::Category { field, .. }
            | Form::Transaction { field, .. }
            | Form::Budget { field, .. } => *field = n,
        }
    }
    fn edit_string(s: &mut String, c: Option<char>, back: bool) {
        if back {
            s.pop();
        } else if let Some(c) = c {
            s.push(c)
        }
    }
    fn edit_form_text(&mut self, c: Option<char>, back: bool) {
        match self.form.as_mut().unwrap() {
            Form::Account { name, typ, field } => match field {
                0 => Self::edit_string(name, c, back),
                1 => Self::edit_string(typ, c, back),
                _ => {}
            },
            Form::Category { name, field, .. } => {
                if *field == 0 {
                    Self::edit_string(name, c, back)
                }
            }
            Form::Transaction {
                amount,
                date,
                payee,
                notes,
                field,
                ..
            } => match field {
                1 => Self::edit_string(amount, c, back),
                4 => Self::edit_string(date, c, back),
                5 => Self::edit_string(payee, c, back),
                6 => Self::edit_string(notes, c, back),
                _ => {}
            },
            Form::Budget {
                month,
                amount,
                field,
                ..
            } => match field {
                2 => Self::edit_string(month, c, back),
                3 => Self::edit_string(amount, c, back),
                _ => {}
            },
        }
    }
    fn adjust_form_select(&mut self, d: isize) {
        let ac = self.db.accounts().map(|x| x.len()).unwrap_or(0);
        let all = self.db.categories().unwrap_or_default();
        match self.form.as_mut().unwrap() {
            Form::Category { kind, field, .. } if *field == 1 => {
                *kind = if *kind == Kind::Income {
                    Kind::Expense
                } else {
                    Kind::Income
                }
            }
            Form::Transaction {
                kind,
                account,
                category,
                field,
                ..
            } => match *field {
                0 => {
                    *kind = if *kind == Kind::Income {
                        Kind::Expense
                    } else {
                        Kind::Income
                    };
                    *category = 0
                }
                2 => cycle(account, ac, d),
                3 => {
                    let n = all.iter().filter(|c| c.kind == *kind).count();
                    cycle(category, n, d)
                }
                _ => {}
            },
            Form::Budget {
                account,
                category,
                field,
                ..
            } => match *field {
                0 => cycle(account, ac, d),
                1 => {
                    let n = all.iter().filter(|c| c.kind == Kind::Expense).count();
                    cycle(category, n, d)
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn cycle(i: &mut usize, n: usize, d: isize) {
    if n > 0 {
        *i = (*i as isize + d).rem_euclid(n as isize) as usize
    }
}

fn main() -> Result<()> {
    let path = db_path()?;
    let db = Database::open(&path)?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run(&mut terminal, App::new(db, path));
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}
fn db_path() -> Result<PathBuf> {
    if let Ok(p) = env::var("TOOLD_DB_PATH") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(dir) = env::var("TOOLD_DATA_DIR") {
        return Ok(PathBuf::from(dir).join("ledger.sqlite3"));
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    Ok(PathBuf::from(home).join(".local/share/toold/ledger.sqlite3"))
}
fn run<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, &app))?;
        if app.quit {
            return Ok(());
        }
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                if let Err(e) = app.on_key(k) {
                    app.status = format!("Error: {e}")
                }
            }
        }
    }
}

fn draw(f: &mut Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(f.area());
    draw_nav(f, areas[0], app);
    match app.view {
        View::Summary => draw_summary(f, areas[1], app),
        View::Accounts => draw_accounts(f, areas[1], app),
        View::Categories => draw_categories(f, areas[1], app),
        View::Transactions => draw_transactions(f, areas[1], app),
        View::Budgets => draw_budgets(f, areas[1], app),
        View::Help => draw_help(f, areas[1], app),
    };
    draw_status(f, areas[2], app);
    if let Some(form) = &app.form {
        draw_form(f, areas[1], app, form)
    }
    if let Some(tx) = &app.detail {
        draw_detail(f, areas[1], tx)
    }
}
fn draw_nav(f: &mut Frame, a: Rect, app: &App) {
    let mut spans = vec![
        Span::styled(
            " toold ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];
    for (i, v) in View::all().iter().enumerate() {
        let st = if *v == app.view {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        spans.push(Span::styled(format!("{}:{}  ", i + 1, v.title()), st));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::BOTTOM)),
        a,
    )
}
fn draw_status(f: &mut Frame, a: Rect, app: &App) {
    let keys = match app.form {
        Some(_) => "Tab/↑↓ field  ←→ choose  Ctrl-S save  Esc cancel",
        None => "↑↓/jk select  Enter open  a add  d delete  [/] month  ? help  q quit",
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", app.status),
            Style::default().fg(Color::Black).bg(Color::LightYellow),
        ),
        Span::raw("  "),
        Span::styled(keys, Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(line), a)
}
fn title_block(t: &str) -> Block<'_> {
    Block::default()
        .title(format!(" {t} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}
fn selected_style() -> Style {
    Style::default()
        .bg(Color::DarkGray)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}
fn empty(text: &str) -> Paragraph<'_> {
    Paragraph::new(text.to_string())
        .alignment(Alignment::Center)
        .block(title_block("No records"))
}

fn draw_summary(f: &mut Frame, a: Rect, app: &App) {
    let s = match app.db.summary(&app.month) {
        Ok(x) => x,
        Err(e) => {
            f.render_widget(Paragraph::new(e.to_string()), a);
            return;
        }
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(7)])
        .split(a);
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(chunks[0]);
    card(
        f,
        cards[0],
        "MONTHLY INCOME",
        &format!("{} {}", CURRENCY, money(s.income_cents)),
        Color::Green,
    );
    card(
        f,
        cards[1],
        "MONTHLY EXPENSE",
        &format!("{} {}", CURRENCY, money(s.expense_cents)),
        Color::Red,
    );
    let net = s.income_cents - s.expense_cents;
    card(
        f,
        cards[2],
        "NET INCOME",
        &format!("{} {}", CURRENCY, money(net)),
        if net >= 0 { Color::Green } else { Color::Red },
    );
    card(
        f,
        cards[3],
        "TRANSACTIONS",
        &s.transaction_count.to_string(),
        Color::Cyan,
    );
    let top = app
        .db
        .top_expense_categories(&app.month)
        .unwrap_or_default();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("Month: {}", app.month),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("   Change with [ and ] or ← and →"),
        ]),
        Line::raw(format!(
            "Largest expense: {} {}",
            CURRENCY,
            money(s.largest_expense_cents)
        )),
        Line::raw(""),
    ];
    if top.is_empty() {
        lines.push(Line::raw("No expenses recorded for this month."))
    } else {
        lines.push(Line::styled(
            "Expense by category",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        for (name, val) in top {
            lines.push(Line::raw(format!(
                "  {:<24} {} {}",
                name,
                CURRENCY,
                money(val)
            )))
        }
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(title_block("Monthly overview"))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}
fn card(f: &mut Frame, a: Rect, label: &str, value: &str, color: Color) {
    f.render_widget(
        Paragraph::new(Text::from(vec![
            Line::raw(""),
            Line::styled(
                value,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center)
        .block(title_block(label)),
        a,
    )
}

fn draw_accounts(f: &mut Frame, a: Rect, app: &App) {
    let xs = app.db.accounts().unwrap_or_default();
    if xs.is_empty() {
        f.render_widget(empty("No accounts. Press a to create one."), a);
        return;
    }
    let rows = xs.iter().map(|x| {
        Row::new(vec![
            Cell::from(x.id.to_string()),
            Cell::from(x.name.clone()),
            Cell::from(x.account_type.clone()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Percentage(50),
            Constraint::Percentage(40),
        ],
    )
    .header(
        Row::new(["ID", "NAME", "TYPE"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .row_highlight_style(selected_style())
    .highlight_symbol("▶ ")
    .block(title_block("Accounts — a add, d delete"));
    let mut state = TableState::default().with_selected(Some(app.selected));
    f.render_stateful_widget(table, a, &mut state)
}
fn draw_categories(f: &mut Frame, a: Rect, app: &App) {
    let xs = app.db.categories().unwrap_or_default();
    if xs.is_empty() {
        f.render_widget(empty("No categories. Press a to create one."), a);
        return;
    }
    let rows = xs.iter().map(|x| {
        Row::new(vec![
            x.id.to_string(),
            x.name.clone(),
            x.kind.label().into(),
        ])
        .style(Style::default().fg(if x.kind == Kind::Income {
            Color::Green
        } else {
            Color::Red
        }))
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Percentage(60),
            Constraint::Percentage(30),
        ],
    )
    .header(
        Row::new(["ID", "NAME", "KIND"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .row_highlight_style(selected_style())
    .highlight_symbol("▶ ")
    .block(title_block("Categories — a add, d delete"));
    let mut state = TableState::default().with_selected(Some(app.selected));
    f.render_stateful_widget(table, a, &mut state)
}
fn draw_transactions(f: &mut Frame, a: Rect, app: &App) {
    let xs = app.db.transactions().unwrap_or_default();
    if xs.is_empty() {
        f.render_widget(
            empty("No transactions. Press i for income or e for expense."),
            a,
        );
        return;
    }
    let rows = xs.iter().map(|x| {
        let amount = format!(
            "{}{}",
            if x.kind == Kind::Expense { "-" } else { "+" },
            money(x.amount_cents)
        );
        Row::new(vec![
            x.date.clone(),
            x.kind.label().into(),
            amount,
            x.account.clone(),
            x.category.clone(),
            x.payee.clone(),
        ])
        .style(Style::default().fg(if x.kind == Kind::Income {
            Color::Green
        } else {
            Color::Red
        }))
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(9),
            Constraint::Length(14),
            Constraint::Percentage(18),
            Constraint::Percentage(18),
            Constraint::Percentage(25),
        ],
    )
    .header(
        Row::new(["DATE", "KIND", "AMOUNT CNY", "ACCOUNT", "CATEGORY", "PAYEE"]).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .row_highlight_style(selected_style())
    .highlight_symbol("▶ ")
    .block(title_block(
        "Transactions — i income, e expense, Enter details",
    ));
    let mut state = TableState::default().with_selected(Some(app.selected));
    f.render_stateful_widget(table, a, &mut state)
}
fn draw_budgets(f: &mut Frame, a: Rect, app: &App) {
    let xs = app.db.budgets(&app.month).unwrap_or_default();
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(a);
    f.render_widget(
        Paragraph::new(format!(
            "Month {}   [ / ] changes month   a sets or updates a category budget",
            app.month
        ))
        .block(title_block("Budget period")),
        parts[0],
    );
    if xs.is_empty() {
        f.render_widget(
            empty("No budgets for this month. Press a to set one."),
            parts[1],
        );
        return;
    }
    let rows = xs.iter().map(|x| {
        let rem = x.amount_cents - x.spent_cents;
        Row::new(vec![
            x.account.clone(),
            x.category.clone(),
            money(x.amount_cents),
            money(x.spent_cents),
            money(rem),
            if rem >= 0 { "On track" } else { "Over budget" }.into(),
        ])
        .style(Style::default().fg(if rem >= 0 { Color::Green } else { Color::Red }))
    });
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Length(15),
            Constraint::Length(15),
            Constraint::Length(15),
            Constraint::Percentage(15),
        ],
    )
    .header(
        Row::new([
            "ACCOUNT",
            "CATEGORY",
            "BUDGET CNY",
            "SPENT CNY",
            "REMAINING CNY",
            "STATUS",
        ])
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .row_highlight_style(selected_style())
    .highlight_symbol("▶ ")
    .block(title_block("Configured budgets — d delete"));
    let mut state = TableState::default().with_selected(Some(app.selected));
    f.render_stateful_widget(table, parts[1], &mut state)
}

fn draw_help(f: &mut Frame, a: Rect, app: &App) {
    let text=vec![Line::styled("toold — local-first personal finance ledger",Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),Line::raw(""),Line::raw("VIEWS"),Line::raw("  1 Summary   2 Accounts   3 Categories   4 Transactions   5 Budgets   6 Help"),Line::raw("  Tab / Shift-Tab switches views. All data is stored in a real local SQLite database."),Line::raw(""),Line::raw("BROWSING"),Line::raw("  ↑/↓ or j/k select rows   Enter opens full transaction details   q quits"),Line::raw("  [ / ] or ←/→ changes the month in Summary and Budgets"),Line::raw(""),Line::raw("CREATING DATA"),Line::raw("  Accounts/Categories/Budgets: a adds a record"),Line::raw("  Transactions: i records income; e records expense; a defaults to expense"),Line::raw("  In forms: Tab/↑/↓ moves fields; ←/→ changes choices; Ctrl-S saves; Esc cancels"),Line::raw("  Enter advances a field and saves from the final field."),Line::raw(""),Line::raw("DELETING"),Line::raw("  d deletes the selected item. Accounts/categories referenced by transactions are protected."),Line::raw(""),Line::raw("STORAGE"),Line::raw(format!("  Database: {}",app.db_path.display())),Line::raw("  Override with TOOLD_DB_PATH=/path/file.sqlite3 or TOOLD_DATA_DIR=/directory"),Line::raw("  Currency: CNY (yuan); all displayed amounts have exactly two decimal places.")];
    f.render_widget(
        Paragraph::new(text)
            .block(title_block("Keyboard help"))
            .wrap(Wrap { trim: false }),
        a,
    )
}

fn form_area(a: Rect) -> Rect {
    let w = a.width.saturating_sub(8).min(90);
    let h = a.height.saturating_sub(4).min(24);
    Rect {
        x: a.x + (a.width - w) / 2,
        y: a.y + (a.height - h) / 2,
        width: w,
        height: h,
    }
}
fn draw_form(f: &mut Frame, a: Rect, app: &App, form: &Form) {
    let area = form_area(a);
    f.render_widget(Clear, area);
    let (title, fields, active) = form_lines(app, form);
    let mut lines = vec![
        Line::styled(
            "All fields are visible here. * means required.",
            Style::default().fg(Color::DarkGray),
        ),
        Line::raw(""),
    ];
    for (i, (label, value, hint)) in fields.into_iter().enumerate() {
        let st = if i == active {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(if i == active { "▶ " } else { "  " }, st),
            Span::styled(format!("{label:<16}"), st),
            Span::styled(value, Style::default().fg(Color::Cyan)),
            Span::styled(format!("  {hint}"), Style::default().fg(Color::DarkGray)),
        ]));
        lines.push(Line::raw(""));
    }
    lines.push(Line::styled(
        "Tab/↑↓: field   ←→: choose   Ctrl-S: save   Esc: cancel",
        Style::default().fg(Color::Green),
    ));
    f.render_widget(
        Paragraph::new(lines)
            .block(title_block(&title))
            .wrap(Wrap { trim: false }),
        area,
    )
}
fn form_lines(app: &App, form: &Form) -> (String, Vec<(String, String, String)>, usize) {
    let aa = app.db.accounts().unwrap_or_default();
    let cc = app.db.categories().unwrap_or_default();
    match form {
        Form::Account { name, typ, field } => (
            "New account".into(),
            vec![
                ("Name *".into(), name.clone(), "type text".into()),
                (
                    "Account type *".into(),
                    typ.clone(),
                    "e.g. checking, cash, credit".into(),
                ),
            ],
            *field,
        ),
        Form::Category { name, kind, field } => (
            "New category".into(),
            vec![
                ("Name *".into(), name.clone(), "type text".into()),
                ("Kind *".into(), kind.label().into(), "←/→ choose".into()),
            ],
            *field,
        ),
        Form::Transaction {
            kind,
            amount,
            account,
            category,
            date,
            payee,
            notes,
            field,
        } => {
            let cats: Vec<&Category> = cc.iter().filter(|c| c.kind == *kind).collect();
            (
                "New transaction".into(),
                vec![
                    ("Kind *".into(), kind.label().into(), "←/→ choose".into()),
                    (
                        "Amount (CNY) *".into(),
                        amount.clone(),
                        "e.g. 800.05".into(),
                    ),
                    (
                        "Account *".into(),
                        aa.get(*account).map(|x| x.name.clone()).unwrap_or_default(),
                        "←/→ choose".into(),
                    ),
                    (
                        "Category *".into(),
                        cats.get(*category)
                            .map(|x| x.name.clone())
                            .unwrap_or_default(),
                        "←/→ choose".into(),
                    ),
                    ("Date *".into(), date.clone(), "YYYY-MM-DD".into()),
                    ("Payee".into(), payee.clone(), "optional".into()),
                    ("Notes".into(), notes.clone(), "optional".into()),
                ],
                *field,
            )
        }
        Form::Budget {
            account,
            category,
            month,
            amount,
            field,
        } => {
            let cats: Vec<&Category> = cc.iter().filter(|c| c.kind == Kind::Expense).collect();
            (
                "Set monthly budget".into(),
                vec![
                    (
                        "Account *".into(),
                        aa.get(*account).map(|x| x.name.clone()).unwrap_or_default(),
                        "←/→ choose".into(),
                    ),
                    (
                        "Category *".into(),
                        cats.get(*category)
                            .map(|x| x.name.clone())
                            .unwrap_or_default(),
                        "expense categories only".into(),
                    ),
                    ("Month *".into(), month.clone(), "YYYY-MM".into()),
                    (
                        "Budget (CNY) *".into(),
                        amount.clone(),
                        "e.g. 1200.00".into(),
                    ),
                ],
                *field,
            )
        }
    }
}
fn draw_detail(f: &mut Frame, a: Rect, t: &Transaction) {
    f.render_widget(Clear, a);
    let signed = if t.kind == Kind::Expense {
        format!("-{}", money(t.amount_cents))
    } else {
        format!("+{}", money(t.amount_cents))
    };
    let lines = vec![
        Line::styled(
            format!("{} {}", CURRENCY, signed),
            Style::default()
                .fg(if t.kind == Kind::Income {
                    Color::Green
                } else {
                    Color::Red
                })
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        detail_line("Transaction ID", format!("#{}", t.id)),
        detail_line("Type", t.kind.label()),
        detail_line("Amount", format!("{} {}", CURRENCY, money(t.amount_cents))),
        detail_line("Account", &t.account),
        detail_line("Category", &t.category),
        detail_line("Date", &t.date),
        detail_line("Payee", if t.payee.is_empty() { "—" } else { &t.payee }),
        detail_line("Notes", if t.notes.is_empty() { "—" } else { &t.notes }),
        detail_line("Created", &t.created_at),
        Line::raw(""),
        Line::styled(
            "Enter / Esc / Backspace returns to the transaction list",
            Style::default().fg(Color::Green),
        ),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(title_block("Full transaction details"))
            .wrap(Wrap { trim: false }),
        a,
    )
}
fn detail_line<'a>(label: &str, value: impl Into<Span<'a>>) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label:<18}"), Style::default().fg(Color::DarkGray)),
        value.into(),
    ])
}
