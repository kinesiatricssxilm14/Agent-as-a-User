# toole

A keyboard-driven interactive regular expression tester for text files.

## Install

```sh
cargo install --path .
```

## Run

```sh
toole -f /bench/data/input.txt
```

Type a regular expression and replacement rule directly into the two top fields. Matching, background highlighting, character offsets, and the complete replacement preview update immediately.

Key bindings are always summarized in the status area; leave an input with `Esc`, then press `?` for expanded help. Important shortcuts include `Tab`/`Shift+Tab` to move focus, `F2` for case-insensitive mode, `Ctrl+P` for the mainland China mobile-number pattern, arrows or `j`/`k` to scroll, and `Ctrl+C` to quit.
