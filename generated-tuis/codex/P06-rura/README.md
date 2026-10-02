# toolf

`toolf` is a keyboard-driven shell pipeline debugger. It feeds a selected log
file to the pipeline's standard input, previews changes, runs a prefix of the
pipeline, and saves real command output.

## Install and run

```sh
cargo install --path .
toolf --file /bench/server.log
```

The initial pipeline is `tail -n 50`. Edit it directly; the selected file is
connected to stdin, so commands such as `grep ERROR | tail -n 20` work without
putting the filename in the command.

## Main keys

| Key | Action |
| --- | --- |
| `Enter` / `Ctrl-R` | Run the complete pipeline |
| `Alt-\` / `F6` | Run everything before the editor cursor |
| `Tab` / `Shift-Tab` | Complete commands and paths |
| `Ctrl-S` | Enter an output path and save |
| `PageUp` / `PageDown` | Scroll output |
| `F1` | Show in-app help |
| `Ctrl-C` | Quit |

Preview execution is debounced while editing and commands are stopped after
five seconds. `bash` is used with `pipefail`; the actual file system, shell
commands, and output files are used.

