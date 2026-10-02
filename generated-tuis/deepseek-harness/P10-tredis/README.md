# toolj

A keyboard-driven Redis database management TUI, built with Rust,
[ratatui](https://ratatui.rs) and [crossterm](https://crates.io/crates/crossterm).

`toolj` gives you a visual, searchable, same-screen view over a Redis server so
you can browse, create, edit and delete keys of every Redis data type, inspect
streams and Pub/Sub channels, and review ACL users — without memorising
`redis-cli` commands.

## Build & install

```sh
cd toolj
cargo build --release
cp target/release/toolj /usr/local/bin/
```

The binary is then available as `toolj` on `PATH`.

## Run

```sh
toolj
```

On startup it connects to the default server `redis://localhost:6379/0`.
Everything is keyboard driven; no mouse is required.

## Key bindings

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | switch resource (`:keys`, `:streams`, `:pubsub`, `:acl`) |
| `←` / `→` (`h` / `l`) | move focus between the list and detail panels |
| `↑` / `↓` (`k` / `j`) | move selection / scroll the detail panel |
| `PgUp` / `PgDn` | page scroll |
| `Home` / `End` (`g` / `G`) | jump to top / bottom |
| `Enter` | view the selected item / toggle focus |
| `1`–`7` | filter keys by type (1 all, 2 string, 3 hash, 4 list, 5 set, 6 zset, 7 stream) |
| `/` | live search filter over key names (`Esc` cancels, `Enter` accepts) |
| `e` | edit the selected key (string = value; other types = add an item) |
| `a` | create a new key / add an item when the detail panel is focused |
| `d` | delete a key / delete an item when the detail panel is focused |
| `r` | refresh the current resource |
| `s` | add a named server (name + Redis URI) |
| `c` | connect / disconnect |
| `[` / `]` | previous / next server |
| `:` | command line (`keys`, `streams`, `pubsub`, `acl`, `connect`, `disconnect`, `refresh`, `add-server`, `help`, `quit`) |
| `?` / `F1` | toggle help |
| `q` / `Ctrl+C` | quit |

## Views

- **Keys** — total key count plus a scrollable list of every key with its type.
  Selecting a key shows its full value on the same screen:
  - *string*: the raw value
  - *hash*: all field/value pairs
  - *list*: all elements, in order
  - *set*: all members
  - *sorted set*: all members with scores
  - *stream*: all messages (id + fields)
- **Streams** — the complete list of stream keys; selecting one shows all of its
  messages on the same screen.
- **PubSub** — the complete list of channels reported by `PUBSUB CHANNELS`
  (including currently subscribed channels) with subscriber counts.
- **ACL** — the complete list of ACL usernames (including `default`); selecting a
  user shows its permission configuration.

## Creating data

When creating a key you are prompted for a value; multi-value types use a
compact, comma-separated syntax:

| Type | Syntax | Example |
| --- | --- | --- |
| string | raw value | `hello world` |
| hash | `field=value,...` | `name=alice,age=30` |
| list / set | `item,item,...` | `a,b,c` |
| sorted set | `member=score,...` | `alice=3.5,bob=1` |
| stream | `field=value,...` | `sensor=42,temp=21` |

## Notes

- All operations go through real Redis commands (`SCAN`, `TYPE`, `GET`, `HGETALL`,
  `LRANGE`, `SMEMBERS`, `ZRANGE`, `XRANGE`, `PUBSUB`, `ACL`, …); nothing is
  simulated.
- The key scan is capped at 20,000 keys per refresh to keep the UI responsive on
  very large databases.
