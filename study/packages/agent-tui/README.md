# agent-tui

`agent-tui` is a robust CLI tool designed for LLM Agents to interact with Terminal User Interfaces (TUI) applications. It acts as a drop-in replacement for existing tools like `tui-use`, with several killer features tailored specifically for AI evaluation:

1. **Perfect Key Mappings**: Uses `tmux` under the hood to ensure flawless handling of VT100 escape sequences, arrow keys, and special characters.
2. **TrueColor & OSC 4 Palette Hijacking**: Solves the notorious "degraded colors" problem in headless environments. It forces `xterm-256color` and intercepts OSC 4 palette redefinitions, guaranteeing that the snapshot generated looks exactly as it would in a modern VS Code terminal.
3. **Screen Change Detection**: Compares screen content and cursor position before/after an action, setting `has_changed` when either changes (e.g. arrow keys moving the cursor).
4. **System Mutation Detection**: Wraps sessions in `strace` to detect when interactions trigger underlying system state changes (file modifications, external processes, network connections, or signals), providing absolute observability.
5. **Strict JSON Output**: All CLI commands return standard JSON objects, making it incredibly reliable for LLM Agents to parse.

## Requirements

- Linux or macOS
- Python 3.8+
- `tmux` installed and available in `$PATH`
- `wkhtmltopdf` (optional legacy path for `png`/`jpg`/`pdf`; discontinued on Homebrew)
- **macOS recommended:** `brew install librsvg` (provides `rsvg-convert` for PNG via SVG fallback)
- `svg` works out of the box without any system dependencies.

## Installation

You can install `agent-tui` globally using `pip`:

```bash
pip install -e .
```

## Global JSON Output Format

All `agent-tui` commands output data exclusively in JSON format. This allows LLMs to deterministically verify whether a command succeeded and if it resulted in a screen update.

**Example: Successful action with a screen change and system mutation**
```json
{
  "success": true,
  "has_changed": true,
  "system_mutation": {
    "has_mutation": true,
    "process": {
      "changed": true,
      "details": [
        {
          "raw_log": "[pid 12345] execve(\"/usr/bin/git\", [\"git\", \"commit\", \"-m\", \"fix bug\"], 0x...) = 0",
          "parsed_command": "git commit -m 'fix bug'"
        }
      ]
    },
    "file": {
      "changed": false,
      "details": []
    },
    "network": {
      "changed": false,
      "details": []
    },
    "signal": {
      "changed": false,
      "details": []
    }
  }
}
```

**Example: Invalid action or error**
```json
{
  "success": false,
  "has_changed": false,
  "error": "Invalid key: 'invalid_key'. Valid keys are: ..."
}
```

**Example: Querying data (e.g., `snapshot`)**
```json
{
  "success": true,
  "has_changed": false,
  "data": {
    "snapshot": "snapshot.svg"
  }
}
```

*(Note: If `success` is false, the CLI also returns `Exit Code 1` for compatibility with strict shell scripts).*

## Usage

### Session Management

```bash
agent-tui start <cmd>                            # Start a program (generates a random session id)
agent-tui start --cwd <dir> <cmd>                # Start in specific directory
agent-tui start --cwd <dir> "<cmd> -flags"       # Quote the full command to pass flags
agent-tui start --label <name> <cmd>             # Start with label (session id)
agent-tui start --cols <n> --rows <n> <cmd>      # Custom terminal size (default: 120x30)
agent-tui use <session_id>                       # Switch to a session (save to local state)
agent-tui list                                   # List all sessions (tmux ls)
agent-tui info                                   # Show session details
agent-tui rename <label>                         # Rename session (tmux rename-session)
agent-tui kill                                   # Kill current session (tmux kill-session)
```

### Interactions
*All interaction commands return `"has_changed": true/false` when screen content or cursor position changed.*

```bash
agent-tui type <text>                            # Type literal text
agent-tui type "<text>\n"                        # Type with Enter
agent-tui type "<text>\t"                        # Type with Tab
agent-tui paste "<text>\n<text>\n"               # Multi-line paste
agent-tui press <key>                            # Press a special key (Strict validation required!)
agent-tui scrollup <n>                           # Scroll up to older content
agent-tui scrolldown <n>                         # Scroll down to newer content
agent-tui find <pattern>                         # Search in screen (regex)
```

### Waiting & Synchronization

```bash
agent-tui wait                                   # Wait for screen change (default timeout: 3000ms)
agent-tui wait <ms>                              # Custom timeout, e.g. wait 5000
agent-tui wait --text <pattern>                  # Wait until screen contains pattern
agent-tui wait --debounce <ms>                   # Idle time after last change before resolving (default: 100ms)
agent-tui wait --format svg                      # Combine wait and snapshot
```

### Multimodal Snapshots
`agent-tui` preserves 24-bit TrueColor and dynamically injected palettes to guarantee perfect visual fidelity.

```bash
agent-tui snapshot                               # Get current screen (plain text by default)
agent-tui snapshot --format json                 # JSON output with cursor and lines
agent-tui snapshot --format semantic             # XML tags for ANSI colors (e.g., <fg:red>Error</fg:red>)
agent-tui snapshot --format svg                  # [RECOMMENDED] High-fidelity SVG rendering (No dependencies needed)
agent-tui snapshot --format png                  # High-fidelity PNG rendering (Requires wkhtmltopdf)
agent-tui snapshot --format jpg                  # JPEG rendering
agent-tui snapshot --format pdf                  # PDF rendering
```

### Daemon Management

```bash
agent-tui daemon status                          # Check if daemon (tmux server) is running
agent-tui daemon stop                            # Stop the daemon (tmux kill-server)
agent-tui daemon restart                         # Restart the daemon
```

## Key Whitelist (`press` command)

The `press` command enforce strict key validation to prevent undefined behavior. Supported keys are:
- `ctrl+a` to `ctrl+z`
- `alt+a` to `alt+z`, plus `alt+\`, `alt+|`
- `arrow_up`, `arrow_down`, `arrow_right`, `arrow_left`
- `page_up`, `page_down`, `home`, `end`
- `enter`, `tab`, `space`, `escape`, `backspace`, `delete`
- `f1` to `f10`

## Architecture

- **`cli.py`**: Provides the strict `click`-based command-line interface with a unified JSON response wrapper.
- **`tmux_backend.py`**: Handles all the underlying `tmux` states, key mappings, debounce-wait loops, TrueColor forcing, OSC 4 palette tracking, and change detection hashes.
- **`syscall_parser.py`**: Monitors `strace` logs incrementally to detect underlying system mutations (`execve`, `unlink`, `rename`, `connect`, `kill`) triggered by the Agent's interactions.
- **`ansi_parser.py`**: Converts raw ANSI text from `tmux` into `plain`, `json`, `semantic` XML, and renders pixel-perfect `svg`/`png` images using `rich.console`.
