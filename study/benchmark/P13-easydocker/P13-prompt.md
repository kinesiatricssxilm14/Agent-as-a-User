# toolm: Docker Container Management TUI Development Prompt

## Project Overview
Develop a Docker container management text user interface (TUI) tool named "toolm" for managing and monitoring Docker containers, images, networks, and volumes through an intuitive interactive interface. The tool should provide comprehensive Docker resource management so users can efficiently view and operate on various resources in a Docker environment.

## Scenario Description
Suppose you are a system administrator or developer who frequently manages Docker container environments. Although command-line tools can execute Docker operations, they face the following problems:
1. Must memorize complex Docker commands and arguments
2. Hard to intuitively view status and relationships of containers, images, networks, and volumes
3. Viewing container logs and details requires frequently switching commands
4. Lack of a unified interface to manage all Docker resources

## Functional Requirements
Develop a TUI tool with the following core capabilities:
1. **Container list**: Display all Docker containers including running and stopped, showing container name, image, status, and other key information
2. **Container logs**: View complete log output for a specified container with scroll support
3. **Container details**: View detailed container configuration including ID, creation time, port mappings, command, etc.
4. **Image list**: Display all Docker images including name, tag, size, ID, etc.
5. **Network list**: Display all Docker networks including network name and driver type
6. **Volume list**: Display all Docker volumes including volume name and driver type
7. **Volume details**: View detailed information for a specified volume including name, driver, and mount point path

## Technology Stack
- Programming language: Go
- TUI framework: Must use Bubble Tea (bubbletea) (with Lip Gloss styling library)
- Build tool: Go modules

## Environment Constraints
1. **Runtime environment**: The tool runs as root inside a Docker container, communicating with the Docker daemon via a simulated Docker API
2. **System dependencies**: The container has ca-certificates, docker-cli, bash, and other base dependencies installed
3. **Connection**:
   - Access resources via local container API socket (API responses may be provided by a local mock socket)
4. **Launch command**: Default launch command is `toolm` (no path arguments)
5. **Installation**: Create a folder `toolm` containing all source code; install with `cd toolm && go install .`

6. **Build notes**: This document only specifies the source directory, install command, and executable entry point (see "Installation" under Environment Constraints); third-party libraries introduced during the build are managed by the implementer. Aside from the language and TUI framework requirements in "Technology Stack", no additional restrictions are imposed here.

## UI and Delivery Guidelines

1. **Deliverable**: Source directory `toolm/`, system command after install is `toolm`. Public project name **toolm**.
2. **Authenticity**: All operations that read or write system state must call real underlying tools (package managers, filesystem, databases, Git, etc.); pure UI simulation is prohibited.
3. **Same-screen visibility**: When the interface needs to display multiple fields, values, list items, or panel content, all of that content must be visible simultaneously in the **same view** after the operation completes (scrolling allowed; pagination, tab switching, or modal overlays that hide key information from the current screen snapshot are prohibited).
4. **Task-driven**: Specific package names, file names, table names, key names, regular expressions, paths, etc. are provided by runtime task descriptions; the implementation must support the corresponding operation for **any valid input** and must not hard-code logic for specific inputs.
5. **Keyboard-only operation**: All functionality must be completed using only the keyboard; mouse input must not be required. Browsing, selection, confirmation, cancellation, input, search, pagination, etc. must each be bound to explicit keys.
6. **Discoverable shortcuts**: The interface must provide explorable key documentation (e.g., bottom status bar, help page, shortcut list, key labels beside menu items, etc.); the exact presentation is up to you. Users should be able to discover which keys correspond to each function through browsing and experimentation within the TUI, without consulting external documentation.
7. **User experience and auxiliary features**: Focus on reasonable interaction and ease of onboarding; key bindings and layout should facilitate core operations. You may implement auxiliary features not listed item-by-item (e.g., operation feedback, input confirmation, list filtering, error messages) to lower the barrier to use, but you must comply with "Authenticity" and "Task-driven"; do not replace real capabilities with simulation or hard-coded behavior.

## Capability Specification (must be fully implemented)

- **Docker resource browsing** (connect to local Docker API; deployment may provide API via mock socket, no real daemon required):
  - **Container list**: Running and stopped containers; same screen must include at least name and image.
  - **Container details**: Name, image (with tag), port mappings, start command, ID, and other fields on the same screen.
  - **Container logs**: Display complete log output after selecting a container (scrollable on same screen).
  - **Image list**: Name, tag, size (MB, **one fixed decimal place**, e.g., `143.1 MB`, `256.0 MB`), Image ID on the same screen.
  - **Image details**: Same fields in detail view on the same screen.
  - **Network list**: Name and driver on the same screen.
  - **Volume list**: Name and driver on the same screen.
  - **Volume details**: Name, driver, mountpoint (absolute path) on the same screen.

## User Interface Requirements
1. **Keyboard-only and discoverability**: Must satisfy "Keyboard-only operation" and "Discoverable shortcuts" in "UI and Delivery Guidelines"; common navigation keys (arrow keys, Enter, Esc, Tab, etc.) must complete browsing and confirmation in the corresponding scenarios.
2. Provide clear prompts and help documentation
3. Reasonable layout with clear information display
4. Support multi-view switching (containers, images, networks, volumes)
5. Selected items must have clear visual feedback
6. Support pagination or scrolling for long lists

## Naming Rules
- Project name: toolm
- Source directory: `toolm/`
- Command after install: `toolm`

Please develop the complete TUI tool project according to the requirements above.
