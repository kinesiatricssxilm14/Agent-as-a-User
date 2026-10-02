# toolh

`toolh` is a keyboard-first code snippet manager built with Python, Textual, and
SQLite. It stores real data locally and presents the snippet list, metadata,
description, and complete code body together in one screen.

## Install and run

```sh
cd toolh
pip install .
toolh
```

Python 3.9 or newer is required.

## Storage

By default, snippets are stored in:

```text
${XDG_DATA_HOME:-~/.local/share}/toolh/snippets.db
```

Override the exact database path:

```sh
TOOLH_DB_PATH=/some/place/library.sqlite3 toolh
```

Or override the data directory:

```sh
TOOLH_DATA_DIR=/some/place toolh
```

`TOOLH_DB_PATH` takes precedence. Parent directories are created
automatically.

## Keyboard controls

The application displays the primary controls in its footer. Press `?` at any
time on the library screen for the full keyboard reference.

| Key | Action |
| --- | --- |
| `↑` / `↓`, `j` / `k` | Select a snippet |
| `Enter` | Focus the code/detail pane |
| `/` | Focus live search |
| `n` | Create a snippet |
| `e` | Edit the selected snippet, including its title |
| `d` | Delete the selected snippet (with confirmation) |
| `y` / `n`, `Enter` / `Esc` | Confirm or cancel deletion |
| `c` | Copy code to the terminal clipboard |
| `r` | Refresh from SQLite |
| `?` | Open help |
| `q` | Quit |

In the create/edit form, use `Tab` and `Shift+Tab` to move between fields,
`Ctrl+S` to save, and `Esc` to cancel. Tags are entered as comma-separated
text.

## Clipboard behavior

Textual's terminal clipboard integration is used, so copying works through the
terminal (including terminals supporting OSC 52). The code remains available
in the detail pane if the terminal does not grant clipboard access.

