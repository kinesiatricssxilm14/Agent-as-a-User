# toolb

A keyboard-driven kanban board TUI for a board kept as plain text on disk.

`toolb` reads and writes an ordinary directory of files. There is no database and no hidden
state: `board.txt` declares the columns, each column is a directory of Markdown cards, and
`order.txt` records the order of the cards within it. Anything `toolb` shows you can also be
inspected with `cat`, edited with an editor, diffed, and committed to version control.

## Install

```sh
cd toolb && cargo install --path .
```

This puts a `toolb` executable on your `PATH`. Requires Rust 1.74 or newer.

## Run

```sh
toolb                        # open the default board root, /bench/data/board
toolb /srv/board             # open a specific board root
toolb --board /srv/board     # the same, spelled out
toolb --init                 # create the board directory and default columns, then exit
toolb --help                 # full usage, including how paths are resolved
```

The board root is resolved in this order, highest priority first:

1. `--board`, or the first positional argument
2. the `TOOLB_BOARD` environment variable
3. `board_root` in the configuration file
4. `/bench/data/board`

The configuration file is resolved as `--config`, then `$TOOLB_CONFIG`,
then `$XDG_CONFIG_HOME/toolb/config.conf`, then `$HOME/.config/toolb/config.conf`, and finally
`/root/.config/toolb/config.conf`. It is a plain `key = value` file; comments start with `#` or
`;`. You can change any key from inside the application with `C`, and the board root specifically
with `S` — both are written back to the file immediately.

Pointing `toolb` at an empty or non-existent directory is fine. Nothing is written until you ask
for it: press `I` to create a board with the default columns, or `c` to add a single column.

## Board layout

```
<root>/board.txt                     col <column_id> "<display name>"
<root>/cols/<column_id>/order.txt    one card id per line, no extension
<root>/cols/<column_id>/<card_id>.md line 1 is "# <title>", the rest is the body
```

For example, a column with id `todo` shown as `TO DO`, holding one card `item-1`:

```
$ cat board.txt
col todo "TO DO"

$ cat cols/todo/order.txt
item-1

$ cat cols/todo/item-1.md
# Fix login bug
Investigate timeout on mobile clients.
```

Columns have both an **id** and a **display name**. The id determines the directory on disk and
never changes once created; the display name is what you see and type in the interface. Renaming a
column (`r`) changes only the display name, so no files move and no links break.

## Keys

Press `?` at any time for the full list, shown alongside the board rather than over it. The bottom
bar always shows the most common keys for whatever you are currently doing.

| Key | Action |
| --- | --- |
| `←` `→` `h` `l`, `Tab` | Select a column |
| `↑` `↓` `k` `j` | Select a card |
| `g` `G` `Home` `End`, `PgUp` `PgDn` | Jump and page through a column |
| `Enter` | Focus the card pane, so `↑` `↓` scroll a long body; `Esc` returns |
| `t` | Go to a column by display name (`Tab` completes) |
| `n` | New card — asks for a title, then an id (prefilled from the title) |
| `e` | Edit the card title, keeping the `# ` prefix |
| `a` | Append one line to the card body |
| `B` | Edit the whole body in a multi-line editor (`Ctrl+S` saves, `Esc` discards) |
| `d` | Delete the card, after a `y`/`N` confirmation |
| `m` | Move the card to a column chosen by display name |
| `H` `L`, `Ctrl+←` `Ctrl+→` | Move the card to the previous / next column |
| `K` `J` | Reorder the card within its column |
| `c` | Create a column — asks for a display name, then an id |
| `r` | Rename the selected column's display name |
| `O` | Rewrite `order.txt` to match what is displayed |
| `I` | Create `board.txt` and the default columns in an empty root |
| `/` | Filter cards by id, title or body; `Ctrl+n` / `Ctrl+p` step through matches |
| `S` `C` | Set the board root / any configuration key, saved to the config file |
| `R` | Re-read the board from disk |
| `?` | Show or hide the key list |
| `q`, `Ctrl+C` | Quit |

Everything is reachable from the keyboard; no mouse is needed. While a prompt is open, letter keys
are text rather than commands, `Enter` confirms and `Esc` cancels.

## Layout

The interface is a single screen with no overlays. Prompts, messages and the help pane take up
their own rows and shrink the panes around them, rather than covering content, so what you can see
after an operation is never hidden behind something else:

* **Board** — every column side by side, each with its display name, id, card count and cards.
  Columns shrink to fit so that as many as possible are visible at once.
* **Card** — the selected card's id, full title, absolute file path and complete body. It updates
  as soon as the selection moves; long bodies scroll and show a scrollbar.
* **Columns / Config** — every column with its id and card count, plus the board root, where that
  value came from, the configuration file path and its contents. Because every column is always
  listed here, none is ever completely off screen even on a narrow terminal.

## Behaviour worth knowing

`toolb` is written to be safe against a board that other tools also touch.

* **Reading never writes.** Opening a board leaves every file byte-for-byte as it was, including
  its modification time.
* **Writes are atomic.** Each file is written to a temporary file in the same directory and
  renamed into place, so a reader never sees a half-written file.
* **Interruptions heal.** A card file with no `order.txt` entry is shown and appended; an entry
  with no file is ignored. So an interrupted create, move or delete loses nothing and duplicates
  nothing — the next load reconciles it.
* **Formatting is preserved.** Rewriting `board.txt` keeps its comments, blank lines and original
  quoting; only the line being changed is touched. CRLF files stay CRLF. Appending to a card never
  removes blank lines you put there deliberately.
* **Awkward input is tolerated.** Unquoted display names, `.md` suffixes in `order.txt`, duplicate
  entries, missing title lines, empty files and invalid UTF-8 are all handled, and anything
  surprising is reported in the interface instead of being silently ignored.
* **Ids are validated.** Anything that would escape the board directory — a path separator, `..`, a
  leading dot — is refused, for both card and column ids.

## Development

```sh
cargo test      # unit and integration tests
cargo clippy    # lints
```

The tests cover the file format byte-for-byte, the recovery cases above, and the interface itself:
`tests/render.rs` draws real frames and asserts that all the columns, the selected card's full
title and every line of its body appear together in one screen, including while a prompt or the
help pane is open.
