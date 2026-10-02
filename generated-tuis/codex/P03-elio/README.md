# toolc

`toolc` is a keyboard-only, dual-pane file manager for Linux terminals. The
left pane browses the current directory and the right pane displays the
selected file's name, path, and content. Operations use the real filesystem.

## Install

```sh
cd toolc
cargo install --path .
```

Run with the benchmark default:

```sh
toolc /bench/data/src
```

Or open another directory:

```sh
toolc /path/to/directory
```

If no path is given, `/bench/data/src` is used.

## Keys

| Key | Action |
| --- | --- |
| `Up` / `Down`, `j` / `k` | Select a file or scroll the preview |
| `Enter` / `Right` | Open a directory; focus a file preview |
| `Backspace` / `Left` | Open the parent directory |
| `Tab` | Switch between file-list and preview focus |
| `PageUp` / `PageDown` | Move or scroll by a page |
| `Home` / `End` | First/last item or start/end of preview |
| `c` | Copy selected file |
| `m` | Move selected item to a path or directory |
| `e` | Rename selected item |
| `d` | Delete selected item after typing `DELETE` |
| `n` | Create a directory (including missing parent directories) |
| `r` | Refresh the directory |
| `?` / `F1` | Toggle in-application help |
| `q` / `Ctrl-C` | Quit |

Operation paths can be absolute, relative to the currently open directory, or
start with `~/`. Existing destinations are never overwritten. Copying a file
or moving an item to an existing directory preserves its original name.

## Safety and behavior

- Copy creates a byte-identical file and leaves the source in place.
- Move and rename remove the original path after a successful operation.
- Delete requires the exact confirmation text `DELETE`.
- The file list refreshes immediately after each successful operation.
- Directory deletion is recursive.
- Text previews keep the complete file in memory and are scrollable. Files
  containing NUL bytes are marked as binary and shown with lossy text decoding.

## Development

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
