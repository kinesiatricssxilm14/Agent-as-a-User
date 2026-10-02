from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock

import pytest

from keyboard_agent.keyboard_driver import KeyboardDriver
from keyboard_agent.models import ObservationMode


@pytest.fixture
def mock_tmux(monkeypatch):
    tmux = MagicMock()
    tmux.snapshot.side_effect = lambda **kwargs: f"{kwargs.get('format', 'plain')}-snap"
    monkeypatch.setattr(
        "keyboard_agent.keyboard_driver.KeyboardDriver._bind",
        lambda self: setattr(self, "_tmux", tmux) or None,
    )
    return tmux


def test_observe_settles_once_after_action(mock_tmux, monkeypatch, tmp_path: Path):
    sleeps: list[float] = []
    monkeypatch.setattr("keyboard_agent.keyboard_driver.time.sleep", lambda s: sleeps.append(s))

    driver = KeyboardDriver(
        "sess",
        tmp_path / "turns",
        observation_mode=ObservationMode.SEMANTIC,
        snapshot_settle_sec=1.0,
    )
    driver.observe(0, run_dir=tmp_path)
    assert sleeps == []

    driver.observe(1, run_dir=tmp_path)
    assert mock_tmux.snapshot.call_count == 6
    assert sleeps == [1.0]


def test_observe_no_settle_when_disabled(mock_tmux, monkeypatch, tmp_path: Path):
    sleeps: list[float] = []
    monkeypatch.setattr("keyboard_agent.keyboard_driver.time.sleep", lambda s: sleeps.append(s))

    driver = KeyboardDriver(
        "sess",
        tmp_path / "turns",
        observation_mode=ObservationMode.PLAIN,
        snapshot_settle_sec=0.0,
    )
    driver.observe(1, run_dir=tmp_path)

    assert mock_tmux.snapshot.call_count == 3
    assert sleeps == []
