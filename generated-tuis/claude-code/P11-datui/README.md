# toolk

An interactive CSV data analysis TUI for the terminal. Load a CSV file, browse it
in a table, and query it three ways — fuzzy keyword search, a compact
`select where …` syntax, or full standard SQL.

Built with [Ratatui](https://ratatui.rs) (Crossterm backend) and
[Polars](https://pola.rs). Every operation reads or writes the real file on
disk; nothing in the interface is simulated.

## Install

```sh
cd toolk && cargo install --path .
```

This installs a `toolk` executable.

## Run

```sh
toolk                            # loads /bench/data/employees.csv
toolk /path/to/other.csv         # or any CSV you pass as the first argument
toolk --help                     # usage and key summary
```

Columns are read from the CSV header row; no column name or type is assumed.

## Query modes

Switch with `F2` / `F3` / `F4`, or cycle with `m` / `Shift-Tab`. Each mode keeps
its own query line, so switching never loses what you typed.

### Fuzzy (`F2`)

Type a keyword and the table filters as you type. The keyword is matched
case-insensitively against every column, and results are ranked best-match
first. Several space-separated words all have to match somewhere in the row.

```
Sales
Novak 45
```

Matching is subsequence-based, so `Egnr` still finds `Engineering`. Exact
matches rank above prefixes, which rank above substrings, which rank above
scattered matches.

### SQL-Like (`F3`)

Condition filtering in a `select where …` form. Press `Enter` to run.

```
select where age > 40
select where department = 'Engineering' and salary > 10000
select where salary between 8000 and 13000
select where department in ('Sales', 'Support')
select where name like '%Novak' and score is not null
select where not country = 'US' or score > 90
select id, name, salary where salary > 15000
```

- Combine with `and`, `or`, `not`, and parentheses. As in SQL, `and` binds
  tighter than `or`.
- Comparisons: `=`, `==`, `!=`, `<>`, `>`, `<`, `>=`, `<=`.
- Also supported: `between … and …`, `in (…)`, `like` (with `%` and `_`),
  `contains`, `is null`, `is not null`.
- The leading `select` and the `where` are both optional, so `age > 40` works.
- Column names are matched case-insensitively; quote them with `` ` `` or `[]`
  if they contain spaces. String equality is case-insensitive, and quotes around
  a plain value are optional.
- Numeric columns compare numerically, so `salary > 9000` orders by value rather
  than by text.

### SQL (`F4`)

Full standard SQL, executed by Polars. The table is named `df` (also available
as `data`, `csv`, or the file's stem). Press `Enter` to run.

```
select * from df where country = 'US' and score > 85
select * from df where age = 35 and salary > 5000 and salary < 15000
select department, count(*) as n from df group by department order by n desc
select name, salary from df order by salary desc limit 5
```

Aggregates and projections are supported; the results table adopts whatever
columns the query returns.

After running a query, the results table shows the matching rows and the match
count in its title.

## Analysis

Press `a` to open the analysis panel for the focused column (moved with
`←` / `→`). It shows, all on the same screen:

- **Descriptive statistics** — count, missing, unique, mean, standard deviation,
  min, p25, median, p75, max, sum, range, variance. Text columns get length and
  frequency statistics instead.
- **Distribution** — value counts with bars and percentages. Continuous numeric
  columns are bucketed into bins automatically.
- **Correlation** — Pearson *r* for every pair of numeric columns, ranked
  strongest first, plus the full matrix.

Analysis always reflects the *current result*, so filtering first and analysing
second gives statistics for the filtered subset.

### Number formats

Analysis values use fixed-width formats so trailing zeros are never dropped.
Press `F` to switch:

- **2dp** (default) — always two decimals: `50.00`, `75.25`, `1288.42`.
  Correlations show as `1.00`, `0.85`, `-0.12`.
- **3sig** — three significant figures: `456`, `45.6`, `12.3`, and values below
  one keep all three figures: `0.878`.

Press `x` to change how numbers appear in the table itself, cycling between
`auto` (as stored in the file), two decimals, and three significant figures.

## Sorting, exporting, and files

- `s` sorts by the focused column; press again to reverse it.
- `S` adds the focused column as an additional sort key, so you can sort by
  several columns at once. The header shows each key's direction and priority
  (`department▲2 salary▼1`).
- `R` clears all sorting. A sort survives running a new query.
- `e` exports the current result to a CSV path you type, with a header row.
- `o` opens a different CSV file; `r` reloads the current one from disk.

## Keys

Press `?` or `F1` inside the app for the full list, grouped by category. The
bottom bar always shows the keys that apply to what you are doing.

| Key | Action |
| --- | --- |
| `Tab` | move between the query line and the table |
| `F2` / `F3` / `F4` | Fuzzy / SQL-Like / SQL mode |
| `m`, `Shift-Tab` | cycle modes |
| `/` or `f` | jump to the query line |
| `Enter` | run the query |
| `↑` `↓` in the query line | recall previous queries |
| `Ctrl-l` or `c` | clear the query, showing all rows |
| `↑` `↓` / `j` `k` | previous / next row |
| `←` `→` / `h` `l` | previous / next column |
| `PgUp` `PgDn`, `Space` | page through rows |
| `Home` `End`, `g` `G` | first / last row |
| `J` `K` | scroll the side panel |
| `s` / `S` / `R` | sort / add sort key / clear sorting |
| `a` | analysis panel |
| `e` / `o` / `r` | export / open / reload |
| `x` / `F` | table / analysis number format |
| `?` or `F1` | help |
| `q`, `Esc`, `Ctrl-c` | quit |

Everything is reachable from the keyboard; a mouse is never required.

## Layout

The table, the selected row's complete field list, and the analysis metrics are
laid out side by side in one view. Because the table can be scrolled
horizontally, the "Selected row" panel always lists *every* column of the
current selection, so no field is ever off-screen for the row you are looking
at. Panels scroll when their content is taller than the terminal; nothing is
hidden behind a tab or a modal. On terminals narrower than 100 columns the
panels stack vertically instead.

## Development

```sh
cargo test     # unit tests for formatting, parsing, querying, and key handling
cargo build    # debug build
```

The code is organised as:

| File | Responsibility |
| --- | --- |
| `src/main.rs` | CLI arguments, terminal setup and teardown, event loop |
| `src/app.rs` | application state, key handling, cached analysis |
| `src/ui.rs` | all rendering |
| `src/data.rs` | CSV I/O, querying, sorting, statistics, export (Polars) |
| `src/sqllike.rs` | recursive-descent parser for the SQL-Like mode |
| `src/editor.rs` | single-line text editing with history |
| `src/fmtnum.rs` | fixed-width number formatting rules |
