# toolb

`toolb` is a keyboard-only, filesystem-backed Kanban board TUI written in Rust with ratatui and crossterm.

## Install

```sh
cd toolb
cargo install --path .
```

## Run

```sh
toolb                         # /bench/data/board
toolb --board /another/path   # explicit board root
TOOLB_BOARD_ROOT=/path toolb  # environment override
```

If the root, `cols/`, or `board.txt` do not exist, toolb creates them. Existing board data is read directly from disk. Press `r` to reload files changed by another process.

## Keyboard controls

- `Left`/`Right` or `h`/`l`: select a column
- `/` or `g`: jump to a column by its display name
- `Up`/`Down` or `k`/`j`: select a card
- `Home`/`End`: first/last card
- `PageUp`/`PageDown` or `[`/`]`: scroll full card details
- `c` or `n`: create a card in the selected column
- `C` or `N`: create a column
- `e`: edit selected card title
- `a`: append one line to selected card body
- `m`: move selected card; arrows select the target and Enter confirms
- `d` or `Delete`: delete selected card
- `r`: reload the board from disk
- `?` or `F1`: show/hide expanded keyboard help
- `Esc`: cancel the current operation
- `q` or `Ctrl-C`: quit

All successful changes are written immediately to the real board files.

## On-disk format

```text
board.txt
cols/
  todo/
    order.txt
    item-1.md
```

`board.txt` contains lines such as:

```text
col todo "TO DO"
```

`order.txt` contains one card id per line. A card file starts with `# <title>` and all remaining lines are its body.
