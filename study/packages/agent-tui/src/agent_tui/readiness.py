"""Detect shell/bootstrap output vs an interactive TUI ready for the agent."""

from __future__ import annotations

import re
from dataclasses import dataclass

# Common init/bootstrap output — not an interactive TUI yet.
BOOTSTRAP_PATTERNS: tuple[re.Pattern[str], ...] = (
    re.compile(r"Reading package lists\.\.\.", re.I),
    re.compile(r"Building dependency tree\.\.\.", re.I),
    re.compile(r"Reading state information\.\.\.", re.I),
    re.compile(r"^Get:\d+\s+", re.M),
    re.compile(r"Need to get \d+ kB of archives", re.I),
    re.compile(r"\d+% \[.+?\]", re.I),
    re.compile(r"^Unpacking ", re.M),
    re.compile(r"^Setting up ", re.M),
    re.compile(r"apt-get (install|update|upgrade)", re.I),
    re.compile(r"^Fetched \d+", re.M),
    re.compile(r"^Preparing to unpack", re.M),
    re.compile(r"npm (WARN|ERR!|notice)", re.I),
    re.compile(r"pip install", re.I),
    re.compile(r"cargo (build|fetch|compile)", re.I),
    re.compile(r"Downloading \d+", re.I),
    re.compile(r"^\+ ", re.M),  # set -x / install scripts
)


@dataclass(frozen=True)
class ScreenReadyConfig:
    """When the screen looks ready for agent interaction."""

    min_text_len: int = 40
    ready_pattern: str | None = None

    def matches(self, text: str) -> bool:
        stripped = text.strip()
        if len(stripped) < self.min_text_len:
            return False
        if self.ready_pattern:
            return bool(re.search(self.ready_pattern, text, re.I | re.S))
        if is_bootstrap_screen(text):
            return False
        return True


def is_bootstrap_screen(text: str) -> bool:
    return any(p.search(text) for p in BOOTSTRAP_PATTERNS)


def is_screen_ready(text: str, *, config: ScreenReadyConfig | None = None) -> bool:
    cfg = config or ScreenReadyConfig()
    return cfg.matches(text)
