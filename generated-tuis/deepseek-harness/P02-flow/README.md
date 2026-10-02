# toolb

`toolb` is a keyboard-only kanban board management TUI written in Rust. It
manages task boards stored as plain files on the real filesystem, so every
operation (create/edit/delete/move cards, create/delete columns) reads from and
writes to actual files.

## Install

```sh
cd toolb && cargo install --path .
```

The `toolb` executable is then placed on `PATH`.

## Run

```sh
toolb                       # uses the default board root /bench/data/board
toolb --board /some/board   # use a specific board root for this run
```

Config management (non-interactive):

```sh
toolb --show-board-root            # print the effective board root
toolb --set-board-root /my/board   # persist the default board root to config
toolb --config /path/config.toml ... # use a specific config file
```

## Board directory structure

Relative to the board root (default `/bench/data/board`, configurable):

```text
board.txt                       # column definitions: col <column_id> "<display_name>"
cols/<column_id>/order.txt      # card ids, one per line (no extension)
cols/<column_id>/<card_id>.md   # line 1 is `# <title>`, then the body text
```

Example:

```text
board.txt:
  col todo "TO DO"

cols/todo/order.txt:
  item-1

cols/todo/item-1.md:
  # Fix login bug
  Investigate timeout on mobile clients.
```

The UI identifies columns by **display name**; on disk, paths use the **column
id** (`cols/<column_id>/`). Card ids are the file names without the `.md`
extension.

## Configuration

The default board root is read from `$XDG_CONFIG_HOME/toolb/config.toml`
(falling back to `~/.config/toolb/config.toml`):

```toml
board_root = "/bench/data/board"
```

`TOOLB_CONFIG` or `--config` overrides the config file path; `--board`
overrides the board root for a single run.

## Keyboard reference

| Key | Action |
| --- | --- |
| `↑`/`↓` or `j`/`k` | select card up/down |
| `←`/`→` or `h`/`l` | select column left/right |
| `Tab` | switch focus between the board and the card-details panel |
| `g` / `G` | jump to first / last card |
| `n` | create a new card in the current column |
| `e` / `Enter` | edit the selected card's title |
| `a` | append a line to the selected card's body |
| `d` | delete the selected card (confirmation) |
| `<` / `>` | move the selected card to the previous / next column |
| `c` | create a new column |
| `D` | delete the current column (confirmation) |
| `/` | filter cards by text substring (Esc clears) |
| `r` | reload the board from disk |
| `?` | show the help page |
| `q` / `Esc` | quit |
| `Ctrl+C` | quit from anywhere |

In a text-input prompt: `Enter` confirms, `Esc` cancels, `←`/`→` move the
cursor, `Backspace`/`Delete` edit, `Ctrl+U` clears the line, and `Ctrl+W`
deletes the previous word.
