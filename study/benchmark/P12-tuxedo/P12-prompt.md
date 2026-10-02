# tooll: Task Management TUI Development Prompt

## Project Overview
Develop a task management text user interface (TUI) tool named "tooll" for managing text-based task lists. The tool should provide an intuitive interactive interface so users can efficiently manage task priority, project tags, and contexts.

## Scenario Description
Suppose you are a terminal user who needs to manage daily tasks in a command-line environment. You have text-based task lists, but face the following problems:
1. Lack of an intuitive interface to view and manage tasks
2. Cannot conveniently change task priority and tags
3. Hard to quickly filter tasks for a specific project
4. Marking tasks complete requires manually editing text files

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Task list display**: Display all current tasks including priority, project tags, and context tags
2. **Add new tasks**: Add new tasks with priority, project tags, and context tags
3. **Change task priority**: Modify priority of existing tasks
4. **Mark tasks complete**: Mark tasks as completed
5. **Filter by project**: Filter tasks by project tag
6. **Filter by context**: Filter tasks by context tag

## Data Format Description
Tasks are stored in todo.txt format with the following specification:
- **Priority**: Uppercase letter in parentheses, e.g., `(A)` is highest priority, `(B)` next, and so on
- **Project tags**: `+` prefix, e.g., `+work`, `+personal`
- **Context tags**: `@` prefix, e.g., `@computer`, `@phone`
- **Due date**: `due:YYYY-MM-DD` format, e.g., `due:2029-12-12`
- **Completion marker**: Completed tasks have `x ` prefix at line start (x followed by space)

**Examples** (one task per line; for format understanding only, not fixed test data):

- Incomplete: `(B) Buy groceries +errands @home due:2026-03-15`
- Complete: `x (A) Pay rent +finance @computer due:2026-01-01`

## Technology Stack
- Programming language: Rust
- TUI framework: Must use ratatui from the Rust TUI ecosystem (with crossterm backend)
- Build tool: Cargo

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has ca-certificates, libssl3, and other base dependencies installed
3. **Data path** (must support override via launch arguments or configuration):
   - Default task file: `/bench/data/todo.txt`
4. **Launch command**: Default launch command is `tooll` (default task file `/bench/data/todo.txt`; can be overridden via launch arguments or configuration)
5. **Installation**: Create a folder `tooll` containing all source code; install with `cd tooll && cargo install --path .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `tooll/`, system command after install is `tooll`. Public project name **tooll**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **todo.txt format**: Parse and write `todo.txt` syntax (priority parentheses, description, `+project`, `@context`, due dates, etc.; see "Data Format Description" above).
- **Default file**: Default task list file `/bench/data/todo.txt` (can be overridden via launch arguments or configuration).
- **List view**: Display readable line text for multiple tasks on the same screen.
- **Priority**: Support changing task priority (e.g., upgrade `(B)` to `(A)`); write back to file and update interface.
- **Complete**: Mark tasks as complete (`x` prefix); write back to file and update interface.
- **Context tags**: Support adding/modifying `@tags` for tasks; write back to file.
- **Authenticity**: Interface state must match disk file content.

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support real-time search and filtering

## Naming Rules
- Project name: tooll
- Source directory: `tooll/`
- Command after install: `tooll`

Please develop the complete TUI tool project according to the requirements above.
