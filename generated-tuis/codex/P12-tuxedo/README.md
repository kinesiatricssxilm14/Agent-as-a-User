# tooll

`tooll` is a keyboard-first terminal interface for real `todo.txt` files. It
uses Rust, ratatui, and crossterm.

## Install

```sh
cd tooll
cargo install --path .
```

Then run:

```sh
tooll
tooll /path/to/todo.txt
tooll --file /path/to/todo.txt
TOOLL_FILE=/path/to/todo.txt tooll
```

The default file is `/bench/data/todo.txt`. Missing files and parent
directories are created automatically.

## Keys

- `↑`/`↓` or `j`/`k`: navigate
- `a`: add a task
- `Enter` or `x`: complete/reopen
- `p`: change/remove priority
- `t`: replace projects and contexts
- `/`: live text search
- `P`: live project filter
- `C`: live context filter
- `c`: clear filters
- `d`: delete with confirmation
- `r`: reload from disk
- `?`: full in-app help
- `q`: quit

Changes are atomically persisted immediately to the active todo file.
