# toolf — Shell pipeline debugging TUI

`toolf` is a terminal UI for building, debugging and running shell pipelines against a log
file. It is aimed at the situation where you are iterating on something like

```
cat /bench/server.log | grep ERROR | awk '{print $3}' | sort | uniq -c | sort -rn
```

and you want to see what each stage actually produces without re-typing the command, plus a
one-key way to write the result to a file.

Every pipeline is executed by a real shell (`bash -c`), against the real filesystem. Nothing
in the interface is simulated: the output you see is the process output, and a saved file is
byte-identical to redirecting the same pipeline in a shell.

## Install

```sh
cd toolf && cargo install --path .
```

This installs a single binary, `toolf`.

## Run

```sh
toolf --file /bench/server.log     # the default; --file may point anywhere
toolf                              # same as above
toolf -f app.log -c 'cat app.log | grep WARN'   # pre-fill the command line
```

| Flag | Meaning |
| --- | --- |
| `-f`, `--file <PATH>` | Log file the session targets. Default `/bench/server.log`. |
| `-c`, `--command <PIPELINE>` | Start with this pipeline in the editor. |
| `-h`, `--help` / `-V`, `--version` | Usage and version. |

## Layout

Everything is on one screen — there are no modal dialogs that hide the output, and no tabs
to switch between. Long content scrolls in place.

```
╭ session ───────────────────────────────────────────────────────────────╮
│ toolf   ● log file: /bench/server.log   size: 2.3 KiB                  │
╰────────────────────────────────────────────────────────────────────────╯
╭ pipeline [stage 2/4] ──────────────────────────────────────────────────╮
│$ cat /bench/server.log | grep ERROR | awk '{print $2}' | sort -u       │
╰ Enter=run  Alt+\=run to cursor  Tab=complete ──────────────────────────╯
╭ output [partial 2/4] exit 0 5 line(s) ───────────╮╭ stages ────────────╮
│  1 │ 2026-08-13T10:07:00 ERROR request id=7      ││  1. cat            │
│  2 │ 2026-08-13T10:14:00 ERROR request id=14     ││▸ 2. grep           │
│  3 │ 2026-08-13T10:21:00 ERROR request id=21     ││  3. awk            │
│                                                  ││  4. sort           │
│                                                  │╰────────────────────╯
│                                                  │╭ keys (F1) ─────────╮
│                                                  ││RUN                 │
│                                                  ││Enter   execute the │
╰ $ cat /bench/server.log | grep ERROR ────────────╯╰ Alt+↑↓ scroll ─────╯
 ✓ stages 1..2/4: exit 0 · 5 line(s) · 22 ms
 Enter run   Alt+\ run→cursor   Alt+←→ stage   Tab complete   F10 quit
```

- **pipeline** — the command being edited, with syntax highlighting and a real cursor. The
  stage the cursor is in is shown at full brightness; the others are dimmed.
- **output** — stdout of the last run, with line numbers. The title reports whether the run
  was `full` or `partial n/total`, the exit code, and the line/byte count. stderr is appended
  in the same pane, so failures are never hidden.
- **stages** — one row per pipeline stage, with the cursor's stage marked `▸`.
- **keys (F1)** — the scrollable key reference; the bottom bar always shows the core keys.

## Keys

Everything is keyboard-only. The bottom bar lists the common keys and `F1` opens the full
reference inside the app, so nothing here needs to be memorised.

### Run

