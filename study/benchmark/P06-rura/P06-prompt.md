# toolf: Shell Pipeline Debugging TUI Development Prompt

## Project Overview
Develop a Shell pipeline debugging text user interface (TUI) tool named "toolf" for interactively building, debugging, and executing Shell pipeline commands. The tool should provide an intuitive interactive interface so users can conveniently write pipeline commands, view intermediate results, and save output.

## Scenario Description
Suppose you are a system administrator who needs to analyze server log files. You currently combine command-line tools to process logs, but face the following problems:
1. Must repeatedly execute different pipeline commands to view intermediate results
2. Cannot conveniently debug complex pipeline commands
3. Lack of real-time preview and syntax highlighting
4. Must manually save output results to files

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Pipeline command editing**: Write and edit Shell pipeline commands in the interface
2. **Command execution**: Execute complete pipeline commands and view output
3. **Partial execution debugging**: Select and execute part of a pipeline to view intermediate results
4. **Syntax highlighting**: Pipeline commands should support syntax highlighting
5. **Tab completion**: Support Tab completion for commands and paths
6. **Result saving**: Save current output to a specified file

## Technology Stack
- Programming language: Rust
- TUI framework: Must use the ratatui framework (with crossterm backend)
- Build tool: Cargo

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has `ca-certificates`, `libssl3`, `bash`, and other base dependencies installed
3. **Launch and data paths** (`--file` specifies log file; can be overridden):
   - Default log file: `/bench/server.log` (under `/bench/`, not `/bench/data/`)
   - Pipeline results can be saved to task-specified paths (e.g., `/bench/data/result.txt`)
4. **Launch command**: Default launch command is `toolf --file /bench/server.log` (`--file` can specify another log file)
5. **Installation**: Create a folder `toolf` containing all source code; install with `cd toolf && cargo install --path .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toolf/`, system command after install is `toolf`. Public project name **toolf**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Log pipeline**: Edit shell-style pipeline commands against a log file (default `/bench/server.log`, specified via `--file`, can be overridden).
- **Execution and output**: Display output after executing the full pipeline; support common filters such as `tail`, `grep`, etc.
- **Partial execution**: Cursor can be placed at pipeline subcommand boundaries; provide a shortcut to execute the segment before the cursor (e.g., `Alt+\`); **the current command line must show the cursor and be visible on the same screen as the output area**.
- **Save output**: Support writing current output to a user-specified path (e.g., `/bench/data/result.txt`).

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support real-time preview and syntax highlighting

## Naming Rules
- Project name: toolf
- Source directory: `toolf/`
- Command after install: `toolf`

Please develop the complete TUI tool project according to the requirements above.
