# toold

`toold` is a keyboard-driven, local-first personal finance ledger TUI. It manages accounts, income/expense categories, transactions, monthly summaries, and per-account/category monthly budgets in SQLite. Currency is CNY and monetary values are stored as integer cents.

## Install

```sh
cd toold
cargo install --path .
toold
```

## Storage

The default database is `$HOME/.local/share/toold/ledger.sqlite3`. Override it with either:

```sh
TOOLD_DB_PATH=/data/my-ledger.sqlite3 toold
TOOLD_DATA_DIR=/data toold
```

The schema is created automatically on first launch.

## Keyboard map

- `1` Summary, `2` Accounts, `3` Categories, `4` Transactions, `5` Budgets, `6` Help
- `Tab` / `Shift-Tab`: switch views
- `↑` / `↓` or `j` / `k`: select
- `a`: add in Accounts, Categories, and Budgets
- `i` / `e`: add income / expense in Transactions
- `Enter`: open transaction details; advance/save form
- `d`: delete selected record
- `[` / `]`: previous/next month
- `?`: complete in-app help
- `q`: quit
- In forms: `Tab`, arrows, `Ctrl-S`, and `Esc`

Accounts or categories already referenced by transactions are protected by SQLite foreign keys. Setting the same account/category/month budget again updates it.
