# toolb: Kanban Board Management TUI Development Prompt

## Project Overview
Develop a kanban board management text user interface (TUI) tool named "toolb" for managing task boards with column management and card CRUD. The tool should provide an intuitive interactive interface so users can efficiently organize and manage tasks.

## Scenario Description
Suppose you are a project manager who needs to manage a team's task board. The team currently uses simple text files to record tasks, but faces the following problems:
1. Cannot intuitively view task status and distribution
2. Lack of convenient task creation and editing
3. Difficult to move tasks between status columns
4. Cannot dynamically adjust board column structure

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Board view**: Display a multi-column board where each column represents a different task status
2. **Card management**: Create, edit, delete, and view card details
3. **Column management**: Create new columns and view existing columns
4. **Card movement**: Move cards between columns
5. **Filesystem integration**: All operations must read and write the real filesystem
6. **Configuration management**: Read and modify board configuration

## Technology Stack
- Programming language: Rust
- TUI framework: Must use ratatui (with crossterm terminal handling library)
- Build tool: Cargo

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a debian:12-slim Docker container
2. **System dependencies**: The container has ca-certificates, libssl3, vim, and other base dependencies installed
3. **Board data path**:
   - Default board root directory: `/bench/data/board`
   - Must support specifying another root directory via launch arguments or configuration file (exact method and parameter names are up to you)
4. **Launch command**: Default launch command is `toolb` (default board root `/bench/data/board`; can be overridden via launch arguments or configuration)
5. **Installation**: Create a folder `toolb` containing all source code; install with `cd toolb && cargo install --path .` so the `toolb` executable is on PATH

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toolb/`, system command after install is `toolb`. Public project name **toolb**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Board directory structure** (read/write files under board root `/bench/data/board`; path is configurable):
  - `board.txt`: Column definitions; each line `col <column_id> "<display_name>"` (e.g., `col todo "TO DO"`). The UI identifies columns by **display name**; disk paths use **column id**: `cols/<column_id>/`.
  - `cols/<column_id>/`: One subdirectory per column.
  - `cols/<column_id>/order.txt`: Card order for that column; one card id per line (no extension).
  - `cols/<column_id>/<card_id>.md`: Card file; task card ids are filenames (without `.md`). Line 1 is `# <title>`, followed by body text (may be empty or multi-line).
  - **Example** (column id `todo`, card id `item-1`; for format understanding only, not fixed test data):
    - `board.txt` line: `col todo "TO DO"`
    - `cols/todo/order.txt` line: `item-1`
    - `cols/todo/item-1.md` content: line 1 `# Fix login bug`, body e.g. `Investigate timeout on mobile clients.`
- **View**: Enter a column by **display name**; when a card is selected, show title and full body on the same screen.
- **Create**: Create a new card file in the specified column with title line `# <user-provided title>`; write to that column directory and maintain `order.txt`.
- **Edit title**: Modify line 1 of the card, keeping the `# ` prefix.
- **Append body**: Append a new line at the end of the card file.
- **Move between columns** (optional): Move card files between column directories and update each column's `order.txt`.

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support keyboard shortcuts to move cards between columns
5. Card details must show the full title and description content

## Naming Rules
- Project name: toolb
- Source directory: `toolb/`
- Command after install: `toolb`

Please develop the complete TUI tool project according to the requirements above.
