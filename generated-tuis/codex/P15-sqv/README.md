# toolo

`toolo` is a keyboard-first SQLite browser built with Rust, ratatui, and rusqlite.
It discovers every user table, displays arbitrary schemas and rows, executes SQL,
sorts, filters (contains/exact/regex), and edits records using SQLite rowids or
primary keys.

## Install

```sh
cd toolo
cargo install --path .
toolo /bench/data/bench.db
```

If no database path is supplied, `/bench/data/bench.db` is used.

## Keyboard overview

The running application always shows its most important keys in the footer.
Press `?` for complete help.

- Arrow keys: navigate rows/tables and horizontally scroll all columns
- `Enter`: open selected table / run SQL / confirm input
- `Tab`: move focus among table, data, and details panes
- `s`: choose a sort column; `a`/`d` sorts ascending/descending
- `/`: choose a filter column, mode, and expression
- `e`: edit the selected row
- `q`: enter arbitrary SQL (SELECT results appear in the data pane)
- `x`: clear active sort and filter
- `r`: reload database contents
- `Esc`: cancel a workflow
- `Ctrl-C`: quit

The details pane remains visible with the selected table schema and complete
selected record. When sorting, this record is the current extreme row. When an
edit succeeds, the updated complete record remains selected and visible.
