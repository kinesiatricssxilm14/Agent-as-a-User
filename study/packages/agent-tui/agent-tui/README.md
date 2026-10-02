# agent-tui

`agent-tui` is a robust CLI tool designed for LLM Agents to interact with Terminal User Interfaces (TUI) applications. It acts as a drop-in replacement for existing tools like `tui-use`, with several key improvements:

1. **Stable Input**: Uses `tmux` under the hood to ensure perfect handling of VT100 escape sequences, including arrow keys and special characters.
2. **Visual Semantics Preservation**: Captures ANSI color codes and converts them to multiple modalities (JSON, semantic XML, high-fidelity PNG) so that LLMs can understand critical visual cues like red errors or green highlights.

## Requirements

- Linux or macOS
- Python 3.8+
- `tmux` installed and available in `$PATH`
- `wkhtmltopdf` (optional, for rendering PNG snapshots)

## Installation

You can install `agent-tui` globally using `pip`:

```bash
pip install -e .
```

## Usage

`agent-tui` provides a strict CLI interface:

```bash
# Session Management
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

# Interactions
agent-tui type <text>                            # Type literal text
agent-tui type "<text>\n"                        # Type with Enter
agent-tui type "<text>\t"                        # Type with Tab
agent-tui paste "<text>\n<text>\n"               # Multi-line paste
agent-tui press <key>                            # Press a special key (Strict validation required!)
agent-tui scrollup <n>                           # Scroll up to older content
agent-tui scrolldown <n>                         # Scroll down to newer content
agent-tui find <pattern>                         # Search in screen (regex)

# Waiting & Synchronization
agent-tui wait                                   # Wait for screen change (default timeout: 3000ms)
agent-tui wait <ms>                              # Custom timeout, e.g. wait 5000
agent-tui wait --text <pattern>                  # Wait until screen contains pattern
agent-tui wait --debounce <ms>                   # Idle time after last change before resolving (default: 100ms)
agent-tui wait --format json                     # JSON output after waiting

# Multimodal Snapshots
agent-tui snapshot                               # Get current screen (plain text by default)
agent-tui snapshot --format json                 # JSON output with cursor and lines
agent-tui snapshot --format semantic             # XML tags for ANSI colors (e.g., <fg:red>Error</fg:red>)
agent-tui snapshot --format png                  # High-fidelity PNG rendering

# Daemon Management
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
- `enter`, `tab`, `escape`, `backspace`, `delete`
- `f1` to `f10`

## Architecture

- **`cli.py`**: Provides the strict `click`-based command-line interface.
- **`tmux_backend.py`**: Handles all the underlying `tmux` states, key mappings, debounce-wait loops, and screen capturing.
- **`ansi_parser.py`**: Converts raw ANSI text from `tmux` into `plain`, `json`, `semantic` XML, and `png` images.

## Testing

Run tests with `pytest`:

```bash
pytest tests/
```
