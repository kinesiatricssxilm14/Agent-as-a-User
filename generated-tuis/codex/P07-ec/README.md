# toolg

`toolg` is a keyboard-only three-way Git merge conflict resolver for the terminal. It uses Bubble Tea, Bubbles, and Lip Gloss and performs real filesystem and Git operations.

## Install

```sh
cd toolg
go install .
```

## Run

The benchmark/default invocation uses `/bench/data/repo` and `conflict.py`:

```sh
toolg conflict.py
```

For another repository or file:

```sh
toolg -C /path/to/repository path/to/conflicted-file
```

The file path is relative to the selected repository directory. It must remain inside the repository.

## Keys

- `Left`/`Right`, `j`/`k`, `Tab`/`Shift+Tab`: select a conflict
- `o`, `t`, `b`, `n`: resolve with ours, theirs, both, or neither
- `O`, `T`, `B`, `N`: apply that choice to every conflict
- `u`: mark the selected conflict unresolved again
- `PgUp`/`PgDn`: scroll the three merge panes together
- `w` or `Ctrl+S`: write marker-free output and run `git add`
- `c`: save, stage, and prompt for a real `git commit` message
- `r`: reload the working-tree file
- `?`: expanded help; `q`: quit

The latest Git history is always shown below the merge panes and refreshes after committing.
