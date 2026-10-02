# toolj

A keyboard-first Redis database management TUI written in Rust with ratatui and crossterm.

## Build and install

```sh
cargo build --release
cp target/release/toolj /usr/local/bin/
toolj
```

The initial connection is `redis://localhost:6379/0`. Named Redis connections can be added from the server screen with `n`; URIs support database numbers and ACL/password credentials, for example `redis://user:password@localhost:6379/2`.

## Keyboard map

- `Enter`: connect or accept an input
- `1` / `2` / `3` / `4`: keys, streams, pubsub, ACL views
- `:`: resource selector (`:keys`, `:streams`, `:pubsub`, `:acl`)
- `↑` / `↓` or `j` / `k`: select or scroll
- `←` / `→` or `h` / `l`: focus list/detail
- `/`: live search
- `t`: cycle Redis key type filter
- `n`: create key; `e`: edit (or append to stream); `d`: delete
- `r`: refresh from Redis; `c`: return to servers
- `?`: in-app help; `q`: quit; `Esc`: cancel

Strings are edited as plain text. Hashes and stream messages use JSON objects, lists and sets use JSON arrays, and sorted sets use JSON objects whose values are numeric scores. All reads and mutations are issued to the connected Redis server using the Redis wire protocol.
