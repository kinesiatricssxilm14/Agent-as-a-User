use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub account_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Income,
    Expense,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Income => "Income",
            Self::Expense => "Expense",
        }
    }
    pub fn from_db(s: &str) -> Self {
        if s == "income" {
            Self::Income
        } else {
            Self::Expense
        }
    }
}

#[derive(Clone, Debug)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub kind: Kind,
}

#[derive(Clone, Debug)]
pub struct Transaction {
    pub id: i64,
    pub kind: Kind,
    pub amount_cents: i64,
    pub account: String,
    pub category: String,
    pub date: String,
    pub payee: String,
    pub notes: String,
    pub created_at: String,
}

pub struct NewTransaction<'a> {
    pub kind: &'a Kind,
    pub amount_cents: i64,
    pub account_id: i64,
    pub category_id: i64,
    pub date: &'a str,
    pub payee: &'a str,
    pub notes: &'a str,
}

#[derive(Clone, Debug)]
pub struct BudgetRow {
    pub id: i64,
    pub account: String,
    pub category: String,
    pub amount_cents: i64,
    pub spent_cents: i64,
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub income_cents: i64,
    pub expense_cents: i64,
    pub transaction_count: i64,
    pub largest_expense_cents: i64,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("open database {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA busy_timeout=3000;",
        )?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                account_type TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS categories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('income','expense')),
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(name, kind)
            );
            CREATE TABLE IF NOT EXISTS transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL CHECK(kind IN ('income','expense')),
                amount_cents INTEGER NOT NULL CHECK(amount_cents > 0),
                account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
                category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE RESTRICT,
                date TEXT NOT NULL,
                payee TEXT NOT NULL DEFAULT '',
                notes TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_transactions_date ON transactions(date);
            CREATE INDEX IF NOT EXISTS idx_transactions_category ON transactions(category_id);
            CREATE TABLE IF NOT EXISTS budgets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
                month TEXT NOT NULL,
                amount_cents INTEGER NOT NULL CHECK(amount_cents > 0),
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(account_id, category_id, month)
            );
        "#,
        )?;
        Ok(())
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let mut s = self
            .conn
            .prepare("SELECT id,name,account_type FROM accounts ORDER BY name")?;
        let rows = s
            .query_map([], |r| {
                Ok(Account {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    account_type: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
    pub fn add_account(&self, name: &str, typ: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO accounts(name,account_type) VALUES(?1,?2)",
            params![name.trim(), typ.trim()],
        )?;
        Ok(())
    }
    pub fn delete_account(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM accounts WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut s = self
            .conn
            .prepare("SELECT id,name,kind FROM categories ORDER BY kind,name")?;
        let rows = s
            .query_map([], |r| {
                let k: String = r.get(2)?;
                Ok(Category {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    kind: Kind::from_db(&k),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
    pub fn add_category(&self, name: &str, kind: &Kind) -> Result<()> {
        self.conn.execute(
            "INSERT INTO categories(name,kind) VALUES(?1,?2)",
            params![name.trim(), kind.as_str()],
        )?;
        Ok(())
    }
    pub fn delete_category(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM categories WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn add_transaction(&self, input: NewTransaction<'_>) -> Result<()> {
        let category_kind: Option<String> = self
            .conn
            .query_row(
                "SELECT kind FROM categories WHERE id=?1",
                [input.category_id],
                |r| r.get(0),
            )
            .optional()?;
        if category_kind.as_deref() != Some(input.kind.as_str()) {
            anyhow::bail!("category type must match transaction type");
        }
        self.conn.execute(
            "INSERT INTO transactions(kind,amount_cents,account_id,category_id,date,payee,notes) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                input.kind.as_str(),
                input.amount_cents,
                input.account_id,
                input.category_id,
                input.date,
                input.payee.trim(),
                input.notes.trim()
            ],
        )?;
        Ok(())
    }
    pub fn transactions(&self) -> Result<Vec<Transaction>> {
        let mut s=self.conn.prepare(r#"SELECT t.id,t.kind,t.amount_cents,a.name,c.name,t.date,t.payee,t.notes,t.created_at
            FROM transactions t JOIN accounts a ON a.id=t.account_id JOIN categories c ON c.id=t.category_id
            ORDER BY t.date DESC,t.id DESC"#)?;
        let rows = s
            .query_map([], |r| {
                let k: String = r.get(1)?;
                Ok(Transaction {
                    id: r.get(0)?,
                    kind: Kind::from_db(&k),
                    amount_cents: r.get(2)?,
                    account: r.get(3)?,
                    category: r.get(4)?,
                    date: r.get(5)?,
                    payee: r.get(6)?,
                    notes: r.get(7)?,
                    created_at: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
    pub fn delete_transaction(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM transactions WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn summary(&self, month: &str) -> Result<Summary> {
        let pattern = format!("{}-%", month);
        self.conn
            .query_row(
                r#"SELECT
          COALESCE(SUM(CASE WHEN kind='income' THEN amount_cents ELSE 0 END),0),
          COALESCE(SUM(CASE WHEN kind='expense' THEN amount_cents ELSE 0 END),0),
          COUNT(*), COALESCE(MAX(CASE WHEN kind='expense' THEN amount_cents ELSE 0 END),0)
          FROM transactions WHERE date LIKE ?1"#,
                [pattern],
                |r| {
                    Ok(Summary {
                        income_cents: r.get(0)?,
                        expense_cents: r.get(1)?,
                        transaction_count: r.get(2)?,
                        largest_expense_cents: r.get(3)?,
                    })
                },
            )
            .map_err(Into::into)
    }
    pub fn top_expense_categories(&self, month: &str) -> Result<Vec<(String, i64)>> {
        let pattern = format!("{}-%", month);
        let mut s=self.conn.prepare(r#"SELECT c.name,SUM(t.amount_cents) total FROM transactions t JOIN categories c ON c.id=t.category_id WHERE t.kind='expense' AND t.date LIKE ?1 GROUP BY c.id,c.name ORDER BY total DESC LIMIT 8"#)?;
        let rows = s
            .query_map([pattern], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn set_budget(&self, account: i64, category: i64, month: &str, amount: i64) -> Result<()> {
        self.conn.execute(r#"INSERT INTO budgets(account_id,category_id,month,amount_cents) VALUES(?1,?2,?3,?4)
          ON CONFLICT(account_id,category_id,month) DO UPDATE SET amount_cents=excluded.amount_cents"#,params![account,category,month,amount])?;
        Ok(())
    }
    pub fn budgets(&self, month: &str) -> Result<Vec<BudgetRow>> {
        let mut s=self.conn.prepare(r#"SELECT b.id,a.name,c.name,b.month,b.amount_cents,
          COALESCE((SELECT SUM(t.amount_cents) FROM transactions t WHERE t.account_id=b.account_id AND t.category_id=b.category_id AND t.kind='expense' AND substr(t.date,1,7)=b.month),0)
          FROM budgets b JOIN accounts a ON a.id=b.account_id JOIN categories c ON c.id=b.category_id WHERE b.month=?1 ORDER BY a.name,c.name"#)?;
        let rows = s
            .query_map([month], |r| {
                Ok(BudgetRow {
                    id: r.get(0)?,
                    account: r.get(1)?,
                    category: r.get(2)?,
                    amount_cents: r.get(4)?,
                    spent_cents: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
    pub fn delete_budget(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM budgets WHERE id=?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn stores_and_aggregates_ledger_data() {
        let db = test_db();
        db.add_account("Personal", "checking").unwrap();
        db.add_category("Salary", &Kind::Income).unwrap();
        db.add_category("Dining", &Kind::Expense).unwrap();
        let account = db.accounts().unwrap()[0].id;
        let categories = db.categories().unwrap();
        let salary = categories
            .iter()
            .find(|c| c.kind == Kind::Income)
            .unwrap()
            .id;
        let dining = categories
            .iter()
            .find(|c| c.kind == Kind::Expense)
            .unwrap()
            .id;

        db.add_transaction(NewTransaction {
            kind: &Kind::Income,
            amount_cents: 500_000,
            account_id: account,
            category_id: salary,
            date: "2026-08-01",
            payee: "Employer",
            notes: "August salary",
        })
        .unwrap();
        db.add_transaction(NewTransaction {
            kind: &Kind::Expense,
            amount_cents: 12_345,
            account_id: account,
            category_id: dining,
            date: "2026-08-02",
            payee: "Cafe",
            notes: "Lunch",
        })
        .unwrap();
        db.set_budget(account, dining, "2026-08", 50_000).unwrap();

        let summary = db.summary("2026-08").unwrap();
        assert_eq!(summary.income_cents, 500_000);
        assert_eq!(summary.expense_cents, 12_345);
        assert_eq!(summary.transaction_count, 2);

        let budget = &db.budgets("2026-08").unwrap()[0];
        assert_eq!(budget.amount_cents, 50_000);
        assert_eq!(budget.spent_cents, 12_345);
    }

    #[test]
    fn rejects_category_kind_mismatch() {
        let db = test_db();
        db.add_account("Cash", "cash").unwrap();
        db.add_category("Salary", &Kind::Income).unwrap();
        let account = db.accounts().unwrap()[0].id;
        let category = db.categories().unwrap()[0].id;
        assert!(db
            .add_transaction(NewTransaction {
                kind: &Kind::Expense,
                amount_cents: 100,
                account_id: account,
                category_id: category,
                date: "2026-08-01",
                payee: "",
                notes: "",
            })
            .is_err());
    }
}
