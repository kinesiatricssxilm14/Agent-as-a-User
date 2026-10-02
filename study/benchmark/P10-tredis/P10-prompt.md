# toolj: Redis Database Management TUI Development Prompt

## Project Overview
Develop a Redis database management text user interface (TUI) tool named "toolj" for visually browsing and managing Redis key-value storage. The tool should provide an intuitive interactive interface so users can efficiently view, search, edit, and delete various Redis data types.

## Scenario Description
Suppose you are a database administrator or developer who frequently views and manages data in Redis databases. Although redis-cli can be used, it faces the following problems:
1. Command-line operations are not intuitive; many commands must be memorized
2. Cannot quickly browse and search large numbers of key-value pairs
3. Lack of a unified visual interface for multiple Redis data types (hash, list, set, sorted set, stream)
4. No convenient way to view PubSub channels and Streams messages

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Key browsing**: Browse all keys in Redis with type filtering
2. **Data viewing**: View complete content of different data types (string, hash, list, set, sorted set, stream)
3. **Search and filter**: Quickly search and filter key names by keyword
4. **Data editing**: Modify values of existing keys
5. **Data creation**: Create new key-value pairs
6. **Data deletion**: Delete specified keys
7. **PubSub management**: View all PubSub channel lists
8. **Streams management**: View all Streams and their messages
9. **ACL management**: View all ACL users and their permission configuration

## Technology Stack
- Programming language: Rust
- TUI framework: Must use ratatui (with crossterm terminal handling library)
- Build tool: Cargo

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a debian:12-slim Docker container
2. **System dependencies**: The container has ca-certificates, libssl3, redis-server, and other base dependencies installed
3. **Connection**:
   - Default Redis URI: `redis://localhost:6379/0` (local Redis service provided in the environment)
4. **Launch command**: Default launch command is `toolj` (no path arguments)
5. **Installation**: Create a folder `toolj` containing all source code; install with `cd toolj && cargo build --release && cp target/release/toolj /usr/local/bin/`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toolj/`, system command after install is `toolj`. Public project name **toolj**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Connection configuration**: Support adding named servers (name + Redis URI, e.g., `New_user` + `redis://localhost:6379/0`) and connecting; default `redis://localhost:6379/0`.
- **Resource navigation**: After connecting, switch via resource selector to views such as `:keys`, `:streams`, `:pubsub`, `:acl`.
- **Key browsing**: `:keys` view must display total key count and support viewing string / hash / list / set / sorted set / stream keys by type.
- **String keys**: Display key name and full value (same screen).
- **Hash keys**: Display all field/value pairs (same screen).
- **List keys**: Display all elements (same screen).
- **Streams**: List all stream names (complete list on same screen).
- **Pub/Sub**: List channel names on the current Redis instance (including subscribed channels; complete list on same screen).
- **ACL**: List all ACL usernames (complete list on same screen; includes `default` and custom users).

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support real-time search and filtering
5. Use appropriate display formats for different data types (e.g., tables, lists)

## Naming Rules
- Project name: toolj
- Source directory: `toolj/`
- Command after install: `toolj`

Please develop the complete TUI tool project according to the requirements above.
