# tooli

`tooli` is a keyboard-driven disk-space explorer built with Rust, ratatui, and
crossterm. It recursively scans a directory, presents a browsable tree, a
proportional treemap, Top-N files, counts, filters, sorting, and real file
deletion.

## Install

```sh
cd tooli
cargo install --path .
```

## Run

```sh
tooli /bench/data
```

The directory argument is optional and defaults to `/bench/data`.

Press `?` inside the application for the complete keyboard reference. The
bottom bar always shows the principal shortcuts.

## Safety

- Deletion is restricted to files and always requires confirmation.
- Symbolic links are measured as link entries but are never followed.
- Refresh with `r` after external filesystem changes.
