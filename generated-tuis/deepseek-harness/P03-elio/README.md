# toolc

A keyboard-driven, dual-pane file manager TUI for Linux administration.

`toolc` browses a directory as a file list with a live content preview, and
performs copy, move, rename, delete, and directory-creation operations using
the real filesystem (no simulation).

## Install

```sh
cd toolc
cargo install --path .
```

This installs the `toolc` binary into Cargo's bin directory.

## Usage

```sh
toolc /bench/data/src
```

The first command-line argument is the directory to open. It defaults to
`/bench/data/src` when omitted.

## Key bindings

| Key | Action |
|-----|--------|
| `Up`/`k`, `Down`/`j` | Move selection |
| `Enter`/`Right` | Open selected directory |
| `Left`/`Backspace` | Go to parent directory |
| `Home`/`g`, `End`/`G` | Jump to top / bottom of list |
| `PageUp`/`PageDown` | Page through the list |
| `Tab` | Switch focus between list and preview |
| `F2` | Rename selected entry |
| `F5` | Copy selected file to a destination |
| `F6` | Move selected entry to a destination |
| `F7` | Create a new directory |
| `F8`/`Delete` | Delete selected entry (with confirmation) |
| `/` | Filter the list by name |
| `F1`/`?` | Show help |
| `q`/`Ctrl-C` | Quit |

When the preview panel is focused, `Up`/`Down`, `PageUp`/`PageDown`, and
`Home`/`End` scroll the preview content.

## Notes

- Copy is byte-identical (`std::fs::copy`).
- Move/rename remove the original path (`std::fs::rename`, with a
  copy-and-delete fallback for cross-device moves).
- Delete removes files and directories (directories are removed recursively).
- Directory creation uses `std::fs::create_dir_all`, so nested paths such as
  `archive/2024` also work.
