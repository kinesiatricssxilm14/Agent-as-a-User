//! SQLite persistence layer. All money is stored as integer cents.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{
    Account, BudgetRow, Category, CategoryTotal, Summary, TransactionDetail, TransactionRow,
};

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (and initialize) the ledger database at `path`.
    pub fn open(path: &Path) -> Result<Db, String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;",
        )
        .map_err(|e| e.to_string())?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS accounts (
                    id           INTEGER PRIMARY KEY AUTOINCREMENT,
                    name         TEXT NOT NULL UNIQUE,
                    account_type TEXT NOT NULL DEFAULT 'checking'
                );

                CREATE TABLE IF NOT EXISTS categories (
                    id   INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    kind TEXT NOT NULL CHECK (kind IN ('income','expense'))
                );

                CREATE TABLE IF NOT EXISTS transactions (
                    id           INTEGER PRIMARY KEY AUTOINCREMENT,
                    account_id   INTEGER NOT NULL REFERENCES accounts(id),
                    category_id  INTEGER NOT NULL REFERENCES categories(id),
                    kind         TEXT NOT NULL CHECK (kind IN ('income','expense')),
                    amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
                    date         TEXT NOT NULL,
                    payee        TEXT NOT NULL DEFAULT '',
                    notes        TEXT NOT NULL DEFAULT '',
                    created_at   TEXT NOT NULL DEFAULT (datetime('now','localtime'))
                );

                CREATE TABLE IF NOT EXISTS budgets (
                    id           INTEGER PRIMARY KEY AUTOINCREMENT,
                    account_id   INTEGER NOT NULL REFERENCES accounts(id),
                    category_id  INTEGER NOT NULL REFERENCES categories(id),
                    month        TEXT NOT NULL,
                    amount_cents INTEGER NOT NULL CHECK (amount_cents >= 0),
                    UNIQUE (account_id, category_id, month)
                );

                CREATE INDEX IF NOT EXISTS idx_tx_date   ON transactions(date);
                CREATE INDEX IF NOT EXISTS idx_tx_acc    ON transactions(account_id);
                CREATE INDEX IF NOT EXISTS idx_tx_cat    ON transactions(category_id);
                CREATE INDEX IF NOT EXISTS idx_bud_month ON budgets(month);
                "#,
            )
            .map_err(|e| e.to_string())
    }

    // ---- reads -----------------------------------------------------------

    pub fn load_accounts(&self) -> Result<Vec<Account>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, account_type FROM accounts ORDER BY name COLLATE NOCASE")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Account {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    account_type: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        collect_rows(rows)
    }

    pub fn load_categories(&self) -> Result<Vec<Category>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, kind FROM categories ORDER BY kind, name COLLATE NOCASE")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Category {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    kind: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        collect_rows(rows)
    }

    pub fn load_transactions(&self, filter: &str) -> Result<Vec<TransactionRow>, String> {
        let mut sql = String::from(
            "SELECT t.id, t.date, t.kind, t.amount_cents, a.name, c.name, t.payee \
             FROM transactions t \
             JOIN accounts a ON a.id = t.account_id \
             JOIN categories c ON c.id = t.category_id",
        );
        match filter {
            "income" => sql.push_str(" WHERE t.kind = 'income'"),
            "expense" => sql.push_str(" WHERE t.kind = 'expense'"),
            _ => {}
        }
        sql.push_str(" ORDER BY t.date DESC, t.id DESC");

        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(TransactionRow {
                    id: r.get(0)?,
                    date: r.get(1)?,
                    kind: r.get(2)?,
                    amount_cents: r.get(3)?,
                    account: r.get(4)?,
                    category: r.get(5)?,
                    payee: r.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        collect_rows(rows)
    }

    pub fn load_budgets(&self, month: &str) -> Result<Vec<BudgetRow>, String> {
        let sql = "SELECT b.id, a.name, c.name, b.amount_cents, \
                   COALESCE((SELECT SUM(t.amount_cents) FROM transactions t \
                             WHERE t.account_id = b.account_id \
                               AND t.category_id = b.category_id \
                               AND t.kind = 'expense' \
                               AND substr(t.date,1,7) = ?1), 0) \
                   FROM budgets b \
                   JOIN accounts a ON a.id = b.account_id \
                   JOIN categories c ON c.id = b.category_id \
                   WHERE b.month = ?1 \
                   ORDER BY a.name COLLATE NOCASE, c.name COLLATE NOCASE";
        let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![month], |r| {
                Ok(BudgetRow {
                    id: r.get(0)?,
                    account: r.get(1)?,
                    category: r.get(2)?,
                    amount_cents: r.get(3)?,
                    spent_cents: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        collect_rows(rows)
    }

    pub fn load_summary(&self, month: &str) -> Result<Summary, String> {
        self.conn
            .query_row(
                "SELECT COALESCE(SUM(CASE WHEN kind='income' THEN amount_cents ELSE 0 END),0), \
                        COALESCE(SUM(CASE WHEN kind='expense' THEN amount_cents ELSE 0 END),0) \
                 FROM transactions WHERE substr(date,1,7) = ?1",
                params![month],
                |r| {
                    Ok(Summary {
                        income_cents: r.get(0)?,
                        expense_cents: r.get(1)?,
                    })
                },
            )
            .map_err(|e| e.to_string())
    }

    pub fn load_breakdown(&self, month: &str) -> Result<Vec<CategoryTotal>, String> {
        let sql = "SELECT c.name, c.kind, SUM(t.amount_cents) \
                   FROM transactions t \
                   JOIN categories c ON c.id = t.category_id \
                   WHERE substr(t.date,1,7) = ?1 \
                   GROUP BY t.category_id \
                   ORDER BY c.kind, c.name COLLATE NOCASE";
        let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![month], |r| {
                Ok(CategoryTotal {
                    name: r.get(0)?,
                    kind: r.get(1)?,
                    total_cents: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        collect_rows(rows)
    }

    pub fn transaction_detail(&self, id: i64) -> Result<TransactionDetail, String> {
        let sql = "SELECT t.id, t.kind, t.amount_cents, a.name, a.account_type, c.name, \
                          t.date, t.payee, t.notes, t.created_at \
                   FROM transactions t \
                   JOIN accounts a ON a.id = t.account_id \
                   JOIN categories c ON c.id = t.category_id \
                   WHERE t.id = ?1";
        self.conn
            .query_row(sql, params![id], |r| {
                Ok(TransactionDetail {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    amount_cents: r.get(2)?,
                    account: r.get(3)?,
                    account_type: r.get(4)?,
                    category: r.get(5)?,
                    date: r.get(6)?,
                    payee: r.get(7)?,
                    notes: r.get(8)?,
                    created_at: r.get(9)?,
                })
            })
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Transaction no longer exists".to_string())
    }

    pub fn account_id_by_name(&self, name: &str) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT id FROM accounts WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account \"{}\" not found", name))
    }

    pub fn category_id_by_name(&self, name: &str) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT id FROM categories WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Category \"{}\" not found", name))
    }

    // ---- writes ----------------------------------------------------------

    pub fn add_account(&self, name: &str, account_type: &str) -> Result<(), String> {
        let exists: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists > 0 {
            return Err(format!("An account named \"{}\" already exists", name));
        }
        self.conn
            .execute(
                "INSERT INTO accounts (name, account_type) VALUES (?1, ?2)",
                params![name, account_type],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn add_category(&self, name: &str, kind: &str) -> Result<(), String> {
        let exists: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM categories WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists > 0 {
            return Err(format!("A category named \"{}\" already exists", name));
        }
        self.conn
            .execute(
                "INSERT INTO categories (name, kind) VALUES (?1, ?2)",
                params![name, kind],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_transaction(
        &self,
        account_id: i64,
        category_id: i64,
        kind: &str,
        amount_cents: i64,
        date: &str,
        payee: &str,
        notes: &str,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO transactions \
                 (account_id, category_id, kind, amount_cents, date, payee, notes) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    account_id,
                    category_id,
                    kind,
                    amount_cents,
                    date,
                    payee,
                    notes
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn add_budget(
        &self,
        account_id: i64,
        category_id: i64,
        month: &str,
        amount_cents: i64,
    ) -> Result<(), String> {
        let exists: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM budgets WHERE account_id = ?1 AND category_id = ?2 AND month = ?3",
                params![account_id, category_id, month],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists > 0 {
            return Err(
                "A budget for this account/category/month already exists — edit it instead".into(),
            );
        }
        self.conn
            .execute(
                "INSERT INTO budgets (account_id, category_id, month, amount_cents) VALUES (?1, ?2, ?3, ?4)",
                params![account_id, category_id, month, amount_cents],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_budget(&self, id: i64, amount_cents: i64) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE budgets SET amount_cents = ?1 WHERE id = ?2",
                params![amount_cents, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_account(&self, id: i64) -> Result<(), String> {
        let tx: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE account_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let bd: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM budgets WHERE account_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if tx > 0 || bd > 0 {
            return Err("Cannot delete account: it still has transactions or budgets".into());
        }
        self.conn
            .execute("DELETE FROM accounts WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_category(&self, id: i64) -> Result<(), String> {
        let tx: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE category_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let bd: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM budgets WHERE category_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if tx > 0 || bd > 0 {
            return Err("Cannot delete category: it still has transactions or budgets".into());
        }
        self.conn
            .execute("DELETE FROM categories WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_transaction(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM transactions WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_budget(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM budgets WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn collect_rows<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>, String> {
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db() -> (Db, PathBuf) {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("toold_test_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ledger.db");
        let db = Db::open(&path).unwrap();
        (db, dir)
    }

    #[test]
    fn end_to_end_ledger_flow() {
        let (db, dir) = temp_db();

        // Accounts.
        db.add_account("personal", "checking").unwrap();
        db.add_account("business", "business").unwrap();
        assert!(db.add_account("personal", "checking").is_err()); // duplicate

        // Categories.
        db.add_category("salary", "income").unwrap();
        db.add_category("dining", "expense").unwrap();

        let personal = db.account_id_by_name("personal").unwrap();
        let salary = db.category_id_by_name("salary").unwrap();
        let dining = db.category_id_by_name("dining").unwrap();

        // Transactions.
        db.add_transaction(
            personal,
            salary,
            "income",
            500_000,
            "2025-01-10",
            "",
            "Jan pay",
        )
        .unwrap();
        let exp1 = db.add_transaction(
            personal,
            dining,
            "expense",
            1_200,
            "2025-01-11",
            "Ramen",
            "lunch",
        );
        assert!(exp1.is_ok());
        db.add_transaction(personal, dining, "expense", 800, "2025-01-12", "", "")
            .unwrap();
        // A transaction in a different month must not affect January totals.
        db.add_transaction(personal, dining, "expense", 999, "2025-02-01", "", "")
            .unwrap();

        // Summary for January.
        let summary = db.load_summary("2025-01").unwrap();
        assert_eq!(summary.income_cents, 500_000);
        assert_eq!(summary.expense_cents, 2_000);
        assert_eq!(summary.net_cents(), 498_000);

        // Budget + spent computation.
        db.add_budget(personal, dining, "2025-01", 3_000).unwrap();
        assert!(db.add_budget(personal, dining, "2025-01", 4_000).is_err()); // duplicate
        let budgets = db.load_budgets("2025-01").unwrap();
        assert_eq!(budgets.len(), 1);
        assert_eq!(budgets[0].amount_cents, 3_000);
        assert_eq!(budgets[0].spent_cents, 2_000); // only the two January dining expenses
        assert_eq!(budgets[0].account, "personal");
        assert_eq!(budgets[0].category, "dining");

        // Transaction detail (all fields).
        let tx_id = {
            let txs = db.load_transactions("expense").unwrap();
            txs.iter().find(|t| t.amount_cents == 1_200).unwrap().id
        };
        let detail = db.transaction_detail(tx_id).unwrap();
        assert_eq!(detail.kind, "expense");
        assert_eq!(detail.amount_cents, 1_200);
        assert_eq!(detail.account, "personal");
        assert_eq!(detail.account_type, "checking");
        assert_eq!(detail.category, "dining");
        assert_eq!(detail.date, "2025-01-11");
        assert_eq!(detail.payee, "Ramen");
        assert_eq!(detail.notes, "lunch");

        // Referential protection.
        assert!(db.delete_account(personal).is_err());
        assert!(db.delete_category(dining).is_err());

        // Budget edit.
        db.update_budget(budgets[0].id, 3_500).unwrap();
        assert_eq!(db.load_budgets("2025-01").unwrap()[0].amount_cents, 3_500);

        // Cleanup.
        std::fs::remove_dir_all(&dir).ok();
    }
}
