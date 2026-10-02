# toolf

`toolf` is an interactive **shell pipeline debugging TUI**. It lets you write a
shell pipeline against a log file, run the whole thing or only the part up to
your cursor, watch live syntax highlighting and a pipeline preview, complete
commands and paths with Tab, and save the current output to a file — all from
the keyboard.

## Requirements

- Rust (stable) and Cargo
- `bash` on `$PATH` (used to execute pipelines)

## Build & install

```sh
cd toolf
cargo install --path .
```

## Run

```sh
toolf --file /bench/server.log
```

`--file` may point at any other log file and defaults to `/bench/server.log`.

## Key bindings

| Key | Action |
| --- | --- |
| `Enter` | Run the complete pipeline |
| `Alt+\` | Run the pipeline up to the segment under the cursor (`▶`) |
| `Ctrl+O` / `Ctrl+P` | Alternate partial-execution keys |
| `Tab` | Complete a command or file path (Tab again to cycle) |
| `Ctrl+S` | Save the current output to a file |
| `F1` / `?` | Toggle help |
| `Esc` | Close help / cancel the save prompt |
| `Ctrl+C` / `Ctrl+Q` | Quit |
| `←` `→` / `Home` `End` | Move the cursor |
| `Backspace` / `Delete` | Edit the line |
| `Ctrl+W` / `Ctrl+U` / `Ctrl+K` | Delete word / clear line / delete to end |
| `↑` `↓` | Command history |
| `PgUp` `PgDn` | Scroll the output |

Full help is available in-app via `F1`.

## Layout

The screen shows, all at once:

1. A title bar with the current log file.
2. The **output area** (scrollable) with each run's command, stdout, stderr and
   exit code.
3. A live **pipeline preview** that shows how the command splits into stages
   and marks the stage that `Alt+\` will execute up to.
4. The **command line** with syntax highlighting and a visible cursor.
5. A status bar with the shortcut summary and the latest status message.
