# toolh

**toolh** is a keyboard-first code snippet management TUI. Organize, search,
edit, and copy your frequently used code snippets from the comfort of your
terminal.

Built with [Textual](https://textual.textualize.io/) and backed by SQLite.

## Features

- **Create** snippets with title, language, tags, description, and code body
- **Edit** every field of an existing snippet
- **Rename** a snippet (change its title)
- **Delete** snippets with confirmation
- **Search** across title, language, tags, description, and code — in real time
- **Copy** a snippet's code to the system clipboard
- **Persistent storage** in a local SQLite database
- **Fully keyboard-driven** with discoverable shortcuts (footer bar + `?` help)

## Installation

```bash
cd toolh
pip install .
```

This installs the `toolh` command.

## Usage

```bash
toolh                # launch the TUI
toolh --version      # show version
toolh --print-db     # print the resolved database path
toolh --db /path/to/snippets.db   # use a specific database file
```

### Keyboard shortcuts

| Key | Action |
| --- | --- |
| `↑`/`↓` or `j`/`k` | Move the highlight through the snippet list |
| `Enter` | Return focus to the list (from search) |
| `/` | Focus the search box and filter in real time |
| `n` | Create a new snippet |
| `e` | Edit the highlighted snippet |
| `r` | Rename the highlighted snippet |
| `c` | Copy the highlighted snippet's code |
| `d` | Delete the highlighted snippet |
| `?` | Show the help screen |
| `Ctrl+S` | Save the current form |
| `Esc` | Cancel / go back |
| `Tab` | Move between form fields |
| `q` / `Ctrl+Q` | Quit |

## Storage location

Snippets are stored in a SQLite database. The path is resolved in this order:

1. `--db PATH` command-line argument
2. `TOOLH_DB` environment variable (full path to the database file)
3. `database` key in the config file
4. `TOOLH_DATA_DIR` environment variable or `data_dir` key in the config file
   (the file `snippets.db` is created inside that directory)
5. Default: `~/.local/share/toolh/snippets.db`

The config file is TOML and lives at `~/.config/toolh/config.toml`
(or under `$XDG_CONFIG_HOME`). Example:

```toml
# ~/.config/toolh/config.toml
database = "/home/me/snippets/snippets.db"
# or: data_dir = "/home/me/snippets"
```

You can verify which path will be used with `toolh --print-db`.

## Data model

Each snippet stores:

- `title` — short name (required)
- `language` — programming language (free text, used for highlighting)
- `tags` — comma-separated tags
- `description` — what the snippet does
- `code` — the snippet body

The detail panel shows all five fields on the same screen.

## Development

```bash
python -m venv .venv
.venv/bin/pip install -e .
.venv/bin/toolh
```

Run the tests:

```bash
.venv/bin/python -m unittest discover -s tests
```
