//! SQLite persistence layer. Every read and write in the application goes
//! through here; there is no in-memory shadow copy of the ledger, so what the
//! UI shows is always what the database contains.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::date;

pub type Result<T> = std::result::Result<T, rusqlite::Error>;

/// Default currency for a fresh ledger.
pub const DEFAULT_CURRENCY: &str = "CNY";

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// Which direction a category or transaction moves money.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Income,
    Expense,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Income => "income",
            Kind::Expense => "expense",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Income => "Income",
            Kind::Expense => "Expense",
        }
    }

    pub fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("income") {
            Kind::Income
        } else {
            Kind::Expense
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Kind::Income => Kind::Expense,
            Kind::Expense => Kind::Income,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub account_type: String,
    pub currency: String,
    pub notes: String,
    pub created_at: String,
    /// Sum of income minus expense over all time, in cents.
    pub balance_cents: i64,
    pub txn_count: i64,
}

#[derive(Debug, Clone)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub kind: Kind,
    pub notes: String,
    pub created_at: String,
    /// Total booked against this category over all time, in cents.
    pub total_cents: i64,
    pub txn_count: i64,
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: i64,
    pub kind: Kind,
    pub amount_cents: i64,
    pub account_id: i64,
    pub account_name: String,
    pub category_id: Option<i64>,
    pub category_name: String,
    pub date: String,
    pub payee: String,
    pub notes: String,
    pub created_at: String,
}

/// A budget row joined with the spending actually recorded for its month.
#[derive(Debug, Clone)]
pub struct BudgetRow {
    pub id: i64,
    pub category_id: i64,
    pub category_name: String,
    pub category_kind: Kind,
    pub account_id: Option<i64>,
    pub account_name: String,
    pub month: String,
    pub amount_cents: i64,
    pub spent_cents: i64,
    pub notes: String,
}

impl BudgetRow {
    /// Remaining budget; negative once overspent.
    pub fn remaining_cents(&self) -> i64 {
        self.amount_cents - self.spent_cents
    }

    /// Share of the budget consumed, clamped to [0, 1] for gauge rendering.
    pub fn used_ratio(&self) -> f64 {
        if self.amount_cents <= 0 {
            return if self.spent_cents > 0 { 1.0 } else { 0.0 };
        }
        (self.spent_cents as f64 / self.amount_cents as f64).clamp(0.0, 1.0)
    }

    pub fn is_over(&self) -> bool {
        self.spent_cents > self.amount_cents
    }
}

/// Aggregates behind the SUMMARY view for one month.
#[derive(Debug, Clone, Default)]
pub struct MonthlySummary {
    pub month: String,
    pub income_cents: i64,
    pub expense_cents: i64,
    pub income_count: i64,
    pub expense_count: i64,
    pub budget_total_cents: i64,
    pub budget_spent_cents: i64,
    pub budget_count: i64,
    /// Largest single expense of the month, if any.
    pub largest_expense: Option<(String, i64)>,
}

impl MonthlySummary {
    /// Net income = income - expense.
    pub fn net_cents(&self) -> i64 {
        self.income_cents - self.expense_cents
    }

    pub fn budget_remaining_cents(&self) -> i64 {
        self.budget_total_cents - self.budget_spent_cents
    }

    pub fn txn_count(&self) -> i64 {
        self.income_count + self.expense_count
    }

    /// Share of monthly income that was not spent.
    pub fn savings_rate(&self) -> Option<f64> {
        if self.income_cents <= 0 {
            return None;
        }
        Some(self.net_cents() as f64 / self.income_cents as f64)
    }
}

