# toolo: SQLite Database Browser TUI Development Prompt

## Project Overview
Develop a SQLite database browser text user interface (TUI) tool named "toolo" to simplify viewing and managing SQLite databases. The tool should provide an intuitive interactive interface so developers can efficiently browse database tables, view table structure, execute SQL queries, sort and filter data without memorizing complex command-line arguments.

## Scenario Description
Suppose you are a developer or data analyst who frequently views and analyzes data in SQLite databases. You currently use command-line tools (such as the sqlite3 CLI) for database operations, but face the following problems:
1. Command arguments are complex and easy to misremember
2. Operation steps are tedious and inefficient
3. Lack of an intuitive interface to view table structure and data
4. Multiple operations require switching between different commands
5. Cannot conveniently sort and filter data

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Database connection**: Open and connect to SQLite database files
2. **Table browsing**: Display all tables in the database in a list
3. **Data viewing**: Display complete data records in tables
4. **Table structure viewing**: Display table creation statements (schema)
5. **SQL query execution**: Execute custom SQL query statements
6. **Data sorting**: Sort by specified columns in ascending or descending order
7. **Data filtering**: Filter data by specified conditions including exact match, contains match, and regular expression match
8. **Data editing**: Modify data records in tables

## Technology Stack
- Programming language: Rust
- TUI framework: Must use a Rust TUI framework (e.g., ratatui, tui-rs)
- Build tool: Rust standard toolchain (cargo)

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has `ca-certificates`, `libssl3`, `sqlite3`, `libsqlite3-0`, `bash`, and other base dependencies installed
3. **Launch and data paths** (first command-line argument is database file; can be overridden):
   - Default database: `/bench/data/bench.db`
4. **Launch command**: Default launch command is `toolo /bench/data/bench.db` (database path can be overridden via launch arguments)
5. **Installation**: Create a folder `toolo` containing all source code; install with `cd toolo && cargo install --path .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toolo/`, system command after install is `toolo`. Public project name **toolo**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Open database**: Open SQLite database (default `/bench/data/bench.db`; can be overridden via launch arguments); automatically discover **all tables** in the database (do not assume fixed table or column names).
- **Table data view**: Display multiple rows of the selected table; **all column values per row** must be visible in the same table view (scrollable).
- **Sort**: Ascending/descending sort by any column; after sorting, locate extreme-value row and display complete record (same screen).
- **Column filter**: Open filter on any column supporting at least: **substring contains**, **regex match**, **exact equals**; after filtering show only matching rows and match count.
- **Row edit**: Modify column values for a specified row and write back to database; after save display updated complete row on same screen.
- **Multiple tables**: Must support multiple tables with different structures in the database (column names and types per `schema`).

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support real-time search and filtering
5. Support switching between views (table list, data view, structure view)

## Naming Rules
- Project name: toolo
- Source directory: `toolo/`
- Command after install: `toolo`

Please develop the complete TUI tool project according to the requirements above.
