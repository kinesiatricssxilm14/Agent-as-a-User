from __future__ import annotations

import os
import subprocess
from typing import Callable


def _tmux_prefix() -> list[str]:
    try:
        from agent_tui.tmux_backend import tmux_cli_prefix

        return list(tmux_cli_prefix())
    except ImportError:
        sock = (os.environ.get("TUI_BENCH_TMUX_SOCKET") or "tui-bench").strip() or "tui-bench"
        return ["tmux", "-L", sock]


def kill_bench_tmux_sessions(*, log: Callable[[str], None] | None = None) -> int:
    """Kill leftover bench tmux sessions (bench-s-*) so resume can start cleanly."""
    _log = log or (lambda _msg: None)
    prefix = _tmux_prefix()
    proc = subprocess.run(
        [*prefix, "list-sessions", "-F", "#{session_name}"],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        return 0

    killed = 0
    for line in proc.stdout.splitlines():
        name = line.strip()
        if not name.startswith("bench-s-"):
            continue
        stop = subprocess.run(
            [*prefix, "kill-session", "-t", name],
            capture_output=True,
            text=True,
            check=False,
        )
        if stop.returncode == 0:
            killed += 1
            _log(f"  English-only text tmux English-only text: {name}")
    if killed:
        _log(f"English-only text {killed} English-only text bench tmux English-only text")
    return killed
