# tooln: Python Package Management TUI Development Prompt

## Project Overview
Develop a Python package management text user interface (TUI) tool named "tooln" to simplify pip package management in Python environments. The tool should provide a lazygit-style intuitive interactive interface so developers can efficiently browse, search, install, uninstall, and view detailed Python package information without memorizing complex command-line arguments.

## Scenario Description
Suppose you are a Python developer who frequently manages dependency packages in Python environments. You currently use command-line tools (such as pip) for package management, but face the following problems:
1. Command arguments are complex and easy to misremember
2. Operation steps are tedious and inefficient
3. Lack of an intuitive interface to view package information and dependencies
4. Multiple operations require switching between different commands
5. Cannot manage pip and system packages (such as apt) simultaneously

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Package browsing**: Display installed pip packages in a list; support switching between pip packages and system packages
2. **Package search**: Search Python packages by name or keyword
3. **Package installation**: Select and install new Python packages from PyPI
4. **Package uninstallation**: Remove installed Python packages or system packages
5. **Package details**: Display full version numbers, dependencies, and other detailed information
6. **View switching**: Switch between pip package list and system package (apt) list

## Technology Stack
- Programming language: Go
- TUI framework: Must use the Bubble Tea framework (with Lip Gloss and Bubbles component libraries)
- Build tool: Go standard toolchain

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has `ca-certificates`, `python3-pip`, `python3`, `python3-venv`, and other base dependencies installed
3. **Data**: Package data comes from pip/apt in the container; does not depend on `/bench/data` data files
4. **Launch command**: Default launch command is `tooln` (no path arguments)
5. **Installation**: Create a folder `tooln` containing all source code; install with `cd tooln && go install .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `tooln/`, system command after install is `tooln`. Public project name **tooln**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Multi-manager**: Tab switch between pip / apt package manager views.
- **pip list**: Display installed Python packages and versions.
- **apt list**: Display installed system packages and versions.
- **Install pip package**: Install user-specified package; after installation, package and **direct dependency** names visible in details/list.
- **Uninstall pip package**: After uninstall, package and no-longer-needed dependencies no longer appear in interface.
- **Uninstall apt package**: After uninstall, package no longer appears in interface.
- **Upgrade pip package**: Upgrade specified package; interface shows new version number, old version no longer appears.
- **Install apt package**: After installation, new package appears in interface.
- **Refresh**: Support list refresh (rescan) after install/uninstall/upgrade to reflect real environment.

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support real-time search and filtering
5. Support view switching between pip packages and system packages

## Naming Rules
- Project name: tooln
- Source directory: `tooln/`
- Command after install: `tooln`

Please develop the complete TUI tool project according to the requirements above.
