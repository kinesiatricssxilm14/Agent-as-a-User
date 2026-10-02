# toole

An interactive regular expression tester for the terminal. Load a text file,
type a pattern, and see every match highlighted in place — with 0-indexed
character offsets and a live preview of what a replacement would produce.

`toole` exists because iterating on a regex with `grep` and `sed` means running
the command again for every small change, with no highlighting, no offsets, and
no way to check a replacement before it rewrites the file.

## Install

```sh
cd toole && cargo install --path .
```

The installed command is `toole`.

## Run

```sh
toole                             # reads the default /bench/data/input.txt
toole -f path/to/file.txt         # any other file
toole -f data/input.txt -e '(?i)error' -r 'WARN'   # pre-fill the fields
```

Everything is editable once the interface is up; the flags only set the starting
state. Press **F1** for the full key map.

## The interface

```
 toole  /bench/data/input.txt  74 lines · 3116 chars · 3199 B  flags i m s x F  engine:regex  focus:pattern
╭ Pattern (regex) 3 matches ───────────────────────────────────────────────────────╮
│1[3-9]\d{9}                                                                       │
╰──────────────────────────────────────────────────────────────────────────────────╯
╭ Replace (template) ──────────────────────────────────────────────────────────────╮
│type a replacement — $1 for a capture group, $0 for the whole match               │
╰──────────────────────────────────────────────────────────────────────────────────╯
╭ Source · lines 1-28 of 74 · match 1 selected ──────╮╭ Matches · 3 · offsets are 0-indexed ─╮
│ 6 Zhang Wei      13812345678    zhang.wei@exam…    ││   #   start     end  line:col  text  │
│ 7 Li Na          +86 15900001111  li.na@exampl…    ││   1     173     184  5:15   1381234…│
│ 8 Wang Fang      186-2233-4455  wang.fang@corp…    ││   2     229     240  6:19   1590000…│
╰────────────────────────────────────────────────────╯│   3     382     393  9:15   1501234…│
╭ Replacement preview · full file · 3 applied ───────╮╰──────────────────────────────────────╯
│ 6 Zhang Wei      1XXXXXXXXXX    zhang.wei@exam…    │╭ Capture groups · Ctrl+G for presets ─╮
╰────────────────────────────────────────────────────╯╰──────────────────────────────────────╯
 match 1/3  start 173  end 184  length 11  line:col 5:15  bytes 173..184  text 13812345678
 pattern sent to engine: 1[3-9]\d{9}
 Tab next field · Enter results · Ctrl+N/P match · F2 (?i) · F7 preview · Ctrl+S write · F1 help · Ctrl+Q quit
```

The source text, the match list with its offsets, the replacement preview and
the capture groups are all on screen at the same time. There are no tabs to
switch between, and the help pane takes a column of its own rather than covering
the results.

## What it does

**Live matching.** Every keystroke in the pattern field re-scans the whole file.
The pane title reports the match count, or the syntax error if the pattern is
not yet valid — a half-typed pattern is an ordinary state, not an error to
recover from.

**Background highlighting.** Matches get a background colour (amber; bright
amber for the selected one), so they stay distinguishable from body text
regardless of the terminal's foreground theme. Replacement output is green, and
zero-width matches are drawn as a purple caret.

**0-indexed offsets.** Each match is listed with its start and end character
offsets, and its `line:col`. Offsets count *characters*, not bytes: in
`English-only textabc`, `abc` starts at offset 2. `end` is exclusive, so a match of length
*n* spans `start..end` with `end == start + n`. The detail strip above the key
bar spells out the selected match, byte range included.

**Case insensitivity.** `F2` toggles `(?i)`, and typing `(?i)` inline works
identically — the status line always shows the exact pattern handed to the
engine. A case-insensitive match can sit mid-token: `(?i)err` matches the `ERR`
inside `xxERRORxx`.

**Replacement preview.** The preview shows the **complete file**, including
every line no match touched, so you can confirm the change in context before
`Ctrl+S` writes it. `$1`/`${1}` insert numbered groups, `$name`/`${name}` named
ones, `$0` the whole match, `$$` a literal `$`, and `\n \t \r \\ \$` the usual
escapes. Writes go through a temporary file and a rename, so an interrupted
write cannot truncate your file, and overwriting the input file asks first.

