# toolh: Code Snippet Management TUI Development Prompt

## Project Overview
Develop a code snippet management text user interface (TUI) tool named "toolh" to help developers efficiently organize, store, and retrieve commonly used code snippets. The tool should provide an intuitive interactive interface so users can easily manage a personal code snippet library.

## Scenario Description
Suppose you are a software developer who frequently uses various code snippets in daily work. You have accumulated many code snippets in local files, but face the following problems:
1. Snippets are scattered across different files and hard to find quickly
2. Lack of a unified categorization and tagging system
3. Cannot conveniently copy snippets to the clipboard
4. Lack of persistent storage; data is easily lost

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Create snippets**: Create new snippets with title, language, description, tags, and code content
2. **Edit snippets**: Modify title, description, tags, or code content of existing snippets
3. **Delete snippets**: Delete snippets that are no longer needed
4. **Search snippets**: Search snippets by keyword
5. **Copy snippets**: Copy snippet content to the clipboard
6. **Persistent storage**: All snippet data must be persistently stored locally

## Technology Stack
- Programming language: Python
- TUI framework: Must use Python's Textual or Rich framework
- Build tool: pip

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has git, sqlite3, and other base dependencies installed
3. **Data**: Snippet persistence path is determined by the implementation (must support environment variable or configuration override and ensure UI display matches stored content)
4. **Launch command**: Default launch command is `toolh` (no path arguments)
5. **Installation**: Create a folder `toolh` containing all source code; install with `cd toolh && pip install .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toolh/`, system command after install is `toolh`. Public project name **toolh**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Persistence**: Snippet data must be persisted; storage path determined by implementation (must support environment variable or configuration override).
- **Fields**: Each snippet has title, language, tags, description, and code body; **detail view must display title, language, tags, description, and code body on the same screen**.
  - **Example** (for field understanding only, not fixed test data):
    - Title: `hello-world`
    - Language: `python`
    - Tags: `demo`
    - Description: Print a greeting message
    - Code: `print("hello")`
- **CRUD**: Create, view, rename (change title), edit content, delete; after deletion, snippet no longer appears in list or storage.

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support real-time search and filtering
5. Provide intuitive operation feedback such as successful save and delete notifications

## Naming Rules
- Project name: toolh
- Source directory: `toolh/`
- Command after install: `toolh`

Please develop the complete TUI tool project according to the requirements above.
