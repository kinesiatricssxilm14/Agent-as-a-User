# tooll

An interactive **todo.txt** task manager for the terminal, built with
[ratatui](https://ratatui.rs) and crossterm.

`tooll` edits a plain `todo.txt` file in place. Every change is written to disk
the moment you make it — there is no separate save step, and the list you see is
always exactly what the file contains.

```
╭──────────────────────────────────────────────────────────────────────────────╮
│tooll  /bench/data/todo.txt [default]                                         │
│showing 7/7   open 6   done 1   overdue 2   filter: no filter   sort: file …  │
╰──────────────────────────────────────────────────────────────────────────────╯
┏ Tasks (1/7) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓╭ Projects (2) ─╮╭ Contexts (3) ─╮
┃  1 [ ] (B) Buy groceries +errands @home due:2026… ┃│● all (7)      ││● all (7)      │
┃  2 [ ] (A) Pay rent +finance @computer due:2026-… ┃│  +admin (1)   ││  @computer (4)│
┃  3 [ ]     Call the plumber +house @phone         ┃│  +errands (1) ││  @home (2)    │
┃  4 [x] (C) Renew passport +admin @computer        ┃│  +finance (1) ││  @phone (1)   │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛╰───────────────╯╰───────────────╯
╭ Selected task ───────────────────────────────────────────────────────────────╮
│Line: 1 of 7                    Status: open   Priority: (B)                  │
│Text: Buy groceries             Due: 2026-03-15  (due in 12 days)             │
│Projects: +errands              Contexts: @home                               │
│Raw: (B) Buy groceries +errands @home due:2026-03-15                          │
╰──────────────────────────────────────────────────────────────────────────────╯
 ✓  Priority (B) → (A) - saved to /bench/data/todo.txt
 ↑↓ move · Tab panel · a add · e edit · Space done · p priority · c context …
```

## Install

```sh
cd tooll && cargo install --path .
```

That puts a `tooll` binary on your `PATH`. Run it with no arguments to open the
default task file, `/bench/data/todo.txt`.

## Running

```sh
tooll                              # open /bench/data/todo.txt
tooll ~/todo.txt                   # open a specific file
tooll --file /path/to/todo.txt     # the same, explicitly
tooll --list                       # print the file and exit (no TUI)
tooll --help                       # full option list
```

The task file is resolved in this order, first match wins:

1. `--file PATH` / `-f PATH`, or a bare path argument
2. `$TOOLL_TODO_FILE`
3. `file = PATH` in the config file
4. `/bench/data/todo.txt`

A missing file is not an error: `tooll` opens an empty list and creates the file
(and any parent directories) on your first change.

### Config file

Optional, read from `$TOOLL_CONFIG`, else `$XDG_CONFIG_HOME/tooll/config.toml`,
else `~/.config/tooll/config.toml`. Use `--no-config` to skip it.

```toml
file = /bench/data/todo.txt
hide_done = false      # start with completed tasks hidden
sort_on_load = false   # sort on load instead of showing file order
```

## Keys

Press <kbd>?</kbd> at any time for the full list — it is generated from the same
table the program uses to dispatch keys, so it cannot drift. The bottom bar
always shows the common ones.

Everything is keyboard-driven; the mouse is never required.

### Moving around

| Key | Action |
| --- | --- |
| <kbd>↑</kbd> <kbd>↓</kbd> or <kbd>k</kbd> <kbd>j</kbd> | previous / next item in the focused panel |
| <kbd>PgUp</kbd> <kbd>PgDn</kbd> | move a screen at a time |
| <kbd>Home</kbd> <kbd>End</kbd> or <kbd>g</kbd> <kbd>G</kbd> | first / last item |
| <kbd>Tab</kbd> <kbd>Shift-Tab</kbd> | cycle focus: tasks → projects → contexts |
| <kbd>1</kbd> <kbd>2</kbd> <kbd>3</kbd> | focus a panel directly |
| <kbd>></kbd> <kbd><</kbd> | scroll long task lines sideways |
| <kbd>Enter</kbd> | on a task: edit it; in a side panel: apply that filter |

### Changing tasks

| Key | Action |
| --- | --- |
| <kbd>a</kbd> | add a task (form with every field on one screen) |
| <kbd>e</kbd> / <kbd>Enter</kbd> | edit the selected task |
| <kbd>Space</kbd> / <kbd>x</kbd> | toggle complete — writes or removes the `x ` prefix |
| <kbd>p</kbd> | set priority by typing a letter (`A`–`Z`, or `-` to clear) |
| <kbd>+</kbd> / <kbd>-</kbd> | raise / lower priority one step |
| <kbd>c</kbd> | set the `@context` tags |
| <kbd>P</kbd> | set the `+project` tags |
| <kbd>d</kbd> | set the `due:YYYY-MM-DD` date |
| <kbd>D</kbd> / <kbd>Del</kbd> | delete the selected task (asks first) |
| <kbd>u</kbd> | undo the last change |

### Filtering and searching

| Key | Action |
| --- | --- |
| <kbd>/</kbd> | search as you type, across the whole task line |
| <kbd>f</kbd> | filter by a project name you type |
| <kbd>@</kbd> | filter by a context name you type |
| <kbd>t</kbd> / <kbd>T</kbd> | filter by the selected task's first project / context |
| <kbd>v</kbd> | cycle visibility: all → open only → done only |
| <kbd>s</kbd> | cycle sort: file order → priority → due date |
| <kbd>F</kbd> / <kbd>Esc</kbd> | clear every filter and the search term |

Filters combine: a project filter, a context filter and a search term all apply
at once, and the header names whatever is active.

### The file

| Key | Action |
| --- | --- |
| <kbd>Ctrl-K</kbd> / <kbd>Ctrl-J</kbd> | move the selected task up / down a line in the file |
| <kbd>S</kbd> | sort the file itself (open first, then priority, then due date) |
| <kbd>X</kbd> | archive completed tasks to `done.txt` (asks first) |
| <kbd>r</kbd> / <kbd>F5</kbd> | re-read the file from disk |
| <kbd>q</kbd> | quit (everything is already saved) |

## todo.txt format

`tooll` reads and writes the standard format:

```
x (A) 2026-01-02 2026-01-01 Pay rent +finance @computer due:2026-01-05
│  │       │          │        │        │         │        │
│  │       │          │        │        │         │        └ due date
│  │       │          │        │        │         └ context tag
│  │       │          │        │        └ project tag
│  │       │          │        └ description
│  │       │          └ creation date
│  │       └ completion date
│  └ priority, (A) is highest
└ completed marker
```

Notes on how `tooll` treats it:

- **Priority** is a single uppercase letter in parentheses. Raising past `(A)`
  and lowering past `(Z)` are both refused rather than silently wrapping.
- **Completing** a task adds `x ` plus today's date. Because a completion date is
  only meaningful next to a creation date, one is added if the task lacks it.
  Reopening a task with <kbd>Space</kbd> removes the completion date again.
- **Tags** are matched case-insensitively for filtering but stored exactly as
  typed. Whitespace inside a tag becomes `-` so the token stays a single word.
- **Unrecognised text is preserved.** Anything that is not valid metadata stays
  in the description, and a task that you load but never edit is written back
  byte-for-byte. `CRLF` line endings are preserved too.
- **Writes are atomic**: the file is written to a sibling temp file, flushed, and
  renamed into place, so an interrupted write cannot truncate your task list.

## Development

```sh
cargo test      # 159 tests
cargo clippy --all-targets
```

The crate is laid out so that everything except the terminal itself is testable
in-process:

| Module | Responsibility |
| --- | --- |
| `task.rs` | todo.txt parsing, mutation, serialisation |
| `store.rs` | loading and atomically writing the file |
| `date.rs` | the small calendar helpers the format needs |
| `config.rs` | argument, environment and config-file resolution |
| `input.rs` | the single-line editor shared by all prompts |
| `app.rs` | application state and every key binding |
| `keys.rs` | the one canonical key/documentation table |
| `ui.rs` | ratatui rendering, a pure function of `App` |

`tests/integration.rs` drives the real application through key events and asserts
against the bytes on disk. `tests/render.rs` asserts on the actual character grid
via ratatui's `TestBackend`, including that prompts and forms never hide the task
list.

## License

MIT
