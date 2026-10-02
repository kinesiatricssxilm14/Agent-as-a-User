# tooln

`tooln` is a keyboard-first Bubble Tea TUI for managing real pip and Debian apt packages.

## Install

```sh
cd tooln
go install .
tooln
```

The runtime should provide `python3`, `python3-pip`, `apt-get`, `apt-cache`, and `dpkg-query`.
Run as root when installing or removing system packages. Press `?` inside the application
for the complete, discoverable keyboard map.

Core keys: `Tab` switch manager, arrows navigate, `/` filter, `s` search PyPI, `i`
install, `u` uninstall, `U` upgrade, `r` refresh, `?` help, `q` quit.
