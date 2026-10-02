"""Wait until entrypoint/init finished and the TUI is on screen before the agent loop."""

from __future__ import annotations

import json
import re
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

from agent_tui.readiness import ScreenReadyConfig, is_bootstrap_screen, is_screen_ready

from .keyboard_driver import KeyboardDriver
from .models import ScreenObservation

try:
    from .human.startup_status import ready_message, waiting_message
except ImportError:
    def ready_message() -> str:
        return "✅ Ready — you can start typing."

    def waiting_message(reason: str) -> str:
        return f"⏳ Waiting for TUI ({reason})…"


class StartupNotReadyError(RuntimeError):
    """Container started but TUI never became ready in time."""


@dataclass
class StartupWaitConfig:
    timeout_sec: float = 180.0
    stable_sec: float = 2.0
    poll_interval_sec: float = 0.5
    min_text_len: int = 40
    ready_pattern: str | None = None

    def screen_config(self) -> ScreenReadyConfig:
        return ScreenReadyConfig(
            min_text_len=self.min_text_len,
            ready_pattern=self.ready_pattern,
        )


def load_startup_wait_config(
    project_dir: Path,
    overrides: StartupWaitConfig | None = None,
) -> StartupWaitConfig:
    cfg = overrides or StartupWaitConfig()
    spec_path = project_dir / "bench.spec.json"
    if not spec_path.is_file():
        return cfg
    raw = json.loads(spec_path.read_text(encoding="utf-8"))
    startup = raw.get("startup") or {}
    if not isinstance(startup, dict):
        return cfg
    return StartupWaitConfig(
        timeout_sec=float(startup.get("timeout_sec", cfg.timeout_sec)),
        stable_sec=float(startup.get("stable_sec", cfg.stable_sec)),
        poll_interval_sec=float(startup.get("poll_interval_sec", cfg.poll_interval_sec)),
        min_text_len=int(startup.get("min_text_len", cfg.min_text_len)),
        ready_pattern=startup.get("ready_pattern", cfg.ready_pattern),
    )


def wait_for_tui_ready(
    driver: KeyboardDriver,
    *,
    run_dir: Path,
    config: StartupWaitConfig,
    container_alive: Callable[[], bool],
    log: Callable[[str], None] | None = None,
) -> ScreenObservation:
    """
    Poll the terminal until bootstrap/init output is gone and the screen is stable.

    Returns turn-0 observation (with SVG) only after readiness checks pass.
    """
    emit = log or (lambda _msg: None)
    screen_cfg = config.screen_config()
    deadline = time.monotonic() + config.timeout_sec
    ready_since: float | None = None
    last_reason = "starting"
    last_emit = 0.0
    emit_interval = max(1.0, config.poll_interval_sec)

    while time.monotonic() < deadline:
        if not container_alive():
            raise StartupNotReadyError(
                "Container exited while waiting for TUI startup. "
                "Check entrypoint.sh / seed/init.sh — init may have failed."
            )

        text = driver.peek_plain()
        ready = is_screen_ready(text, config=screen_cfg)

        if ready:
            if config.stable_sec <= 0:
                waited = config.timeout_sec - (deadline - time.monotonic())
                emit(f"  ✅ TUI English-only text（English-only text {config.stable_sec}s，English-only text {waited:.1f}s）")
                return driver.observe(0, run_dir=run_dir)
            if ready_since is None:
                ready_since = time.monotonic()
            elif time.monotonic() - ready_since >= config.stable_sec:
                waited = config.timeout_sec - (deadline - time.monotonic())
                emit(f"  ✅ TUI English-only text（English-only text {config.stable_sec}s，English-only text {waited:.1f}s）")
                return driver.observe(0, run_dir=run_dir)
        else:
            ready_since = None
            if is_bootstrap_screen(text):
                last_reason = "bootstrap/install output on screen"
            elif len(text.strip()) < config.min_text_len:
                last_reason = "screen too empty"
            elif config.ready_pattern and not re.search(
                config.ready_pattern, text, re.I | re.S
            ):
                last_reason = f"ready_pattern not matched: {config.ready_pattern!r}"
            else:
                last_reason = "screen not ready"
            now = time.monotonic()
            if now - last_emit >= emit_interval:
                emit(f"  {waiting_message(last_reason)}")
                last_emit = now

        time.sleep(config.poll_interval_sec)

    snippet = text.strip().replace("\n", "\\n")[:240]
    raise StartupNotReadyError(
        f"Timed out after {config.timeout_sec}s waiting for TUI ready "
        f"(last reason: {last_reason}).\n"
        f"Screen tail: {snippet!r}\n"
        "Hint: entrypoint may still be running seed/init.sh (apt-get, npm, etc.) "
        "before the TUI binary starts. Move heavy setup to Dockerfile build time, "
        "or set bench.spec.json startup.ready_pattern for this project."
    )
