# tooli: Disk Space Visualization TUI Development Prompt

## Project Overview
Develop a disk space visualization text user interface (TUI) tool named "tooli" that displays directory structure and file sizes in treemap form to help users quickly identify disk space usage. The tool should provide an intuitive interactive interface so users can browse directory trees, view file sizes, sort, filter, and delete files.

## Scenario Description
Suppose you are a system administrator who needs to analyze disk space usage for a directory on a server. The directory contains many files and subdirectories, but faces the following problems:
1. Using command-line tools to check file sizes one by one is inefficient
2. Hard to intuitively find the largest files and directories
3. Lack of a visual way to display directory structure
4. Cannot conveniently perform file management operations directly in the interface

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Directory tree display**: Open a specified directory and display all subdirectories and files in tree form with size for each entry
2. **Subdirectory navigation**: Enter subdirectories to view file details
3. **Treemap visualization**: Provide a treemap view mode where rectangle area intuitively represents file size proportion
4. **Sort by size**: Sort current directory contents by file size
5. **Filter by size**: Set a file size threshold to show only files larger than the specified size
6. **File deletion**: Select and delete files in the interface; operations must actually change filesystem state
7. **File count statistics**: Display file and subdirectory counts for a specified directory
8. **Refresh/rescan**: Refresh the current directory and rescan to update displayed file sizes

## Technology Stack
- Programming language: Rust
- TUI framework: Must use Rust's ratatui framework (with crossterm as terminal backend)
- Build tool: cargo

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has ca-certificates, libssl3, and other base dependencies installed
3. **Launch and data paths** (first command-line argument is scan root directory; can be overridden):
   - Default root directory: `/bench/data`
4. **Launch command**: Default launch command is `tooli /bench/data` (scan root can be overridden via launch arguments)
5. **Installation**: Create a folder `tooli` containing all source code; install with `cd tooli && cargo install --path .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `tooli/`, system command after install is `tooli`. Public project name **tooli**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Directory scan**: Recursively compute sizes for the root directory (default `/bench/data`; can be overridden via launch arguments); display in tree form.
- **Path and size on same screen**: Current directory path and its total size (GB or MB, **at least two decimal places with fixed zero-padding**, unit chosen by UI context) must be displayed in the same main view (e.g., `21.56 GB`, `149.00 MB`, `1234.56 MB`).
- **Largest item**: Locate and display the largest file/directory in the current scope and its size (same screen).
- **Top-N list**: List the top N file names sorted by size (multiple items must appear in the list view on the same screen).
- **Count**: Display the number of direct child items (files) in the current directory.
- **Delete**: Select and delete files in the tree; filesystem and interface must reflect changes after deletion.

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
2. Provide clear prompts informing users of available operations
3. Reasonable layout with intuitive switching between directory tree, file list, and treemap views
4. Support real-time file size display; data must accurately reflect actual filesystem state
5. File sizes must use **at least two fixed decimal places with zero-padding** (e.g., `21.56 GB`, `149.00 MB`); directory totals and largest items should use GB; subdirectories or smaller files may use MB (matching UI context)

## Naming Rules
- Project name: tooli
- Source directory: `tooli/`
- Command after install: `tooli`

Please develop the complete TUI tool project according to the requirements above.
