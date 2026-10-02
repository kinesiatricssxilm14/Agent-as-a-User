import pytest

from agent_tui.readiness import ScreenReadyConfig, is_bootstrap_screen, is_screen_ready

APT_OUTPUT = """Reading package lists... Done
Building dependency tree... Done
Get:1 http://deb.debian.org/debian wget arm64 1.21.3 [922 kB]
66% [14 librtmp1 878 B/59.4 kB 1%]
"""

APTUI_OUTPUT = """╭─ Package List ──────────────────────────────────────────────── 1/113─╮
│           Name                         Version                   Size│
│ ▌ [ ] ●  adduser                      3.134                   686 kB │
╰──────────────────────────────────────────────────────────────────────╯
╭─ Keys ───────────────────────────────────────────────────────────────╮
│space select • i install • / search • q quit                          │
╰──────────────────────────────────────────────────────────────────────╯
"""


def test_bootstrap_detects_apt_install():
    assert is_bootstrap_screen(APT_OUTPUT)


def test_bootstrap_rejects_aptui():
    assert not is_bootstrap_screen(APTUI_OUTPUT)


def test_screen_ready_requires_non_bootstrap():
    cfg = ScreenReadyConfig(min_text_len=40)
    assert not is_screen_ready(APT_OUTPUT, config=cfg)
    assert is_screen_ready(APTUI_OUTPUT, config=cfg)


def test_ready_pattern_optional():
    cfg = ScreenReadyConfig(min_text_len=10, ready_pattern=r"Package List")
    assert is_screen_ready(APTUI_OUTPUT, config=cfg)
    assert not is_screen_ready("some other long enough screen text here\n" * 3, config=cfg)
