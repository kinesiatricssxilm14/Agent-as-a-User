# toolh

A keyboard-driven TUI for organising, searching and reusing code snippets.

`toolh` keeps your snippets in a real SQLite database, shows every field of the
selected snippet on one screen, and copies code to the clipboard with a single
keypress.

```
╭──────────────────────────────────────────────────────────────────────────────────────────────╮
│ Search snippets - words, tag:cli, lang:python, title:foo, "exact phrase"  │ sort: Recently … │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ Snippets (2) ───────────────────────╮╭─ Detail - #2 ──────────────────────────────────────────╮
│  #  Title       Language  Tags  Lines││ Title     git undo                                     │
│  2  git undo    bash      cli…    2  ││ Language  bash                                         │
│  1  hello-world python    demo    1  ││ Tags      cli, git                                     │
│                                      ││ Updated   2026-08-13T15:10:38Z                         │
│                                      ││                                                        │
│                                      ││ Description                                            │
│                                      ││ Undo the last commit but keep changes staged           │
│                                      ││                                                        │
│                                      ││ Code (2 lines)                                         │
│                                      ││   1 git reset --soft HEAD~1                            │
│                                      ││   2 git status                                         │
╰──────────────────────────────────────╯╰────────────────────────────────────────────────────────╯
 2 snippet(s) | sort: Recently updated | db: /root/.local/share/toolh/snippets.db
 ^q Quit  ? Help  ^n New  ^e Edit  r Rename  d Delete  c Copy code  y Copy all  / Search  s Sort
```

## Install

```bash
cd toolh && pip install .
```

Then launch it with no arguments:

```bash
toolh
```

Press `?` inside the app for the full key map — you never need this README to
operate the tool.

## Keys

| Area | Keys | Action |
| --- | --- | --- |
| Navigation | `↑`/`↓`, `j`/`k` | Move the selection |
| Navigation | `home`/`end`, `g`/`G` | First / last snippet |
| Navigation | `pageup`/`pagedown` | Scroll a screen at a time |
| Navigation | `tab`/`shift+tab` | Move focus between search, list and panes |
| Navigation | `enter` | Focus the detail pane to scroll long code |
| Search | `/` or `ctrl+f` | Jump to the search box (filters as you type) |
| Search | `escape` | Clear the search / close the editor or a prompt |
| Search | `s` | Cycle the sort order |
| Create & edit | `ctrl+n` | New snippet |
| Create & edit | `ctrl+e` or `F2` | Edit the selected snippet |
| Create & edit | `r` | Rename (title only) |
| Create & edit | `ctrl+d` | Duplicate |
| Create & edit | `ctrl+s` | Save while editing |
| Delete | `d` or `delete`, then `y`/`n` | Delete with confirmation |
| Clipboard | `c` | Copy the code |
| Clipboard | `y` | Copy the whole snippet (all fields) |
| Other | `?` or `F1` | Help |
| Other | `ctrl+r` | Reload from the database |
| Other | `ctrl+q` | Quit |

## Search syntax

Every word must match (AND). Scope a word to one field, quote phrases, and
prefix with `-` to exclude:

```
greeting                 # anywhere: title, language, description, code or tags
tag:cli lang:python      # a tag AND a language
title:undo               # title only
desc:commit code:reset   # description AND code body
"last commit"            # exact phrase
git -tag:wip             # matches git, excludes anything tagged wip
id:12                    # a specific snippet
```

Aliases: `lang`/`language`, `desc`/`description`, `tag`/`tags`, `code`/`body`,
`title`/`name`.

## Fields

Each snippet has a **title**, **language**, **tags**, **description** and
**code** body. The detail pane shows all five at once, plus created/updated
timestamps — no tabs or pop-ups hide anything.

## Storage

Snippets live in a SQLite database. The location is resolved in this order
(highest priority first):

1. `toolh --db /path/snippets.db`
2. `TOOLH_DB=/path/snippets.db`
3. `"database"` in the config file
4. `TOOLH_HOME=/dir` → `/dir/snippets.db`
5. `$XDG_DATA_HOME/toolh/snippets.db`
6. `~/.local/share/toolh/snippets.db`

Check what is in effect at any time:

```bash
toolh --where
```

The schema is three tables — `snippets`, `tags` and `snippet_tags` — so tags are
queried in SQL rather than by string matching. Inspect it with the normal tools:

```bash
sqlite3 ~/.local/share/toolh/snippets.db "SELECT id, title, language FROM snippets;"
```

### Config file

Optional JSON at `TOOLH_CONFIG`, else `$TOOLH_HOME/config.json`, else
`$XDG_CONFIG_HOME/toolh/config.json`, else `~/.config/toolh/config.json`:

```json
{
  "database": "/data/snippets.db",
  "clipboard_command": "xclip -selection clipboard",
  "clipboard_mirror": "/tmp/toolh-clipboard.txt"
}
```

A malformed config is ignored rather than fatal.

## Clipboard

Copying tries real mechanisms in order and reports which one worked:

1. `TOOLH_CLIPBOARD_COMMAND` / `clipboard_command` — your own command, text on stdin
2. `pbcopy`, `wl-copy`, `xclip`, `xsel`, `clip.exe` — whichever is installed
3. `pyperclip`, if installed (`pip install 'toolh[clipboard]'`)
4. **OSC 52** — asks the terminal emulator itself, which works over SSH and
   inside Docker where there is no X display

If none succeed, `toolh` says so instead of pretending. Set
`TOOLH_CLIPBOARD_FILE=/path/file` to also mirror every copy to a file, which
makes copying verifiable on a headless box:

```bash
TOOLH_CLIPBOARD_FILE=/tmp/clip.txt toolh   # press c, then: cat /tmp/clip.txt
```

## Scripting

The same storage layer is reachable without a terminal:

```bash
toolh --where                                     # resolved paths + backends
toolh --add "hello-world" --language python \
      --tags demo --description "Print a greeting" --code 'print("hello")'
toolh --add "from-file" --code - < snippet.py     # read the body from stdin
toolh --list                                      # every snippet as JSON
toolh --search 'tag:cli lang:bash'                # filtered JSON
```

## Development

```bash
pip install -e '.[test]'
python -m pytest            # 99 tests: storage, config, clipboard, CLI and UI
```

The UI tests drive the app with real key presses through Textual's `Pilot` and
assert against the SQLite file, so "the screen says it saved" and "the database
contains it" are checked as separate facts.

## Docker

```bash
docker build -t toolh .
docker run --rm -it -v toolh-data:/data toolh
```

The image is Debian 12 slim with `git` and `sqlite3`, runs as root, sets
`TOOLH_DB=/data/snippets.db`, and starts `toolh` by default.

## Requirements

Python 3.8+, [Textual](https://github.com/Textualize/textual) and
[Rich](https://github.com/Textualize/rich). Licensed under MIT.
