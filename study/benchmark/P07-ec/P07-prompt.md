# toolg: Git Merge Conflict Resolution TUI Development Prompt

## Project Overview
Develop a Git merge conflict resolution text user interface (TUI) tool named "toolg" for visually resolving conflicts when merging Git branches in the terminal. The tool should provide an intuitive three-way merge interface so developers can efficiently view conflict content, choose resolution strategies, and complete merge operations.

## Scenario Description
Suppose you are a developer who frequently merges Git branches in team collaboration. When two branches modify the same region of the same file, merge conflicts occur. Current conflict resolution approaches face the following problems:
1. Must manually edit conflict files, which is error-prone
2. Lack of an intuitive interface to view conflict context
3. Cannot conveniently compare changes from different branches
4. After resolving conflicts, must manually run git commands to complete the merge

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Conflict viewing**: Display conflict files in a three-way merge interface with clearly marked conflict regions
2. **Conflict resolution**: Support multiple resolution strategies (choose feature branch, choose main branch, keep both, discard conflict section)
3. **File saving**: Save resolved file content
4. **Merge commit**: Commit merge results after conflict resolution
5. **History viewing**: View git commit history after merge

## Conflict File Format

During a merge conflict, working tree files contain standard Git conflict markers; the TUI displays and edits them in ours / result / theirs panels.

**Example** (for format understanding only, not fixed test data):

```
line before
<<<<<<< HEAD
main branch text
=======
feature branch text
>>>>>>> feature
line after
```

After resolution and write-back, `<<<<<<<` / `=======` / `>>>>>>>` markers must be removed; non-conflict lines such as `line before` and `line after` must be preserved.

## Technology Stack
- Programming language: Go
- TUI framework: Must use the Bubble Tea framework (with Lip Gloss and Bubbles component libraries)
- Build tool: Go standard toolchain

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside an Alpine Linux Docker container
2. **System dependencies**: The container has `ca-certificates`, `git`, `bash`, and other base dependencies installed
3. **Launch and data paths** (working directory and conflict file name can be overridden):
   - Default working directory: `/bench/data/repo`
   - Default open file: `conflict.py` (relative to working directory)
4. **Launch command**: Default launch command is `toolg conflict.py` (default working directory `/bench/data/repo`; can be overridden)
5. **Installation**: Create a folder `toolg` containing all source code; install with `cd toolg && go install .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toolg/`, system command after install is `toolg`. Public project name **toolg**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Three-way merge**: Open a file in merge-conflict state in the Git working tree; display ours / result / theirs panels.
- **Conflict highlighting**: Conflict-side text must use a **distinguishing background highlight**; both sides' content must be visible on the same screen.
- **Resolution strategies**: Provide operations to select ours, theirs, both, or none and write to result.
- **Preserve non-conflict lines**: When writing back the resolution, lines outside conflict markers must remain unchanged (only conflict block content is replaced).
- **Write-back and commit**: Write result back to the working tree file; support completing the merge commit (disk writes must take real effect).

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
3. Reasonable layout with clear three-way merge information display
4. Support highlighted display and distinguishing markers for conflict regions
5. Provide clear conflict resolution options and operation feedback

## Naming Rules
- Project name: toolg
- Source directory: `toolg/`
- Command after install: `toolg`

Please develop the complete TUI tool project according to the requirements above.