/// One row of the per-category breakdown shown beside the summary totals.
#[derive(Debug, Clone)]
pub struct CategoryTotal {
    pub name: String,
    pub kind: Kind,
    pub amount_cents: i64,
    pub count: i64,
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

pub struct Store {
    conn: Connection,
    path: PathBuf,
}

impl Store {
    /// Open (creating if absent) the ledger database at `path` and migrate it
    /// to the current schema.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    rusqlite::Error::InvalidPath(
                        format!("{}: {e}", parent.display()).into(),
                    )
                })?;
            }
        }
        let conn = Connection::open(path)?;
        let store = Self { conn, path: path.to_path_buf() };
        store.init_schema()?;
        Ok(store)
    }

    /// An in-memory ledger, used by the test suite.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn, path: PathBuf::from(":memory:") };
        store.init_schema()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Create the schema on first launch. Written to be safe to re-run, so it
    /// doubles as the migration path for an existing file.
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS accounts (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                name         TEXT NOT NULL UNIQUE,
                account_type TEXT NOT NULL DEFAULT 'checking',
                currency     TEXT NOT NULL DEFAULT 'CNY',
                notes        TEXT NOT NULL DEFAULT '',
                created_at   TEXT NOT NULL DEFAULT (datetime('now','localtime'))
            );

            CREATE TABLE IF NOT EXISTS categories (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL,
                kind       TEXT NOT NULL CHECK (kind IN ('income','expense')),
                notes      TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
                UNIQUE (name, kind)
            );

            CREATE TABLE IF NOT EXISTS transactions (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                kind         TEXT NOT NULL CHECK (kind IN ('income','expense')),
                amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
                account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                category_id  INTEGER REFERENCES categories(id) ON DELETE SET NULL,
                date         TEXT NOT NULL,
                payee        TEXT NOT NULL DEFAULT '',
                notes        TEXT NOT NULL DEFAULT '',
                created_at   TEXT NOT NULL DEFAULT (datetime('now','localtime'))
            );

            CREATE TABLE IF NOT EXISTS budgets (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                category_id  INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
                account_id   INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
                month        TEXT NOT NULL,
                amount_cents INTEGER NOT NULL CHECK (amount_cents >= 0),
                notes        TEXT NOT NULL DEFAULT '',
                created_at   TEXT NOT NULL DEFAULT (datetime('now','localtime'))
            );

            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_txn_date     ON transactions(date);
            CREATE INDEX IF NOT EXISTS idx_txn_account  ON transactions(account_id);
            CREATE INDEX IF NOT EXISTS idx_txn_category ON transactions(category_id);
            CREATE INDEX IF NOT EXISTS idx_budget_month ON budgets(month);
            "#,
        )?;

        // A budget is unique per (category, account, month). SQLite treats
        // NULLs as distinct in UNIQUE indexes, so account-wide budgets
        // (account_id IS NULL) need their own partial index to be deduplicated.
        self.conn.execute_batch(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_budget_scoped
                ON budgets(category_id, account_id, month)
                WHERE account_id IS NOT NULL;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_budget_global
                ON budgets(category_id, month)
                WHERE account_id IS NULL;
            "#,
        )?;

        self.conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('currency', ?1)",
            params![DEFAULT_CURRENCY],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '1')",
            [],
        )?;
        Ok(())
    }

    pub fn currency(&self) -> String {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = 'currency'", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap_or_else(|_| DEFAULT_CURRENCY.to_string())
    }

    // -- accounts ----------------------------------------------------------

    /// Create an account. Returns a friendly error when the name is taken,
    /// since that is a normal user mistake rather than a bug.
    pub fn create_account(
        &self,
        name: &str,
        account_type: &str,
        notes: &str,
    ) -> std::result::Result<i64, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("account name is required".to_string());
        }
        let account_type = {
            let t = account_type.trim();
            if t.is_empty() { "checking" } else { t }
        };
        let currency = self.currency();
        self.conn
            .execute(
                "INSERT INTO accounts (name, account_type, currency, notes) VALUES (?1, ?2, ?3, ?4)",
                params![name, account_type, currency, notes.trim()],
            )
            .map_err(|e| friendly(e, &format!("an account named '{name}' already exists")))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_account(
        &self,
        id: i64,
        name: &str,
        account_type: &str,
        notes: &str,
    ) -> std::result::Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("account name is required".to_string());
        }
        let account_type = {
            let t = account_type.trim();
            if t.is_empty() { "checking" } else { t }
        };
        self.conn
            .execute(
                "UPDATE accounts SET name = ?1, account_type = ?2, notes = ?3 WHERE id = ?4",
                params![name, account_type, notes.trim(), id],
            )
            .map_err(|e| friendly(e, &format!("an account named '{name}' already exists")))?;
        Ok(())
    }

    /// Delete an account. Its transactions cascade away with it, so the caller
    /// is expected to confirm first.
    pub fn delete_account(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT a.id, a.name, a.account_type, a.currency, a.notes, a.created_at,
                   COALESCE(SUM(CASE WHEN t.kind = 'income'  THEN t.amount_cents
                                     WHEN t.kind = 'expense' THEN -t.amount_cents
                                     ELSE 0 END), 0) AS balance_cents,
                   COUNT(t.id) AS txn_count
            FROM accounts a
            LEFT JOIN transactions t ON t.account_id = a.id
            GROUP BY a.id
            ORDER BY a.name COLLATE NOCASE
            "#,
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Account {
                id: r.get(0)?,
                name: r.get(1)?,
                account_type: r.get(2)?,
                currency: r.get(3)?,
                notes: r.get(4)?,
                created_at: r.get(5)?,
                balance_cents: r.get(6)?,
                txn_count: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    // -- categories --------------------------------------------------------

    pub fn create_category(
        &self,
        name: &str,
        kind: Kind,
        notes: &str,
    ) -> std::result::Result<i64, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("category name is required".to_string());
        }
        self.conn
            .execute(
                "INSERT INTO categories (name, kind, notes) VALUES (?1, ?2, ?3)",
                params![name, kind.as_str(), notes.trim()],
            )
            .map_err(|e| {
                friendly(
                    e,
                    &format!("a {} category named '{name}' already exists", kind.as_str()),
                )
            })?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_category(
        &self,
        id: i64,
        name: &str,
        kind: Kind,
        notes: &str,
    ) -> std::result::Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("category name is required".to_string());
        }
        self.conn
            .execute(
                "UPDATE categories SET name = ?1, kind = ?2, notes = ?3 WHERE id = ?4",
                params![name, kind.as_str(), notes.trim(), id],
            )
            .map_err(|e| {
                friendly(
                    e,
                    &format!("a {} category named '{name}' already exists", kind.as_str()),
                )
            })?;
        Ok(())
    }

    pub fn delete_category(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM categories WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT c.id, c.name, c.kind, c.notes, c.created_at,
                   COALESCE(SUM(t.amount_cents), 0) AS total_cents,
                   COUNT(t.id) AS txn_count
            FROM categories c
            LEFT JOIN transactions t ON t.category_id = c.id
            GROUP BY c.id
            ORDER BY c.kind, c.name COLLATE NOCASE
            "#,
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Category {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: Kind::from_str(&r.get::<_, String>(2)?),
                notes: r.get(3)?,
                created_at: r.get(4)?,
                total_cents: r.get(5)?,
                txn_count: r.get(6)?,
            })
        })?;
        rows.collect()
    }

    /// Categories of one direction, for the pickers in the transaction and
    /// budget forms.
    pub fn categories_of_kind(&self, kind: Kind) -> Result<Vec<Category>> {
        Ok(self
            .categories()?
            .into_iter()
            .filter(|c| c.kind == kind)
            .collect())
    }

    // -- transactions ------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn create_transaction(
        &self,
        kind: Kind,
        amount_cents: i64,
        account_id: i64,
        category_id: Option<i64>,
        date: &str,
        payee: &str,
        notes: &str,
    ) -> std::result::Result<i64, String> {
        if amount_cents <= 0 {
            return Err("amount must be greater than 0.00".to_string());
        }
        self.conn
            .execute(
                r#"INSERT INTO transactions
                     (kind, amount_cents, account_id, category_id, date, payee, notes)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
                params![
                    kind.as_str(),
                    amount_cents,
                    account_id,
                    category_id,
                    date,
                    payee.trim(),
                    notes.trim()
                ],
            )
            .map_err(|e| friendly(e, "could not save the transaction"))?;
        Ok(self.conn.last_insert_rowid())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_transaction(
        &self,
        id: i64,
        kind: Kind,
        amount_cents: i64,
        account_id: i64,
        category_id: Option<i64>,
        date: &str,
        payee: &str,
        notes: &str,
    ) -> std::result::Result<(), String> {
        if amount_cents <= 0 {
            return Err("amount must be greater than 0.00".to_string());
        }
        self.conn
            .execute(
                r#"UPDATE transactions
                      SET kind = ?1, amount_cents = ?2, account_id = ?3, category_id = ?4,
                          date = ?5, payee = ?6, notes = ?7
                    WHERE id = ?8"#,
                params![
                    kind.as_str(),
                    amount_cents,
                    account_id,
                    category_id,
                    date,
                    payee.trim(),
                    notes.trim(),
                    id
                ],
            )
            .map_err(|e| friendly(e, "could not update the transaction"))?;
        Ok(())
    }

    pub fn delete_transaction(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM transactions WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// All transactions, newest first. `month` optionally restricts to a
    /// `YYYY-MM` period.
    pub fn transactions(&self, month: Option<&str>) -> Result<Vec<Transaction>> {
        let base = r#"
            SELECT t.id, t.kind, t.amount_cents, t.account_id, a.name,
                   t.category_id, COALESCE(c.name, '(uncategorized)'),
                   t.date, t.payee, t.notes, t.created_at
            FROM transactions t
            JOIN accounts a ON a.id = t.account_id
            LEFT JOIN categories c ON c.id = t.category_id
        "#;
        let order = " ORDER BY t.date DESC, t.id DESC";

        let map = |r: &rusqlite::Row<'_>| -> Result<Transaction> {
            Ok(Transaction {
                id: r.get(0)?,
                kind: Kind::from_str(&r.get::<_, String>(1)?),
                amount_cents: r.get(2)?,
                account_id: r.get(3)?,
                account_name: r.get(4)?,
                category_id: r.get(5)?,
                category_name: r.get(6)?,
                date: r.get(7)?,
                payee: r.get(8)?,
                notes: r.get(9)?,
                created_at: r.get(10)?,
            })
        };

        match month {
            Some(m) => {
                let sql = format!("{base} WHERE substr(t.date, 1, 7) = ?1{order}");
                let mut stmt = self.conn.prepare(&sql)?;
                stmt.query_map(params![m], map)?.collect()
            }
            None => {
                let sql = format!("{base}{order}");
                let mut stmt = self.conn.prepare(&sql)?;
                stmt.query_map([], map)?.collect()
            }
        }
    }

    pub fn transaction(&self, id: i64) -> Result<Option<Transaction>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT t.id, t.kind, t.amount_cents, t.account_id, a.name,
                   t.category_id, COALESCE(c.name, '(uncategorized)'),
                   t.date, t.payee, t.notes, t.created_at
            FROM transactions t
            JOIN accounts a ON a.id = t.account_id
            LEFT JOIN categories c ON c.id = t.category_id
            WHERE t.id = ?1
            "#,
        )?;
        stmt.query_row(params![id], |r| {
            Ok(Transaction {
                id: r.get(0)?,
                kind: Kind::from_str(&r.get::<_, String>(1)?),
                amount_cents: r.get(2)?,
                account_id: r.get(3)?,
                account_name: r.get(4)?,
                category_id: r.get(5)?,
                category_name: r.get(6)?,
                date: r.get(7)?,
                payee: r.get(8)?,
                notes: r.get(9)?,
                created_at: r.get(10)?,
            })
        })
        .optional()
    }

    // -- budgets -----------------------------------------------------------

    /// Create or replace the budget for a (category, account, month) triple.
    /// Re-setting a budget is the expected way to change it, so this upserts
    /// instead of erroring on a duplicate.
    pub fn set_budget(
        &self,
        category_id: i64,
        account_id: Option<i64>,
        month: &str,
        amount_cents: i64,
        notes: &str,
    ) -> std::result::Result<i64, String> {
        if amount_cents < 0 {
            return Err("budget amount cannot be negative".to_string());
        }

        // Matching on `account_id IS ?` handles the NULL (all-accounts) case,
        // which a plain `=` comparison would never match.
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM budgets WHERE category_id = ?1 AND account_id IS ?2 AND month = ?3",
                params![category_id, account_id, month],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        match existing {
            Some(id) => {
                self.conn
                    .execute(
                        "UPDATE budgets SET amount_cents = ?1, notes = ?2 WHERE id = ?3",
                        params![amount_cents, notes.trim(), id],
                    )
                    .map_err(|e| friendly(e, "could not update the budget"))?;
                Ok(id)
            }
            None => {
                self.conn
                    .execute(
                        r#"INSERT INTO budgets (category_id, account_id, month, amount_cents, notes)
                           VALUES (?1, ?2, ?3, ?4, ?5)"#,
                        params![category_id, account_id, month, amount_cents, notes.trim()],
                    )
                    .map_err(|e| friendly(e, "could not save the budget"))?;
                Ok(self.conn.last_insert_rowid())
            }
        }
    }

    pub fn delete_budget(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM budgets WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Budgets for `month`, each joined with the spending actually recorded
    /// against its category (and account, when the budget is account-scoped).
    pub fn budgets(&self, month: &str) -> Result<Vec<BudgetRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT b.id, b.category_id, c.name, c.kind, b.account_id,
                   COALESCE(a.name, 'All accounts'), b.month, b.amount_cents, b.notes,
                   COALESCE((
                       SELECT SUM(t.amount_cents)
                       FROM transactions t
                       WHERE t.category_id = b.category_id
                         AND substr(t.date, 1, 7) = b.month
                         AND t.kind = c.kind
                         AND (b.account_id IS NULL OR t.account_id = b.account_id)
                   ), 0) AS spent_cents
            FROM budgets b
            JOIN categories c ON c.id = b.category_id
            LEFT JOIN accounts a ON a.id = b.account_id
            WHERE b.month = ?1
            ORDER BY c.name COLLATE NOCASE
            "#,
        )?;
        let rows = stmt.query_map(params![month], |r| {
            Ok(BudgetRow {
                id: r.get(0)?,
                category_id: r.get(1)?,
                category_name: r.get(2)?,
                category_kind: Kind::from_str(&r.get::<_, String>(3)?),
                account_id: r.get(4)?,
                account_name: r.get(5)?,
                month: r.get(6)?,
                amount_cents: r.get(7)?,
                notes: r.get(8)?,
                spent_cents: r.get(9)?,
            })
        })?;
        rows.collect()
    }

    /// Every month that has either a budget or a transaction, newest first.
    /// Drives the month-stepping in SUMMARY and BUDGETS.
    pub fn known_months(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT month FROM (
                SELECT DISTINCT substr(date, 1, 7) AS month FROM transactions
                UNION
                SELECT DISTINCT month FROM budgets
            )
            WHERE month IS NOT NULL AND month <> ''
            ORDER BY month DESC
            "#,
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut months: Vec<String> = rows.collect::<Result<_>>()?;
        let current = date::current_month();
        if !months.contains(&current) {
            months.push(current);
            months.sort();
            months.reverse();
        }
        Ok(months)
    }

    // -- summary -----------------------------------------------------------

    /// Aggregate one month for the SUMMARY view.
    pub fn monthly_summary(&self, month: &str) -> Result<MonthlySummary> {
        let mut summary = MonthlySummary {
            month: month.to_string(),
            ..Default::default()
        };

        let mut stmt = self.conn.prepare(
            r#"
            SELECT kind, COALESCE(SUM(amount_cents), 0), COUNT(*)
            FROM transactions
            WHERE substr(date, 1, 7) = ?1
            GROUP BY kind
            "#,
        )?;
        let rows = stmt.query_map(params![month], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (kind, total, count) = row?;
            match Kind::from_str(&kind) {
                Kind::Income => {
                    summary.income_cents = total;
                    summary.income_count = count;
                }
                Kind::Expense => {
                    summary.expense_cents = total;
                    summary.expense_count = count;
                }
            }
        }

        let budgets = self.budgets(month)?;
        summary.budget_count = budgets.len() as i64;
        summary.budget_total_cents = budgets.iter().map(|b| b.amount_cents).sum();
        summary.budget_spent_cents = budgets.iter().map(|b| b.spent_cents).sum();

        summary.largest_expense = self
            .conn
            .query_row(
                r#"
                SELECT COALESCE(NULLIF(t.payee, ''), COALESCE(c.name, '(uncategorized)')),
                       t.amount_cents
                FROM transactions t
                LEFT JOIN categories c ON c.id = t.category_id
                WHERE t.kind = 'expense' AND substr(t.date, 1, 7) = ?1
                ORDER BY t.amount_cents DESC, t.id DESC
                LIMIT 1
                "#,
                params![month],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?;

        Ok(summary)
    }

    /// Per-category totals for a month, largest first within each direction.
    pub fn category_totals(&self, month: &str) -> Result<Vec<CategoryTotal>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT COALESCE(c.name, '(uncategorized)'), t.kind,
                   COALESCE(SUM(t.amount_cents), 0), COUNT(*)
            FROM transactions t
            LEFT JOIN categories c ON c.id = t.category_id
            WHERE substr(t.date, 1, 7) = ?1
            GROUP BY c.id, t.kind
            ORDER BY t.kind, SUM(t.amount_cents) DESC
            "#,
        )?;
        let rows = stmt.query_map(params![month], |r| {
            Ok(CategoryTotal {
                name: r.get(0)?,
                kind: Kind::from_str(&r.get::<_, String>(1)?),
                amount_cents: r.get(2)?,
                count: r.get(3)?,
            })
        })?;
        rows.collect()
    }
}

