# toolk

A keyboard-first CSV data analysis TUI built in Rust with Ratatui, Crossterm, and Polars.

## Install

```sh
cd toolk
cargo install --path .
toolk /bench/data/employees.csv
```

With no argument, `toolk` opens `/bench/data/employees.csv`.

## Core workflow

- `1`, `2`, `3`: switch among Fuzzy, SQL-Like, and SQL modes.
- `/`: edit a query. Fuzzy results update as you type; press Enter for SQL modes.
- Arrow keys or `hjkl`: browse rows and columns. The lower pane shows every field in the selected row; `[`/`]` scroll it.
- `s`: sort by one or more comma-separated columns; `d` toggles direction.
- `a`: analyze one or two columns. The same view shows descriptive statistics, distribution, optional Pearson correlation, and the selected record.
- `e`: export the current real result DataFrame to CSV.
- `o`: load another CSV; `r`: reset; `?`: full in-app keyboard help.

SQL-Like accepts `select where <condition>`. SQL uses the table name `df`.

Examples:

```sql
select where department = 'Engineering' and salary > 10000
select * from df where country = 'US' and score > 85
```

Floating-point cells and analysis metrics are rendered with fixed two-decimal formatting, including trailing zeroes.
