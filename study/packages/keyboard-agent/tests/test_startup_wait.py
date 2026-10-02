from pathlib import Path
from unittest.mock import MagicMock

import pytest

from keyboard_agent.models import ScreenObservation
from keyboard_agent.startup_wait import (
    StartupNotReadyError,
    StartupWaitConfig,
    wait_for_tui_ready,
)

APT_OUTPUT = "Reading package lists... Done\nGet:1 http://deb.debian.org/debian wget\n"
APTUI_OUTPUT = "Package List\n" + ("adduser installed\n" * 5)


def test_wait_rejects_bootstrap_then_accepts_tui():
    driver = MagicMock()
    driver.peek_plain.side_effect = [APT_OUTPUT, APT_OUTPUT, APTUI_OUTPUT, APTUI_OUTPUT]
    final_obs = ScreenObservation(
        turn=0,
        mode="semantic",
        plain_text=APTUI_OUTPUT,
        text=APTUI_OUTPUT,
        svg_file="T01/turns/turn_000.svg",
    )
    driver.observe.return_value = final_obs

    cfg = StartupWaitConfig(timeout_sec=5, stable_sec=0.01, poll_interval_sec=0.01)
    obs = wait_for_tui_ready(
        driver,
        run_dir=Path("/tmp/run"),
        config=cfg,
        container_alive=lambda: True,
    )
    assert obs is final_obs
    driver.observe.assert_called_once()


def test_wait_timeout_on_stuck_bootstrap():
    driver = MagicMock()
    driver.peek_plain.return_value = APT_OUTPUT

    cfg = StartupWaitConfig(timeout_sec=0.05, stable_sec=0.01, poll_interval_sec=0.01)
    with pytest.raises(StartupNotReadyError, match="Timed out"):
        wait_for_tui_ready(
            driver,
            run_dir=Path("/tmp/run"),
            config=cfg,
            container_alive=lambda: True,
        )


def test_wait_fails_if_container_dies():
    driver = MagicMock()
    driver.peek_plain.return_value = APT_OUTPUT

    cfg = StartupWaitConfig(timeout_sec=1, stable_sec=0.01, poll_interval_sec=0.01)
    with pytest.raises(StartupNotReadyError, match="Container exited"):
        wait_for_tui_ready(
            driver,
            run_dir=Path("/tmp/run"),
            config=cfg,
            container_alive=lambda: False,
        )
