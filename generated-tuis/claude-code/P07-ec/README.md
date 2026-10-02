# toolg

A terminal UI for resolving Git merge conflicts.

`toolg` opens a file that Git left in a conflicted state and shows the competing
versions side by side — **ours**, the **result** being built, and **theirs** — so
a conflict can be inspected and resolved with single keystrokes. The resolved
file is written back to the working tree and the merge can be committed without
leaving the tool.

Every operation that touches repository state runs the real `git` command. There
is no simulation: what the interface shows is what `git` reports, and what it
commits is what `git` commits.

## Install

```sh
cd toolg && go install .
```

This produces a `toolg` executable in `$(go env GOPATH)/bin`.

## Usage

```sh
toolg [flags] [file]
```

With no arguments, `toolg` opens `conflict.py` in `/bench/data/repo` when that
directory exists, falling back to the current directory otherwise.

```sh
toolg                        # default file in the default working directory
toolg src/app.go             # a specific conflicted file
toolg -C /path/to/repo f.c   # a file in another repository
```

| Flag | Meaning |
| --- | --- |
| `-C`, `-dir` | run as if started in this directory |
| `-log-limit` | how many commits the history view loads (default 200) |
| `-version` | print the version and exit |

## The screen

```
 toolg  branch main │ MERGING ← feature │ file conflict.py │ resolved 0/1
   ■ ours = HEAD  ■ theirs = incoming
╭────────────────────────╮╭────────────────────────╮╭───────────────────────╮
│OURS  HEAD              ││RESULT  written to …    ││THEIRS  feature        │
│    1 line before       ││    1 line before       ││    1 line before      │
│      ▶ conflict 1/1    ││      ▶ conflict 1/1    ││      ▶ conflict 1/1   │
│    2 main branch text  ││    2 <<<<<<< HEAD      ││    2 feature branch…  │
│      ················  ││    3 main branch text  ││      ···············  │
│    3 line after        ││    7 line after        ││    3 line after       │
╰────────────────────────╯╰────────────────────────╯╰───────────────────────╯
• 1 conflict(s) to resolve — o ours · t theirs · b both · d discard
n/tab next conflict · o/1 take ours · … · ? help · q quit
```

All panels are drawn from one shared row list, so a conflict's competing texts
always sit on the same screen line. Conflict content carries a **background
highlight** per side; the block under the cursor is highlighted more strongly.
The result panel is green where a decision has been made and red where one is
still needed. Dotted rows are padding, marking where one side has no
corresponding line.

Every panel is visible at once. Nothing is hidden behind a tab or a page, and
the header, footer and panels stay on screen together.

## Keys

Press `?` inside the tool for the full reference. The most used keys:

| Key | Action |
| --- | --- |
| `n` / `p`, `tab` | next / previous conflict |
| `o` `t` `b` `d` | take ours / theirs / both / discard, for the current conflict |
| `O` `T` `B` | apply ours / theirs / both to **every** conflict |
| `u` | undo the choice, restoring the conflict |
| `e` | edit the resolved text by hand (`ctrl+s` applies, `esc` cancels) |
| `s` | write the file back to the working tree |
| `c` | create the merge commit |
| `L` `f` `m` | git history / conflicted files / merge view |
| `v` | show the base (common ancestor) panel |
| `r` | reload from disk, discarding unsaved choices |
| `X` | abort the merge (`git merge --abort`) |
| `?` `q` | help / quit |

Navigation uses the arrow keys or `hjkl`, with `pgup`/`pgdn`, `home`/`g` and
`end`/`G`. Everything is reachable from the keyboard alone.

## Workflow

1. `n` / `p` to move between conflicts.
2. `o` / `t` / `b` / `d` to choose a strategy, or `e` to type the resolution.
3. `s` to write the file back. Conflict markers are removed and the file is
   staged, which is how Git records a conflict as resolved.
4. `f` to move on to the next conflicted file, if any.
5. `c` to create the merge commit, then `L` to review the result.

Resolutions are held in memory until `s` is pressed, so choices can be revised
freely; the header shows `UNSAVED` while there is unwritten work.

## Behaviour worth knowing

- **Non-conflict lines are never touched.** The file is modelled as untouched
  runs interleaved with conflict blocks, and only block content is replaced.
  Line endings (LF or CRLF), file permissions and a missing final newline are
  all preserved.
- **Partial saves stay valid.** Saving with conflicts still unresolved keeps
  those blocks' markers, so the file remains a well-formed conflict file, and
  the file is *not* staged — staging it would tell Git a conflict was settled
  while markers are still present.
- **Writes are atomic.** The file is written to a temporary file in the same
  directory, flushed to disk, then renamed over the target, so an interrupted
  save cannot truncate your source.
- **Damaged files are refused, not guessed at.** A malformed marker sequence is
  reported instead of being reinterpreted, since guessing risks destroying work.
- **diff3 conflicts are supported.** When markers carry a `|||||||` ancestor
  section, `v` shows it in a fourth panel. The ancestor never leaks into a
  resolution.
- **Committing is checked first.** It is refused, with a message naming the
  outstanding files, while any conflict in this or another file is unresolved.
  The commit message is seeded from Git's own prepared `MERGE_MSG`.
- **Git's own output is shown.** When a command fails, the tool displays git's
  message rather than a paraphrase.

## Layout

```
toolg/
  main.go                     CLI entry point, flag and path resolution
  internal/conflict/          marker parsing, resolution and atomic write-back
  internal/gitx/              real git invocations: status, log, stage, commit
  internal/ui/                Bubble Tea model, key map, layout, rendering
```

`internal/conflict` and `internal/gitx` have no dependency on the UI, so the
parsing and Git layers are tested directly against real repositories.

## Tests

```sh
go test ./...
```

The suite builds real Git repositories with real conflicting merges and drives
the model through actual keystrokes, asserting on the bytes left on disk and on
what `git` reports afterwards — including that a completed merge commit really
has two parents.

## Built with

[Bubble Tea](https://github.com/charmbracelet/bubbletea),
[Bubbles](https://github.com/charmbracelet/bubbles) and
[Lip Gloss](https://github.com/charmbracelet/lipgloss).
