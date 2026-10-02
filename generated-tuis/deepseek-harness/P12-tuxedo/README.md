# tooll

`tooll` is a keyboard-driven terminal UI (TUI) for managing
[todo.txt](https://github.com/todotxt/todo.txt) task lists.

It lets you view, add, edit, prioritize, tag, filter and complete tasks —
and it writes every change straight back to the underlying `todo.txt` file,
so the on-screen state always matches what is on disk.

## Building and installing

```sh
cd tooll
cargo install --path .
```

After installation the executable is available as `tooll`.

## Usage

```sh
tooll                 # uses /bench/data/todo.txt
tooll /path/todo.txt  # positional file argument
tooll --file FILE     # -f FILE also works
```

The task file can also be overridden with the `TOOLL_FILE` environment
variable (CLI arguments take precedence).

Run `tooll --help` for the command-line reference. Inside the TUI, press
`?` for the full key binding reference.

## todo.txt format

One task per line:

- **Priority** — uppercase letter in parentheses at the start, e.g. `(A)`.
- **Projects** — `+project` tags.
- **Contexts** — `@context` tags.
- **Due date** — `due:YYYY-MM-DD`.
- **Completion** — a leading `x ` marks a completed task.

Examples:

```text
(B) Buy groceries +errands @home due:2026-03-15
x (A) Pay rent +finance @computer due:2026-01-01
```

## Key bindings

| Key | Action |
| --- | --- |
| `↑`/`↓` or `j`/`k` | Move selection |
| `Home`/`End`, `g`/`G` | Jump to first / last task |
| `PgUp`/`PgDn` | Page through the list |
| `a` | Add a task |
| `e` / `Enter` | Edit the selected task |
| `Space` | Toggle complete / reopen |
| `x` | Mark complete |
| `u` | Mark not complete |
| `p` / `P` | Raise / lower priority |
| `d` | Delete (with confirmation) |
| `/` | Live text search |
| `+` | Filter by project tag |
| `@` | Filter by context tag |
| `r` / `Esc` | Clear filters |
| `?` / `h` | Help |
| `q` / `Ctrl+C` | Quit |

## License

MIT
