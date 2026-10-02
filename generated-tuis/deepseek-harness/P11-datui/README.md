# toolk

A keyboard-driven CSV data analysis TUI built with **Rust**, **Ratatui**
(Crossterm backend) and **Polars**.

`toolk` loads a CSV file into a browseable table and offers three query modes
plus sorting, statistics, and CSV export — all from the terminal.

## Install

```sh
cd toolk
cargo install --path .
```

The installed command is `toolk`.

## Run

```sh
toolk /path/to/data.csv
```

If no path is given, it defaults to `/bench/data/employees.csv`.

## Query modes

Switch modes with `Tab` (or `m`) while in the table view.

| Mode     | Input form                                                        |
|----------|-------------------------------------------------------------------|
| Fuzzy    | any keyword — case-insensitive substring match across all columns |
| SQL-Like | `select where age > 40`                                           |
| SQL      | `select * from df where country = 'US' and score > 85`            |

SQL-Like supports `and` / `or` and the comparison operators `=`, `>`, `<`,
`>=`, `<=`, `!=`. Both SQL modes are executed by Polars' built-in SQL engine
against a table named `df`.

## Keys

Press `?` inside the app for the full help screen.

- `↑/↓` or `j/k` — move selected row
- `←/→` or `h/l` — scroll columns
- `PageUp` / `PageDown`, `Home` / `End` — page / jump
- `/` or `i` — focus the query input
- `Enter` — run query
- `Esc` — clear input / go back
- `Tab` or `m` — switch query mode
- `s` — sort (Enter cycles ↑ / ↓ / clear; `x` clears all)
- `a` — analysis (descriptive statistics + correlation + distribution)
- `e` — export current results to a CSV path
- `r` — reload the file
- `q` or `Ctrl+C` — quit

## Layout

The screen shows, simultaneously:

1. a header with file, mode, row/match/sort summary,
2. the query input line,
3. the results table,
4. a detail panel listing **every** column of the selected row,
5. a status line,
6. a two-line key hint footer.

The analysis view keeps descriptive statistics, the Pearson correlation matrix
and a distribution histogram on one scrollable screen.

## Numeric display

- Floats in the table and correlation coefficients use fixed **two decimal
  places** (e.g. `50.00`, `0.85`).
- Summary statistics use **three significant figures** (e.g. `456`, `45.6`,
  `0.878`).

## Tests

```sh
cargo test
```
