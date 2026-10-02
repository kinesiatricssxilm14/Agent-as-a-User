# toold

A local-first personal finance ledger TUI. Track income and expenses across
multiple accounts, organize them with categories, set monthly budgets, and
review summaries — entirely offline, with all data stored in a local SQLite
database.

- **Language:** Rust
- **TUI:** [ratatui](https://crates.io/crates/ratatui) + [crossterm](https://crates.io/crates/crossterm)
- **Database:** SQLite via [rusqlite](https://crates.io/crates/rusqlite) (bundled)
- **Currency:** CNY / yuan (¥). All amounts are stored as integer cents and
  displayed with a fixed two-decimal, zero-padded format (e.g. `1234.56`,
  `800.05`).

## Install

```sh
cd toold
cargo install --path .
```

After installation, run:

```sh
toold
```

## Database location

The SQLite database is created automatically on first launch. The path is
resolved in this order:

1. `TOOLD_DB` environment variable (explicit override), e.g.
   `TOOLD_DB=/data/ledger.db toold`
2. `$XDG_DATA_HOME/toold/ledger.db`
3. `$HOME/.local/share/toold/ledger.db`
4. `./toold.db` (fallback)

## Views

Press `Tab` / `Shift+Tab` to cycle, or `1`–`6` to jump directly:

| Key | View          | Purpose                                            |
|-----|---------------|----------------------------------------------------|
| 1   | `SUMMARY`     | Monthly income, expense, net income + category breakdown |
| 2   | `ACCOUNTS`    | Create/manage accounts (checking, savings, …)       |
| 3   | `CATEGORIES`  | Create income/expense categories                    |
| 4   | `TRANSACTIONS`| Record income/expense, browse, open details         |
| 5   | `BUDGETS`     | Per-category monthly budgets with spent/remaining   |
| 6   | `HELP`        | Full key reference                                  |

## Keys

### Global
- `q` / `Ctrl-C` — quit
- `Tab` / `Shift+Tab` — next / previous view
- `1`–`6` — jump to a view
- `?` — help (any key returns)

### Lists
- `↑` / `↓` — move selection
- `Enter` — open transaction details (in `TRANSACTIONS`)

### Transactions
- `i` — record income
- `e` — record expense
- `d` — delete selected
- `f` — cycle filter (all / income / expense)

### Accounts & Categories
- `a` — add
- `d` — delete (only when unused)

### Budgets & Summary
- `a` / `e` / `d` — add / edit / delete budget
- `←` / `→` (or `-` / `+`) — change viewed month
- `t` — jump to current month

### Forms
- `Enter` — next field / submit
- `Tab` / `Shift+Tab` — next / previous field
- `↑` / `↓` — previous / next field
- `←` / `→` — move cursor (or cycle a choice)
- `+` / `-` — change a date by one day
- `Esc` — cancel

Every screen shows a contextual key bar, and the `HELP` view documents all
bindings, so all functionality is discoverable without external documentation.

## Data model

| Table          | Purpose                                                  |
|----------------|----------------------------------------------------------|
| `accounts`     | Funding sources (name + type)                            |
| `categories`   | Income/expense categories                                |
| `transactions` | Income/expense records (amount in cents, date, payee, notes) |
| `budgets`      | Monthly budget per account + category                    |

## Development

```sh
cargo build
cargo test
cargo clippy
```

`cargo test` includes an end-to-end database test covering account/category
creation, transaction recording, monthly summary, budget "spent" computation,
transaction details, and referential delete protection.