| Key | Action |
| --- | --- |
| `Enter` | Execute the full pipeline (or accept the highlighted completion) |
| `Alt+\` | Execute only the stages up to the cursor (`F5` also works) |
| `Ctrl+C` | Cancel the running pipeline |

### Edit the pipeline

| Key | Action |
| --- | --- |
| `←` `→` | Move the cursor |
| `Ctrl+←` `Ctrl+→` | Move by word |
| `Alt+←` `Alt+→` | Jump to the previous / next `|` boundary |
| `Home` / `End` | Start / end of line |
| `Backspace` / `Delete` | Delete before / at the cursor |
| `Ctrl+W` | Delete the previous word |
| `Ctrl+K` / `Ctrl+U` | Delete to end / start of line |
| `Ctrl+L` | Clear the line |
| `Tab` | Complete a command name or path |
| `↑` `↓` | Browse command history |

### Output

| Key | Action |
| --- | --- |
| `PgUp` / `PgDn` | Scroll by a page |
| `Ctrl+↑` `Ctrl+↓` | Scroll by a line |
| `Ctrl+Home` / `Ctrl+End` | First / last line |
| `Ctrl+F` | Search the output |
| `F3` / `Shift+F3` | Next / previous match |

### Other

| Key | Action |
| --- | --- |
| `Ctrl+S` | Save the current output to a path |
| `F1` | Show / hide the key reference |
| `Alt+↑` `Alt+↓` | Scroll the key reference |
| `Esc` | Dismiss the completion list or cancel a prompt |
| `F10` / `Ctrl+Q` | Quit |

## Features

### Partial execution

Put the cursor in a stage (`Alt+←` / `Alt+→` snap to the `|` boundaries) and press `Alt+\`.
`toolf` runs only the prefix of the pipeline up to and including that stage, so you can see
what a stage receives before the rest of the pipeline reshapes it. The title reports
`partial 2/4`, and the executed prefix is shown along the bottom of the output pane. The
command line — including the cursor — stays visible on the same screen as the output.

A cursor sitting exactly on a `|` belongs to the stage before it, and a trailing `|` is
dropped rather than left dangling.

### Syntax highlighting and validation

Commands, flags, quoted strings, `$VAR` / `$(...)` expansions, paths, numbers, pipes and
redirections each get their own colour. The scanner understands quoting, so a `|` inside
`'a|b'` or `-F"|"` is *not* treated as a pipeline separator, and neither is `\|` or `||`.

Unterminated quotes, unclosed `(`, and empty stages (`a | | b`) are flagged live in the
pipeline title, and running is refused until they are fixed rather than handing a broken
command to the shell.

### Tab completion

`Tab` completes the first word of a stage against the executables in `$PATH` (plus shell
builtins), and any other word against the real filesystem. Directories get a trailing `/`
so repeated `Tab` presses descend. With several candidates, the shared prefix is inserted
and the list appears in a side panel — `Tab` / `↑` `↓` select, `Enter` accepts, `Esc`
dismisses. Candidates containing spaces or shell metacharacters are quoted automatically.

### Saving output

`Ctrl+S` opens a save prompt pre-filled with a sensible destination next to the log file.
The path is editable and `Tab`-completable, missing parent directories are created, and the
bytes written are exactly what the pipeline produced. The output stays visible while the
prompt is open.

### Execution safety

Runs happen on a worker thread, so the UI stays responsive and `Ctrl+C` can cancel. stdout
and stderr are drained concurrently (a pipeline that fills one pipe cannot deadlock),
captured output is capped at 8 MiB, and a run is killed after 30 s. Both limits are reported
in the UI rather than failing silently. Control characters in output are neutralised for
display only.

## Tests

```sh
cargo test        # 74 unit + rendering tests
cargo clippy      # clean
```

The rendering tests use ratatui's `TestBackend` to assert on the real rendered screen,
including the same-screen requirement (command line and output visible together) and that
the key bar keeps the help/quit hints at every supported terminal width.

`verify/` holds end-to-end checks that drive the installed binary through a pty with `pyte`
as a real terminal emulator (`pip install pyte`):

| Script | Covers |
| --- | --- |
| `e2e_check.py` | The full workflow: run, partial run, save, complete, search, help, quit |
| `robust_check.py` | Awkward input: `awk`/`sed` with metacharacters, quoted `|`, 5000-line output, paths with spaces, missing commands |
| `highlight_check.py` | That highlighting emits distinct colours per token class |
| `screenshot.py` | Prints a rendered scene for visual review |
| `Dockerfile.verify`, `in_container.sh`, `container_check.py` | The whole workflow inside Debian 12 slim, as root, against `/bench/server.log` |

```sh
docker build -f verify/Dockerfile.verify -t toolf-verify .
docker run --rm -v "$PWD:/host:ro" toolf-verify bash /host/verify/in_container.sh
```

Note: these scripts assert on colour, so run them without `NO_COLOR` set — crossterm honours
that variable and will suppress all colour output.

## Layout of the source

| File | Contents |
| --- | --- |
| `src/main.rs` | CLI, terminal setup, event loop, key dispatch |
| `src/app.rs` | Application state: editor, history, prompts, scrolling, saving |
| `src/exec.rs` | Real pipeline execution on a worker thread |
| `src/pipeline.rs` | Quote-aware stage splitting, boundaries, prefix building |
| `src/highlight.rs` | Token classification for syntax highlighting |
| `src/complete.rs` | `$PATH` and filesystem completion |
| `src/ui.rs` | Rendering |
