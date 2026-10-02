# toolk: CSV Data Analysis TUI Development Prompt

## Project Overview
Develop a CSV data analysis text user interface (TUI) tool named "toolk" for loading, browsing, and querying CSV data files. The tool should provide an intuitive interactive interface supporting multiple query modes (fuzzy search, SQL-Like query, SQL query) so users can efficiently perform data analysis in the terminal.

## Scenario Description
Suppose you are a data analyst who needs to quickly analyze CSV data files in a terminal environment. You face the following problems:
1. Command-line tool arguments are complex and hard to learn quickly
2. Lack of an intuitive interface to view and filter data
3. Multiple query needs require switching between different tools
4. Data browsing and filtering operations are inefficient

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Data loading**: Load a CSV file from a specified path and display complete data in a table
2. **Fuzzy search**: Support fuzzy search on data to quickly locate target records
3. **SQL-Like query**: Support condition filtering using SQL-like syntax
4. **SQL query**: Support complex condition combination queries using standard SQL syntax
5. **Data display**: Clearly display query results in table form

## Query Mode Description

The tool must support **Fuzzy**, **SQL-Like**, and **SQL** query modes (switchable in the interface). Column names come from CSV headers; the following examples illustrate input forms for each mode and are not fixed test data.

- **Fuzzy**: Enter a keyword to fuzzy-match across table columns and locate records; when a row is selected, all columns of that row must be visible on the same screen.
  - Example: Enter `Sales` to view the complete content of matching rows.

- **SQL-Like**: Use `select where <condition>` form for SQL-like condition filtering; supports `and` / `or` and comparison operators (`=`, `>`, `<`, `>=`, `<=`, etc.).
  - Example: `select where age > 40`
  - Example: `select where department = 'Engineering' and salary > 10000`

- **SQL**: Use standard SQL statements for queries; table name is generally `df` (depends on implementation).
  - Example: `select * from df where country = 'US' and score > 85`
  - Example: `select * from df where age = 35 and salary > 5000 and salary < 15000`

After execution in each mode, the results table must display matching rows and show match count (if supported by the interface).

## Numeric Display Format

Analysis numeric values must be formatted per interface or task requirements; **fixed digit count with zero-padding** is required to avoid display/validation mismatches from dropped trailing zeros. The following examples illustrate rules only (not fixed test data):

- **Two decimal places**: Always show two digits after the decimal point (e.g., `75.25`, `1288.42`, `99.99`; integers must display as `50.00`).
- **Three significant figures**: Round to significant figures (e.g., `456`, `45.6`, `12.3`; statistics p-values and other values less than 1 must strictly keep three figures, e.g., `0.878`).
- **Correlation coefficients** (if two decimal places required): Still use fixed two decimals (e.g., `0.85`, `1.00`, `-0.12`); trailing zeros must not be omitted.

## Technology Stack
- Programming language: Rust
- TUI framework: Must use the Ratatui framework (with Crossterm backend)
- Build tool: Cargo standard toolchain
- Data processing: Use the Polars library for CSV loading and query processing

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has `ca-certificates`, `libssl3`, `libfontconfig1`, and other base dependencies installed
3. **Launch and data paths** (first command-line argument is CSV file; can be overridden):
   - Default data file: `/bench/data/employees.csv`
4. **Launch command**: Default launch command is `toolk /bench/data/employees.csv` (data file path can be overridden via launch arguments)
5. **Installation**: Create a folder `toolk` containing all source code; install with `cd toolk && cargo install --path .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toolk/`, system command after install is `toolk`. Public project name **toolk**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Load CSV**: Open a CSV file (default `/bench/data/employees.csv`; can be overridden via launch arguments); table displays **all columns** (dynamically read headers; do not assume fixed column names).
- **Fuzzy**: Keyword fuzzy-matches across columns (see "Query Mode Description" above); when a row is selected, **all columns** of that row must be visible on the same screen.
- **SQL-Like**: `select where <condition>` form filtering (see "Query Mode Description" above); results table displays matching rows and match count.
- **SQL**: `select * from df where …` form queries (see "Query Mode Description" above); results table displays matching rows and match count.
- **Analysis tools**: Perform distribution, descriptive statistics, correlation, etc. on selected columns; format numbers per "Numeric Display Format" rules with related metrics on the same screen.
- **Sort**: Ascending/descending sort by one or more columns; results table displays sorted records (at least key identifier columns).
- **Filter**: Support multi-condition combinations (including OR); display matching records.
- **Export**: Export current results to a user-specified path as CSV with headers and data rows.

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
2. Provide clear mode-switching prompts (e.g., Fuzzy mode, SQL-Like mode, SQL mode)
3. Reasonable layout with clear table display
4. Support real-time querying and result display
5. Provide a help page showing shortcut documentation (e.g., Navigation, Data Operations, Display categories)

## Naming Rules
- Project name: toolk
- Source directory: `toolk/`
- Command after install: `toolk`

Please develop the complete TUI tool project according to the requirements above.
