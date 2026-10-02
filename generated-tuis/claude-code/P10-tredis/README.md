# toolj

A Redis database management TUI: browse, search, inspect, edit and delete
every Redis data type from the terminal, plus Pub/Sub channels, Streams and
ACL users.

Built with [ratatui](https://ratatui.rs) + crossterm. Every value shown and
every change made goes through real Redis commands — nothing is simulated.

## Install

```sh
cd toolj && cargo build --release && cp target/release/toolj /usr/local/bin/
```

Then run it with no arguments:

```sh
toolj
```

It connects to `redis://localhost:6379/0` by default. `--uri` (or `$REDIS_URL`)
points it elsewhere:

```sh
toolj --uri redis://localhost:6380/2
toolj --uri redis://:anonymous@example.invalid:6379/0 --name production
```

`toolj --help` lists the options; `toolj --version` prints the version.

## Getting oriented

Press `?` (or `F1`) at any time for a help page listing every binding — you
never need this README to operate the tool. The bottom two lines always show
the last operation's result and the keys available in the current context.

The six resource views are on the number keys, `Tab`/`Shift-Tab`, or the `:`
selector (`:keys`, `:streams`, `:pubsub`, `:acl`, `:servers`, `:info`):

| View | What it shows |
| --- | --- |
| `:keys` | total key count, a type-filter bar with per-type counts, and the full contents of the selected key |
| `:streams` | every stream in the database with length and consumer-group counts, plus the selected stream's entries |
| `:pubsub` | every channel on the server, which ones this session subscribes to, and messages as they arrive |
| `:acl` | every ACL user with its rule line and full `ACL GETUSER` output |
| `:servers` | saved named connections; add, edit, remove, connect |
| `:info` | the complete `INFO` output plus per-database key counts |

Two panes: a list on the left, detail on the right. `h`/`l` (or `←`/`→`) move
focus between them; `↑`/`↓` (or `k`/`j`) move the selection. All content is
visible on one screen and scrolls in place — there are no modal overlays or
pages that hide what you were looking at.

## Navigating and searching

| Key | Action |
| --- | --- |
| `/` | filter the list by name as you type — substring, or glob if you use `*`/`?` |
| `t` / `T` | cycle the key type filter (all → string → hash → list → set → sorted set → stream) |
| `s` | set the server-side `SCAN MATCH` pattern |
| `g` / `G`, `Home` / `End` | jump to the first / last row |
| `PgUp` / `PgDn`, `Ctrl-u` / `Ctrl-d` | move by a page |
| `Esc` | close a prompt, else clear the filter, else leave the detail pane |
| `r` / `F5` | reload the current view |

`/` filters names already loaded; `s` changes what the server returns. On a
large keyspace, narrow with `s` first.

## Viewing and editing

The detail pane renders each type in a form suited to it: strings wrap as text,
hashes and sorted sets as aligned two-column tables, lists with their indices,
streams grouped by entry ID. The header shows type, TTL, encoding and memory.

| Key | Action |
| --- | --- |
| `Enter` | load the selected key and focus the detail pane |
| `n` | create a key: `<type> <key> <value...>` |
| `e` | edit — on a list the whole key, in the detail pane just the highlighted item |
| `a` | append a field / element / member / entry |
| `d` | delete the key (asks to confirm) |
| `D` | delete the highlighted field / element / member / entry |
| `m` | rename the key |
| `x` | set or clear the TTL (`60`, `5m`, `2h`, `1d`, or `-1` to persist) |

Creating keys — the type decides how the rest is read:

```
string  greeting hello world           SET
hash    user:1 name=alice age=30       HSET  (or: name alice age 30)
list    queue "build image" "run"      RPUSH (quote to keep spaces)
set     tags redis rust tui            SADD
zset    scores alice=10 bob=20         ZADD  (or: 10 alice)
stream  events * event=login           XADD  (`*` auto-generates the ID)
```

Deleting a list element preserves the order of the rest. Editing a set member
replaces it (`SREM` + `SADD`); editing a sorted-set member changes its score.

## Pub/Sub

`:pubsub` lists the channels the server currently knows (`PUBSUB CHANNELS`)
with their subscriber counts. `●` marks a channel this session subscribes to.

| Key | Action |
| --- | --- |
| `Enter` / `Space` | subscribe / unsubscribe to the selected channel |
| `P` | subscribe to a glob pattern (`PSUBSCRIBE`), e.g. `news.*` |
| `p` | publish a message to the selected channel |
| `c` | clear the received-message buffer |
| `u` | unsubscribe from everything |

Each subscription runs on its own thread and connection, so messages appear
while you keep browsing. The channel list refreshes every few seconds.

## ACL

`:acl` lists every user, including `default`, with the `ACL LIST` rule line
and the full `ACL GETUSER` breakdown (flags, passwords, commands, keys,
channels, selectors).

`a` creates or modifies a user; rules are passed to `ACL SETUSER` verbatim:

```
alice on >secret ~cache:* +@read
reporter on >pw ~report:* +@read +@keyspace
```

`e` pre-fills the prompt with the selected user's current rules. `d` deletes a
user (`default` is protected).

## Connections

`:servers` holds named connections, saved to `~/.config/toolj/servers.toml`
(override with `$TOOLJ_CONFIG`). `a` adds one as `<name> <uri>`:

```
New_user redis://localhost:6379/0
```

`Enter` connects, `e` edits, `d` removes, `w` writes the file. Bare forms like
`localhost:6380` or `:6380` are expanded to full URIs. If the initial
connection fails, toolj still starts and opens `:servers` so you can fix it.

## Command line

`:` opens a command line (`↑`/`↓` recalls history):

| Command | Effect |
| --- | --- |
| `:keys` `:streams` `:pubsub` `:acl` `:servers` `:info` `:help` | switch view |
| `:keys user:*` | switch view and filter in one step |
| `:connect <name\|uri>` | connect to a saved name or a raw URI |
| `:add <name> <uri>` | save a named server and connect |
| `:select <db>` | `SELECT` another logical database |
| `:scan <pattern>` | set the `SCAN MATCH` pattern |
| `:filter <text>` / `:type <type>` | set the name / type filter |
| `:get <key>` | jump to a key by exact name |
| `:set <key> <value>` / `:del <key>` | write / delete a key |
| `:publish <chan> <msg>` / `:subscribe <chan>` | Pub/Sub from the keyboard |
| `:cmd <redis command>` | run any Redis command and show the reply |
| `:q` | quit |

`:cmd` is the escape hatch for anything the views don't cover:

```
:cmd CONFIG GET maxmemory
:cmd OBJECT FREQ mykey
:cmd XINFO STREAM events:log
```

## Notes on large data

To stay responsive, one refresh scans at most 50 000 keys, loads at most 5 000
members of a collection and 500 stream entries. When a limit truncates the
view, the pane header and a warning line say so — nothing is silently hidden.

Values are read as raw bytes. Invalid UTF-8 is replaced with `�` and control
bytes render as `·`, so a binary value cannot corrupt the display or inject
terminal escape sequences.

## Development

```sh
cargo test              # unit tests for parsing, filtering, layout and text handling
cargo build --release
```

Layout of `src/`:

| File | Responsibility |
| --- | --- |
| `main.rs` | CLI, terminal setup/teardown, event loop |
| `app.rs` | application state, selection, filtering, refresh |
| `db.rs` | all Redis access; the only module that talks to the server |
| `ui.rs` | rendering |
| `input.rs` | key dispatch |
| `actions.rs` | mutations and the `:` command line |
| `sub.rs` | background Pub/Sub subscriber threads |
| `config.rs` | the named-server file |
| `help.rs` | key documentation (help page and footer hints) |
| `util.rs` | text wrapping, width-aware padding, tokenizing |
