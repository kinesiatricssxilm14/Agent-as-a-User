# toole: Interactive Regular Expression Testing TUI Development Prompt

## Project Overview
Develop an interactive regular expression testing text user interface (TUI) tool named "toole" for real-time matching and highlighting of regex results in text files. The tool should provide an intuitive interactive interface so users can conveniently enter regular expressions, view match results, display match offsets, and support advanced features such as regex replacement and case-insensitive matching.

## Scenario Description
Suppose you are a developer or data analyst who frequently uses regular expressions to match and process text data. You currently use command-line tools (such as grep and sed) for regex matching, but face the following problems:
1. Cannot preview match results in real time; must repeatedly execute commands
2. Lack of intuitive highlighting to distinguish matched content
3. Cannot conveniently view offset positions of matched strings
4. Replacement operations lack real-time preview and are error-prone

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Regex matching**: Read a specified text file and match content according to an entered regular expression
2. **Result highlighting**: Display matched text with a prominent highlighted background
3. **Offset display**: Show start and end positions (0-indexed) beside each matched string
4. **Case-insensitive matching**: Support case-insensitive mode in regular expressions
5. **Regex replacement**: Replace matched content with a specified string and display the complete replaced content
6. **Phone number matching**: Support matching mainland China phone number format

## Technology Stack
- Programming language: Rust
- TUI framework: Must use a TUI framework from the Rust ecosystem (e.g., ratatui, crossterm)
- Build tool: Cargo

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Debian 12 slim Docker container
2. **System dependencies**: The container has `ca-certificates`, `libssl3`, and other base dependencies installed
3. **Launch and data paths** (`-f` specifies input file; can be overridden):
   - Default input file: `/bench/data/input.txt`
4. **Launch command**: Default launch command is `toole -f /bench/data/input.txt` (`-f` can specify another input file)
5. **Installation**: Create a folder `toole` containing all source code; install with `cd toole && cargo install --path .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toole/`, system command after install is `toole`. Public project name **toole**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Input file**: Load a text file on launch (default `/bench/data/input.txt`, specified via `-f`, can be overridden).
- **Regex matching**: Match the full text in real time; each match must be marked with a **background highlight distinguishable from body text** (foreground color alone is not sufficient).
- **Offsets**: Annotate start and end character offsets (0-indexed) for each match; all matches and their offsets must be displayed **on the same screen** in the results area.
- **Case insensitivity**: Support engine semantics such as `(?i)`; matched substrings may appear in the middle of tokens.
- **Replacement preview**: Support replacing matched fragments by rule and display the **complete replaced file content** (including unmodified lines).

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support real-time regex input with immediate match results
5. Clear separation between match results area and input area with distinct visual hierarchy

## Naming Rules
- Project name: toole
- Source directory: `toole/`
- Command after install: `toole`

Please develop the complete TUI tool project according to the requirements above.