## Keys

`F1` lists all of them inside the program. The essentials:

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | move focus: Pattern → Replace → Matches → Source |
| `↑ ↓` `j k` | previous / next match, or scroll, depending on focus |
| `Ctrl+N` / `Ctrl+P` | next / previous match, from any focus |
| `Enter` | leave a field for the results; centre a match in the text |
| `F2` … `F6` | `(?i)` `(?m)` `(?s)` `(?x)` and literal mode |
| `F7` / `F8` | show the preview / replace only the first match |
| `F9` / `F10` | soft wrap / line numbers |
| `[` `]` / `F11` | cycle the built-in pattern presets |
| `Ctrl+S` | write the replaced content to a file |
| `Ctrl+O` / `Ctrl+R` | open another file / reload this one |
| `Ctrl+G` | right panel: capture groups ↔ presets |
| `F1` / `?` | help |
| `Esc` | step back: close help, leave a field, then quit |
| `Ctrl+Q` / `Ctrl+C` | quit |

## Regex support

Patterns run on the [`regex`](https://docs.rs/regex) crate, which guarantees
linear-time matching. Look-around and back-references are not in that engine's
grammar, so when a pattern needs them `toole` recompiles it with
[`fancy-regex`](https://docs.rs/fancy-regex) automatically and says so in the
status line. `Ctrl+Y` forces the backtracking engine.

The presets (`[` / `]`) cover common cases — mainland China mobile and landline
numbers, ID cards, emails, IPv4, URLs, dates, times, hex colours, doubled words,
CJK runs. They only fill the pattern field; the text stays fully editable, and
matching always runs whatever is in the field through the real engine.

## Non-interactive use

With `--print`, or whenever stdout is not a terminal, `toole` writes a plain-text
report instead of starting the interface — the same matching and replacement
code, so it is usable in scripts and pipelines:

```sh
$ toole -f data/input.txt -e '1[3-9]\d{9}' --print
file: data/input.txt
lines: 74  chars: 3116  bytes: 3199
pattern: 1[3-9]\d{9}
effective: 1[3-9]\d{9}
engine: regex

matches: 3
    #    start      end  line:col  text
    1      173      184      5:15  13812345678
    2      229      240      6:19  15900001111
    3      382      393      9:15  15012345678
```

An invalid pattern or an unreadable file exits non-zero with a message on
stderr.

### Options

| Flag | Meaning |
| --- | --- |
| `-f, --file PATH` | input file (default `/bench/data/input.txt`) |
| `-e, --pattern REGEX` | start with this pattern |
| `-r, --replace TEMPLATE` | start with this replacement template |
| `-i, --ignore-case` | `(?i)` |
| `-M, --multi-line` | `(?m)` |
| `-s, --dot-all` | `(?s)` |
| `-x, --extended` | `(?x)` |
| `-F, --literal` | treat the pattern as plain text |
| `--fancy` | force the backtracking engine |
| `--first-only` | replace only the first match |
| `-p, --print` | print a report instead of starting the TUI |
| `--list-presets` | list the built-in patterns |
| `--tab-width N` | columns per tab stop (1–16, default 4) |

## Development

```sh
cargo test              # 166 tests: unit + end-to-end
cargo clippy --all-targets
cargo fmt --check
```

Layout of the source:

| File | Responsibility |
| --- | --- |
| `src/main.rs` | terminal setup, event loop, `--print` report |
| `src/app.rs` | all application state and key handling — no terminal I/O |
| `src/engine.rs` | compilation, matching, replacement template expansion |
| `src/doc.rs` | the loaded file; byte ↔ character offset mapping |
| `src/textlayout.rs` | styled rows: highlights, wrapping, tabs, wide chars |
| `src/ui.rs` | drawing the panes |
| `src/input.rs` | the single-line text editor |
| `src/help.rs` | the in-program key reference |
| `src/theme.rs` | colours |

`app.rs` deliberately contains no terminal I/O, so the entire interaction model
is tested by feeding it key events and inspecting the resulting state.

## Docker

```sh
docker build -t toole .
docker run --rm -it toole                    # starts on /bench/data/input.txt
```

## License

MIT.
