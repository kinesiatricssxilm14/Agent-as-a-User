# toole

**toole** is an interactive regular-expression testing TUI (text user
interface) written in Rust. It gives you real-time match highlighting, 0-indexed
character offsets for every match, case-insensitive matching, a live replacement
preview of the *entire* file, and one-key presets for common patterns (including
mainland China phone numbers).

Built with [`ratatui`](https://crates.io/crates/ratatui) and
[`crossterm`](https://crates.io/crates/crossterm), using the
[`regex`](https://crates.io/crates/regex) engine for matching.

## Features

- **Real-time matching** — the match list updates on every keystroke.
- **Background highlighting** — matches are shown with a distinct background
  color (yellow) that is clearly distinguishable from body text; replaced
  content uses a green background.
- **Offset display** — every match is annotated with its `[start..end)`
  character offsets (0-indexed, Unicode-aware) and the 1-indexed source line.
- **Case-insensitive matching** — toggle with `i`; implemented with the engine's
  native `(?i)` semantics, so substrings matched in the middle of tokens work
  exactly as the regex engine defines them.
- **Replacement preview** — enter a replacement string and the preview pane
  shows the complete file content with all matches replaced (unmodified lines
  are preserved and shown in full). Capture-group references such as `$1`,
  `${name}` and `$0` are supported.
- **Presets** — `F2` opens a preset menu with mainland China mobile
  (`1[3-9]\d{9}`, with optional `+86`/`86` prefix) and landline formats, plus
  email, IPv4, date and URL patterns.
- **Keyboard-only** — every action is bound to a key; a persistent help bar and
  a full help overlay (`F1`) make all shortcuts discoverable.
- **Same-screen layout** — the match list (with offsets), the preview pane, and
  the input area are all visible at once; large lists scroll in place.

## Installation

Requires a Rust toolchain (Cargo).

```sh
cd toole
cargo install --path .
```

This produces the `toole` binary. (If `~/.cargo/bin` is not on your `PATH`, the
binary is at `~/.cargo/bin/toole`.)

## Usage

```sh
toole -f /path/to/file.txt
```

The input file defaults to `/bench/data/input.txt` when `-f` is omitted.

```
USAGE:
    toole [-f <file>]

OPTIONS:
    -f, --file <file>    Input text file to load (default: /bench/data/input.txt)
    -h, --help           Print help and exit
    -V, --version        Print version and exit
```

### Typical workflow

1. Type a regex into the **Pattern** field; matches appear instantly with
   highlights and offsets.
2. Press `r` to focus the **Replace** field and type a replacement string; the
   **Preview** pane shows the complete replaced file content.
3. Press `i` (from the matches/preview area) to toggle case-insensitive mode.
4. Press `F2` and pick a preset (e.g. **China mobile phone**) with `↑`/`↓` and
   `Enter`.

## Keyboard reference

| Keys | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Cycle focus: pattern → replacement → matches → preview |
| `/` | Focus the pattern input |
| `r` | Focus the replacement input |
| `i` | Toggle case-insensitive matching (`(?i)`) |
| `F2` | Open the preset menu |
| `Ctrl+R` | Reload the input file |
| `F1` / `?` / `h` | Show the full help overlay |
| `q` / `Ctrl+C` | Quit |

**Editing the focused input**

| Keys | Action |
| --- | --- |
| type | Insert characters |
| `←` / `→` | Move the cursor |
| `Backspace` / `Delete` | Delete a character |
| `Home` / `End` | Jump to start / end |
| `Ctrl+A` / `Ctrl+E` | Jump to start / end |
| `Ctrl+U` | Clear the input |
| `Ctrl+K` | Delete to end of line |
| `Esc` | Clear the focused input |
| `Enter` | Leave input, focus the matches |

**Browsing the matches / preview**

| Keys | Action |
| --- | --- |
| `↑` / `↓` | Move selection (matches) or scroll (preview) |
| `PgUp` / `PgDn` | Page through matches / preview |
| `Home` / `End` | Jump to first / last |
| `Enter` (on a match) | Jump the preview to that match's line |

## Notes on offsets

Offsets are **character offsets** (Unicode scalar values), not byte offsets or
grapheme clusters, counted from the beginning of the file. This keeps offsets
intuitive even when multibyte characters appear before a match.

## Development

```sh
cd toole
cargo test     # run unit + render tests
cargo clippy --all-targets
cargo build --release
```

The render tests use ratatui's `TestBackend` to assert, without a terminal,
that matches carry a yellow background highlight, replacements carry a green
background, offsets are displayed, and both panes are on the same screen.

## Project layout

```
toole/
├── Cargo.toml
├── src/
│   ├── main.rs   # CLI parsing, terminal setup, event loop
│   ├── app.rs    # state, matching, offsets, replacement, presets, key handling
│   └── ui.rs     # ratatui rendering (layout, highlighting, overlays)
└── README.md
```
