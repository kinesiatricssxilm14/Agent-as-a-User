# toolc: File Manager TUI Development Prompt

## Project Overview
Develop a file management text user interface (TUI) tool named "toolc" for efficiently browsing, copying, moving, renaming, and deleting files on Linux systems. The tool should provide an intuitive dual-pane layout with file list navigation and real-time content preview so administrators can quickly complete everyday file management tasks.

## Scenario Description
Suppose you are a Linux system administrator who frequently performs file management on servers. You currently use command-line tools (such as cp, mv, rm, mkdir) for file management, but face the following problems:
1. Command arguments are complex; paths and syntax are easy to misremember
2. Operation steps are tedious and require repeatedly typing full paths
3. Lack of an intuitive interface to preview file content
4. Multiple operations require switching between different commands, reducing efficiency

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **File browsing**: Display files and subdirectories under a specified directory in a list
2. **File preview**: When a file is selected, show its content in a preview panel in real time
3. **File copy**: Copy a file from a source path to a destination path
4. **File rename**: Change a file name while keeping content unchanged
5. **File delete**: Delete a specified file
6. **Directory creation**: Create a new folder at a specified path

## Technology Stack
- Programming language: Rust
- TUI framework: Use a TUI framework from the Rust ecosystem
- Build tool: Cargo standard toolchain

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has `ca-certificates`, `libssl3`, `xdg-utils`, and other base dependencies installed
3. **Launch and data paths** (first command-line argument is the working directory; can be overridden):
   - Default open directory: `/bench/data/src`
4. **Launch command**: Default launch command is `toolc /bench/data/src` (working directory can be overridden via launch arguments)
5. **Installation**: Create a folder `toolc` containing all source code; install with `cd toolc && cargo install --path .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toolc/`, system command after install is `toolc`. Public project name **toolc**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Directory browsing**: Open the working directory on launch (default `/bench/data/src`; can be overridden via launch arguments); list + preview layout (at least file list and content preview on the same screen).
- **File preview**: After selecting a file, **file name and full file content** must be visible in the same interface.
- **Copy**: Copy a source file to a user-specified destination path with byte-identical content.
- **Move/archive**: Move a file into a user-specified directory (original path no longer exists after move).
- **Directory creation**: Create a new folder at a specified path (e.g., `archive`).
- **Rename**: Change a file name (original path disappears, new path appears, content unchanged).

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
3. Reasonable layout with a dual-pane design (file list + content preview)
4. Support hierarchical directory navigation

## Naming Rules
- Project name: toolc
- Source directory: `toolc/`
- Command after install: `toolc`

Please develop the complete TUI tool project according to the requirements above.
