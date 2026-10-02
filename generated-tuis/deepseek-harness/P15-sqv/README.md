# toolo

A keyboard-driven **SQLite database browser** for the terminal, written in
Rust with [ratatui](https://github.com/ratatui/ratatui).

Browse tables, inspect schemas, run SQL, sort and filter data, and edit rows
— all without memorizing command-line flags.

## Install

```sh
cd toolo
cargo install --path .
```

This produces a `toolo` executable on your `PATH`.

## Run

```sh
toolo /path/to/database.db
```

If no path is given, `toolo` defaults to `/bench/data/bench.db`.

## Features

- **Open database** — connect to any SQLite file (pass it as an argument, or
  press `o` at any time). All tables are discovered automatically.
- **Table browsing** — a live-filterable table list on the left.
- **Data view** — every column of every row, scrollable both directions.
- **Schema view** — the `CREATE TABLE` statement plus per-column metadata.
- **SQL query** — run arbitrary SQL (results or affected-row counts).
- **Sort** — ascending/descending by any column; the extreme value is placed
  at the top of the view.
- **Filter** — per-column `contains`, `regex`, and `exact` matching with a
  live match count.
- **Edit** — modify a cell and write it back to the database.

## Keys

### Global

| Key | Action |
| --- | --- |
| `q` / `Ctrl+C` | quit |
| `o` / `Ctrl+O` | open a database file |
| `F1` or `?` | toggle help |
| `Tab` / `Shift+Tab` | move focus |
| `1` / `2` / `3` | Data / Schema / Query view |
| `Esc` | back to the table list |

### Table list

| Key | Action |
| --- | --- |
| `j`/`k` or `↑`/`↓` | move |
| `Enter` | load table |
| `/` | live-filter the table list |

### Data view

| Key | Action |
| --- | --- |
| `j`/`k` or `↑`/`↓` | move row |
| `h`/`l` or `←`/`→` | move column |
| `PgUp` / `PgDn` | page |
| `g` / `G` | first / last row |
| `e` or `Enter` | edit current cell |
| `f` | filter current column (Tab cycles contains → regex → exact) |
| `s` / `S` | sort current column ascending / descending |
| `u` | clear sort |
| `x` | clear filters |
| `r` | reload data |

### Query view

Type SQL and press `Enter` to run. `↑`/`↓` recall previous queries, and
`Tab` moves focus between the input and the result grid.

## Filter matching

- **contains** — substring match
- **exact** — the whole displayed value must equal the input
- **regex** — Rust `regex` syntax (e.g. `^foo.*bar$`); matching is
  case-sensitive unless you write `(?i)`.
