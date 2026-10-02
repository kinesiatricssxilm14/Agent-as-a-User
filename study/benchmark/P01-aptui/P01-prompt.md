# toola: System Package Management TUI Development Prompt

## Project Overview
Develop a system package management text user interface (TUI) tool named "toola" to simplify software package management on Linux systems. The tool should provide an intuitive interactive interface so administrators can efficiently browse, search, install, uninstall, and upgrade packages without memorizing complex command-line arguments.

## Scenario Description
Suppose you are a Linux system administrator who frequently manages packages on servers. You currently use command-line tools (such as apt and dpkg) for package management, but face the following problems:
1. Command arguments are complex and easy to misremember
2. Operation steps are tedious and inefficient
3. Lack of an intuitive interface to view package information and dependencies
4. Multiple operations require switching between different commands

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Package browsing**: Display installed and available packages in a list
2. **Package search**: Search packages by name or keyword
3. **Package installation**: Select and install new packages from repositories
4. **Package uninstallation**: Remove installed packages
5. **Package details**: Show full description, dependencies, version, and other information
6. **Package upgrade**: Check for upgradable packages and perform upgrades

## Technology Stack
- Programming language: Go
- TUI framework: Must use the Bubble Tea framework (with Lip Gloss and Bubbles component libraries)
- Build tool: Go standard toolchain

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has `ca-certificates`, `apt-utils`, `sudo`, and other base dependencies installed
3. **Data**: Package data comes from apt; no fixed data file paths are required
4. **Launch command**: Default launch command is `sudo toola` (no extra arguments)
5. **Installation**: Create a folder `toola` containing all source code; install with `cd toola && go install .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toola/`, system command after install is `toola`. Public project name **toola**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Package browsing and search**: List installed/available packages; support search by name.
- **Install / uninstall / upgrade**: Execute via apt for real operations; system state must match the interface after operations.
- **Details panel**: After selecting a package, display the full description and **all dependency package names** in a fixed area on the same screen (listed one by one; a one-line summary is not sufficient).
- **Upgrade detection**: List upgradable packages and perform upgrades; after upgrade, the package no longer appears in the upgradable list.

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support real-time search and filtering

## Naming Rules
- Project name: toola
- Source directory: `toola/`
- Command after install: `sudo toola`

Please develop the complete TUI tool project according to the requirements above.