/// Turn a constraint violation into a message worth showing a user, while
/// letting genuine database faults surface verbatim.
fn friendly(err: rusqlite::Error, on_conflict: &str) -> String {
    use rusqlite::ErrorCode;
    if let rusqlite::Error::SqliteFailure(inner, _) = &err {
        if inner.code == ErrorCode::ConstraintViolation {
            return on_conflict.to_string();
        }
    }
    err.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory().expect("open in-memory store")
    }

    #[test]
    fn initializes_with_default_currency() {
        let s = store();
        assert_eq!(s.currency(), "CNY");
        assert!(s.accounts().unwrap().is_empty());
        assert!(s.categories().unwrap().is_empty());
    }

    #[test]
    fn creates_accounts_and_rejects_duplicates() {
        let s = store();
        let id = s.create_account("personal", "checking", "main").unwrap();
        assert!(id > 0);
        let err = s.create_account("personal", "savings", "").unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
        assert!(s.create_account("  ", "checking", "").is_err());

        let accounts = s.accounts().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].name, "personal");
        assert_eq!(accounts[0].account_type, "checking");
        assert_eq!(accounts[0].currency, "CNY");
        assert_eq!(accounts[0].balance_cents, 0);
    }

    #[test]
    fn defaults_blank_account_type_to_checking() {
        let s = store();
        s.create_account("business", "   ", "").unwrap();
        assert_eq!(s.accounts().unwrap()[0].account_type, "checking");
    }

    #[test]
    fn same_category_name_allowed_across_kinds() {
        let s = store();
        s.create_category("bonus", Kind::Income, "").unwrap();
        s.create_category("bonus", Kind::Expense, "").unwrap();
        assert!(s.create_category("bonus", Kind::Income, "").is_err());
        assert_eq!(s.categories().unwrap().len(), 2);
    }

    #[test]
    fn account_balance_nets_income_and_expense() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        let salary = s.create_category("salary", Kind::Income, "").unwrap();
        let dining = s.create_category("dining", Kind::Expense, "").unwrap();

        s.create_transaction(Kind::Income, 500_000, acct, Some(salary), "2026-08-01", "ACME", "")
            .unwrap();
        s.create_transaction(Kind::Expense, 123_456, acct, Some(dining), "2026-08-02", "Cafe", "")
            .unwrap();

        let accounts = s.accounts().unwrap();
        assert_eq!(accounts[0].balance_cents, 500_000 - 123_456);
        assert_eq!(accounts[0].txn_count, 2);
    }

    #[test]
    fn rejects_non_positive_transaction_amounts() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        assert!(
            s.create_transaction(Kind::Expense, 0, acct, None, "2026-08-01", "", "")
                .is_err()
        );
        assert!(
            s.create_transaction(Kind::Expense, -5, acct, None, "2026-08-01", "", "")
                .is_err()
        );
    }

    #[test]
    fn monthly_summary_computes_net_income() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        let salary = s.create_category("salary", Kind::Income, "").unwrap();
        let dining = s.create_category("dining", Kind::Expense, "").unwrap();

        s.create_transaction(Kind::Income, 430_333, acct, Some(salary), "2026-08-01", "", "")
            .unwrap();
        s.create_transaction(Kind::Expense, 71_491, acct, Some(dining), "2026-08-05", "", "")
            .unwrap();
        // A different month must not leak into August's totals.
        s.create_transaction(Kind::Expense, 999_999, acct, Some(dining), "2026-07-31", "", "")
            .unwrap();

        let sum = s.monthly_summary("2026-08").unwrap();
        assert_eq!(sum.income_cents, 430_333);
        assert_eq!(sum.expense_cents, 71_491);
        assert_eq!(sum.net_cents(), 430_333 - 71_491);
        assert_eq!(sum.txn_count(), 2);
        assert_eq!(crate::money::format_cents(sum.net_cents()), "3588.42");
    }

    #[test]
    fn summary_of_empty_month_is_zero() {
        let s = store();
        let sum = s.monthly_summary("2026-08").unwrap();
        assert_eq!(sum.income_cents, 0);
        assert_eq!(sum.expense_cents, 0);
        assert_eq!(sum.net_cents(), 0);
        assert_eq!(crate::money::format_cents(sum.net_cents()), "0.00");
        assert!(sum.savings_rate().is_none());
    }

    #[test]
    fn budget_tracks_spending_for_its_month_only() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        let dining = s.create_category("dining", Kind::Expense, "").unwrap();
        s.set_budget(dining, None, "2026-08", 120_000, "").unwrap();

        s.create_transaction(Kind::Expense, 15_678, acct, Some(dining), "2026-08-03", "", "")
            .unwrap();
        s.create_transaction(Kind::Expense, 50_000, acct, Some(dining), "2026-09-03", "", "")
            .unwrap();

        let budgets = s.budgets("2026-08").unwrap();
        assert_eq!(budgets.len(), 1);
        let b = &budgets[0];
        assert_eq!(crate::money::format_cents(b.amount_cents), "1200.00");
        assert_eq!(crate::money::format_cents(b.spent_cents), "156.78");
        assert_eq!(crate::money::format_cents(b.remaining_cents()), "1043.22");
        assert!(!b.is_over());
    }

    #[test]
    fn budget_upserts_rather_than_duplicating() {
        let s = store();
        let dining = s.create_category("dining", Kind::Expense, "").unwrap();
        let first = s.set_budget(dining, None, "2026-08", 100_000, "").unwrap();
        let second = s.set_budget(dining, None, "2026-08", 200_000, "updated").unwrap();
        assert_eq!(first, second, "re-setting a budget must update in place");

        let budgets = s.budgets("2026-08").unwrap();
        assert_eq!(budgets.len(), 1);
        assert_eq!(budgets[0].amount_cents, 200_000);
        assert_eq!(budgets[0].notes, "updated");
    }

    #[test]
    fn account_scoped_budget_ignores_other_accounts() {
        let s = store();
        let personal = s.create_account("personal", "checking", "").unwrap();
        let business = s.create_account("business", "checking", "").unwrap();
        let travel = s.create_category("travel", Kind::Expense, "").unwrap();

        s.set_budget(travel, Some(personal), "2026-08", 100_000, "").unwrap();
        s.create_transaction(Kind::Expense, 20_000, personal, Some(travel), "2026-08-02", "", "")
            .unwrap();
        s.create_transaction(Kind::Expense, 70_000, business, Some(travel), "2026-08-02", "", "")
            .unwrap();

        let budgets = s.budgets("2026-08").unwrap();
        assert_eq!(budgets[0].spent_cents, 20_000, "only the scoped account counts");
        assert_eq!(budgets[0].account_name, "personal");

        // An all-accounts budget for the same category sees both.
        s.set_budget(travel, None, "2026-08", 100_000, "").unwrap();
        let budgets = s.budgets("2026-08").unwrap();
        assert_eq!(budgets.len(), 2);
        let global = budgets.iter().find(|b| b.account_id.is_none()).unwrap();
        assert_eq!(global.spent_cents, 90_000);
        assert_eq!(global.account_name, "All accounts");
    }

    #[test]
    fn overspent_budget_reports_negative_remaining() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        let dining = s.create_category("dining", Kind::Expense, "").unwrap();
        s.set_budget(dining, None, "2026-08", 10_000, "").unwrap();
        s.create_transaction(Kind::Expense, 15_050, acct, Some(dining), "2026-08-03", "", "")
            .unwrap();

        let b = &s.budgets("2026-08").unwrap()[0];
        assert!(b.is_over());
        assert_eq!(crate::money::format_cents(b.remaining_cents()), "-50.50");
        assert_eq!(b.used_ratio(), 1.0, "ratio clamps for the gauge");
    }

    #[test]
    fn transaction_detail_round_trips_every_field() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        let dining = s.create_category("dining", Kind::Expense, "").unwrap();
        let id = s
            .create_transaction(
                Kind::Expense,
                80_005,
                acct,
                Some(dining),
                "2026-08-13",
                "Blue Bottle",
                "team offsite",
            )
            .unwrap();

        let t = s.transaction(id).unwrap().expect("transaction exists");
        assert_eq!(t.kind, Kind::Expense);
        assert_eq!(crate::money::format_cents(t.amount_cents), "800.05");
        assert_eq!(t.account_name, "personal");
        assert_eq!(t.category_name, "dining");
        assert_eq!(t.date, "2026-08-13");
        assert_eq!(t.payee, "Blue Bottle");
        assert_eq!(t.notes, "team offsite");
        assert!(!t.created_at.is_empty());
    }

    #[test]
    fn update_and_delete_transaction() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        let id = s
            .create_transaction(Kind::Expense, 1000, acct, None, "2026-08-13", "shop", "")
            .unwrap();

        s.update_transaction(id, Kind::Income, 2500, acct, None, "2026-08-14", "refund", "note")
            .unwrap();
        let t = s.transaction(id).unwrap().unwrap();
        assert_eq!(t.kind, Kind::Income);
        assert_eq!(t.amount_cents, 2500);
        assert_eq!(t.payee, "refund");
        assert_eq!(t.category_name, "(uncategorized)");

        s.delete_transaction(id).unwrap();
        assert!(s.transaction(id).unwrap().is_none());
    }

    #[test]
    fn deleting_category_keeps_transaction_uncategorized() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        let dining = s.create_category("dining", Kind::Expense, "").unwrap();
        let id = s
            .create_transaction(Kind::Expense, 5000, acct, Some(dining), "2026-08-13", "", "")
            .unwrap();

        s.delete_category(dining).unwrap();
        let t = s.transaction(id).unwrap().unwrap();
        assert_eq!(t.category_id, None);
        assert_eq!(t.category_name, "(uncategorized)");
        // The expense still counts toward the month.
        assert_eq!(s.monthly_summary("2026-08").unwrap().expense_cents, 5000);
    }

    #[test]
    fn deleting_account_cascades_its_transactions() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        s.create_transaction(Kind::Expense, 5000, acct, None, "2026-08-13", "", "")
            .unwrap();
        s.delete_account(acct).unwrap();
        assert!(s.transactions(None).unwrap().is_empty());
        assert_eq!(s.monthly_summary("2026-08").unwrap().expense_cents, 0);
    }

    #[test]
    fn transactions_filter_by_month_and_sort_newest_first() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        s.create_transaction(Kind::Expense, 100, acct, None, "2026-08-01", "a", "")
            .unwrap();
        s.create_transaction(Kind::Expense, 200, acct, None, "2026-08-20", "b", "")
            .unwrap();
        s.create_transaction(Kind::Expense, 300, acct, None, "2026-07-15", "c", "")
            .unwrap();

        let all = s.transactions(None).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].payee, "b", "newest first");
        assert_eq!(all[2].payee, "c");

        let august = s.transactions(Some("2026-08")).unwrap();
        assert_eq!(august.len(), 2);
        assert!(august.iter().all(|t| t.date.starts_with("2026-08")));
    }

    #[test]
    fn category_totals_group_by_kind_and_category() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        let dining = s.create_category("dining", Kind::Expense, "").unwrap();
        let transport = s.create_category("transport", Kind::Expense, "").unwrap();
        s.create_transaction(Kind::Expense, 3000, acct, Some(dining), "2026-08-01", "", "")
            .unwrap();
        s.create_transaction(Kind::Expense, 1000, acct, Some(dining), "2026-08-02", "", "")
            .unwrap();
        s.create_transaction(Kind::Expense, 2000, acct, Some(transport), "2026-08-03", "", "")
            .unwrap();

        let totals = s.category_totals("2026-08").unwrap();
        assert_eq!(totals.len(), 2);
        assert_eq!(totals[0].name, "dining");
        assert_eq!(totals[0].amount_cents, 4000);
        assert_eq!(totals[0].count, 2);
        assert_eq!(totals[1].name, "transport");
    }

    #[test]
    fn known_months_includes_budgets_and_current_month() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        let dining = s.create_category("dining", Kind::Expense, "").unwrap();
        s.create_transaction(Kind::Expense, 100, acct, Some(dining), "2025-03-04", "", "")
            .unwrap();
        s.set_budget(dining, None, "2025-05", 1000, "").unwrap();

        let months = s.known_months().unwrap();
        assert!(months.contains(&"2025-03".to_string()));
        assert!(months.contains(&"2025-05".to_string()));
        assert!(months.contains(&crate::date::current_month()));
        // Newest first.
        let mut sorted = months.clone();
        sorted.sort();
        sorted.reverse();
        assert_eq!(months, sorted);
    }

    #[test]
    fn income_budget_only_counts_income_transactions() {
        let s = store();
        let acct = s.create_account("personal", "checking", "").unwrap();
        // Same name in both directions; the budget must follow its own kind.
        let inc = s.create_category("bonus", Kind::Income, "").unwrap();
        let exp = s.create_category("bonus", Kind::Expense, "").unwrap();
        s.set_budget(inc, None, "2026-08", 100_000, "").unwrap();
        s.create_transaction(Kind::Income, 60_000, acct, Some(inc), "2026-08-01", "", "")
            .unwrap();
        s.create_transaction(Kind::Expense, 40_000, acct, Some(exp), "2026-08-01", "", "")
            .unwrap();

        let b = &s.budgets("2026-08").unwrap()[0];
        assert_eq!(b.category_kind, Kind::Income);
        assert_eq!(b.spent_cents, 60_000);
    }

    #[test]
    fn data_persists_across_reopen() {
        let dir = std::env::temp_dir().join(format!("toold-test-{}", std::process::id()));
        let path = dir.join("ledger.db");
        let _ = std::fs::remove_dir_all(&dir);

        {
            let s = Store::open(&path).unwrap();
            let acct = s.create_account("personal", "checking", "").unwrap();
            let cat = s.create_category("dining", Kind::Expense, "").unwrap();
            s.create_transaction(Kind::Expense, 80_005, acct, Some(cat), "2026-08-13", "Cafe", "n")
                .unwrap();
            s.set_budget(cat, None, "2026-08", 120_000, "").unwrap();
        }
        {
            let s = Store::open(&path).unwrap();
            assert_eq!(s.accounts().unwrap().len(), 1);
            let txns = s.transactions(None).unwrap();
            assert_eq!(txns.len(), 1);
            assert_eq!(txns[0].amount_cents, 80_005);
            assert_eq!(s.budgets("2026-08").unwrap()[0].amount_cents, 120_000);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
