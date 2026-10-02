//! Data models shared between the database layer and the UI layer.

#[derive(Debug, Clone)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub account_type: String,
}

#[derive(Debug, Clone)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub kind: String, // "income" | "expense"
}

/// A single row in the transactions list (names already resolved via joins).
#[derive(Debug, Clone)]
pub struct TransactionRow {
    pub id: i64,
    pub date: String,
    pub kind: String, // "income" | "expense"
    pub amount_cents: i64,
    pub account: String,
    pub category: String,
    pub payee: String,
}

/// A single row in the budgets list with the spending already resolved.
#[derive(Debug, Clone)]
pub struct BudgetRow {
    pub id: i64,
    pub account: String,
    pub category: String,
    pub amount_cents: i64,
    pub spent_cents: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub income_cents: i64,
    pub expense_cents: i64,
}

impl Summary {
    pub fn net_cents(&self) -> i64 {
        self.income_cents - self.expense_cents
    }
}

#[derive(Debug, Clone)]
pub struct CategoryTotal {
    pub name: String,
    pub kind: String,
    pub total_cents: i64,
}

/// Full detail of a single transaction for the detail screen.
#[derive(Debug, Clone)]
pub struct TransactionDetail {
    pub id: i64,
    pub kind: String,
    pub amount_cents: i64,
    pub account: String,
    pub account_type: String,
    pub category: String,
    pub date: String,
    pub payee: String,
    pub notes: String,
    pub created_at: String,
}
