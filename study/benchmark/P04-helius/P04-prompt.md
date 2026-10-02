# toold: Personal Finance Ledger TUI Development Prompt

## Project Overview
Develop a local-first personal finance ledger text user interface (TUI) tool named "toold" for managing personal income and expense records, budget settings, and financial summaries. The tool should provide an intuitive interactive interface so users can conveniently record income and expenses, set category budgets, and view monthly financial summaries. All data is stored in a local SQLite database.

## Scenario Description
Suppose you are a user focused on personal financial management who needs systematic tracking of daily income and expenses. Most existing ledger tools are GUI or web applications, but face the following problems:
1. Depend on network connectivity and cannot be used offline
2. Data stored in the cloud raises privacy concerns
3. Slow startup and inefficient operation
4. Lack of a ledger solution usable in command-line environments

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Account management**: Create multiple accounts (e.g., personal, business) to distinguish funding sources
2. **Category management**: Create income and expense categories (e.g., dining, transport, salary) for categorized statistics
3. **Income records**: Add income records for a specified account with amount, category, date, and notes
4. **Expense records**: Add expense records for a specified account with amount, category, payee, and notes
5. **Budget settings**: Set monthly budget amounts for specified categories, linked to accounts
6. **Financial overview**: Provide a Dashboard/Summary view showing monthly income, monthly expense, net income, and other aggregates
7. **Budget view**: Provide a Budget view showing all configured budgets with current spending and remaining amounts
8. **Transaction details**: Open a single transaction and display amount, account, category, date, payee, notes, and other fields in full

## Technology Stack
- Programming language: Rust
- TUI framework: Must use a Rust TUI framework (e.g., ratatui with crossterm backend)
- Database: SQLite (using rusqlite, sqlx, or other Rust SQLite bindings)
- Build tool: Cargo

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian-based Docker container
2. **System dependencies**: The container has `ca-certificates`, `libssl3`, `libsqlite3-0`, `sqlite3`, and other base dependencies installed
3. **Data**: Ledger SQLite storage path is determined by the implementation (must support environment variable or configuration override)
4. **Launch command**: Default launch command is `toold` (no path arguments)
5. **Installation**: Create a folder `toold` containing all source code; install with `cd toold && cargo install --path .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toold/`, system command after install is `toold`. Public project name **toold**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Local SQLite ledger**: Automatically initialize the local database on first launch (storage path determined by implementation; default currency CNY/yuan).
- **Accounts**: Create accounts (including account type, e.g., checking) with user-specified names.
- **Categories**: Create income/expense categories.
- **Transactions**: Record income/expense (amount, account, category, date, payee/notes, etc.); amounts displayed in yuan with **fixed two decimal places, zero-padded** (e.g., `1234.56`, `800.05`); may store as integer cents in the database.
- **Transaction details**: When opening a single transaction, amount, account, category, date, payee, notes, and other fields must be **fully displayed on the same screen**.
- **View navigation**: Must be able to switch to main views such as Accounts, Categories, Transactions, SUMMARY, BUDGETS (names may match task descriptions).
- **Monthly summary (SUMMARY)**: Display monthly total income, total expense, and net income on the same screen (net income = income - expense, **fixed two decimal places**, e.g., `3588.42`, `714.91`).
- **Budgets (BUDGETS)**: Set monthly budgets per category; display total budget, spent, and remaining per category on the same screen (spent is the sum of that category's expenses for the month; amounts **fixed two decimal places**, e.g., `1200.00`, `156.78`, `1043.22`).

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
3. Reasonable layout supporting multi-panel switching (e.g., transaction list, account management, budget management, financial overview)
4. Support form input including amount fields, category selection, date selection, and note input

## Naming Rules
- Project name: toold
- Source directory: `toold/`
- Command after install: `toold`

Please develop the complete TUI tool project according to the requirements above.
