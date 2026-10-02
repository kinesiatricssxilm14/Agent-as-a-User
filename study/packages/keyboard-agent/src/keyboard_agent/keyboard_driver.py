from __future__ import annotations

import subprocess
import time
from pathlib import Path
from typing import Any

from .actions import DEFAULT_WAIT_MS
from .models import AgentAction, ObservationMode, ScreenObservation


class KeyboardDriverError(RuntimeError):
    pass


class TmuxSessionGoneError(KeyboardDriverError):
    """tmux pane/session disappeared (e.g. agent pressed quit and TUI exited)."""


def _is_tmux_session_gone(exc: BaseException) -> bool:
    parts = [str(exc)]
    if isinstance(exc, subprocess.CalledProcessError):
        parts.append(exc.stderr or "")
        parts.append(exc.stdout or "")
    msg = " ".join(parts).lower()
    return any(
        token in msg
        for token in (
            "no current target",
            "no server running",
            "can't find pane",
            "can't find session",
            "no such pane",
            "no such session",
        )
    )


class KeyboardDriver:
    """Internal PTY driver — not exposed to the LLM."""

    def __init__(
        self,
        session_id: str,
        turns_dir: Path,
        *,
        observation_mode: ObservationMode = ObservationMode.SEMANTIC,
        snapshot_settle_sec: float = 0.0,
    ) -> None:
        self.session_id = session_id
        self.turns_dir = turns_dir
        self.observation_mode = observation_mode
        self.snapshot_settle_sec = max(0.0, snapshot_settle_sec)
        self.turns_dir.mkdir(parents=True, exist_ok=True)
        self._bind()

    def _bind(self) -> None:
        try:
            from agent_tui import tmux_backend
        except ImportError as exc:
            raise KeyboardDriverError(
                "agent-tui is not installed. Run: pip install -e ../agent-tui"
            ) from exc
        self._tmux = tmux_backend
        self._tmux.use(self.session_id)

    def _snapshot(self, **kwargs: Any) -> Any:
        try:
            return self._tmux.snapshot(**kwargs)
        except Exception as exc:
            if _is_tmux_session_gone(exc):
                raise TmuxSessionGoneError(
                    f"tmux session gone ({self.session_id}): {exc}"
                ) from exc
            raise

    def session_alive(self) -> bool:
        """True if the tmux target still exists (TUI may have quit via q)."""
        try:
            from agent_tui.tmux_backend import tmux_cli_prefix
        except ImportError:
            return False
        proc = subprocess.run(
            [
                *tmux_cli_prefix(),
                "list-panes",
                "-t",
                self.session_id,
                "-F",
                "#{pane_dead}",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            return False
        dead = (proc.stdout or "").strip().splitlines()
        return bool(dead) and dead[0].strip() != "1"

    def peek_plain(self) -> str:
        """Lightweight plain snapshot for startup polling (no SVG)."""
        return str(self._snapshot(format="plain", session_id=self.session_id))

    def execute(self, action: AgentAction) -> dict[str, Any]:
        sid = self.session_id
        if action.action == "press":
            assert action.key is not None
            return self._tmux.press(action.key, session_id=sid)
        if action.action == "type":
            assert action.text is not None
            return self._tmux.type_text(action.text, session_id=sid)
        if action.action == "wait":
            ms = action.ms or DEFAULT_WAIT_MS
            try:
                self._tmux.wait(
                    timeout=ms,
                    text=action.pattern,
                    session_id=sid,
                )
                return {"success": True, "has_changed": True, "error": None}
            except TimeoutError as exc:
                return {"success": False, "has_changed": False, "error": str(exc)}
        if action.action == "submit":
            return {"success": True, "has_changed": False, "error": None}
        raise KeyboardDriverError(f"Unsupported action: {action.action}")

    def observe(self, turn: int, *, run_dir: Path) -> ScreenObservation:
        """Capture screen for LLM (per observation_mode) + SVG trajectory log."""
        sid = self.session_id
        # After an action the LLM may loop faster than the TUI redraws; settle once
        # before capturing (plain/semantic/svg/png are taken back-to-back).
        if turn > 0 and self.snapshot_settle_sec > 0:
            time.sleep(self.snapshot_settle_sec)

        plain = str(self._snapshot(format="plain", session_id=sid))
        semantic = str(self._snapshot(format="semantic", session_id=sid))

        svg_name = f"turn_{turn:03d}.svg"
        svg_path = self.turns_dir / svg_name
        self._snapshot(format="svg", output_path=str(svg_path), session_id=sid)
        svg_rel = str(svg_path.relative_to(run_dir))

        mode = self.observation_mode
        if mode == ObservationMode.PLAIN:
            return ScreenObservation(
                turn=turn,
                mode=mode.value,
                plain_text=plain,
                text=plain,
                svg_file=svg_rel,
            )
        if mode == ObservationMode.SEMANTIC:
            return ScreenObservation(
                turn=turn,
                mode=mode.value,
                plain_text=plain,
                text=semantic,
                svg_file=svg_rel,
            )
        if mode == ObservationMode.PNG:
            png_name = f"turn_{turn:03d}.png"
            png_path = self.turns_dir / png_name
            self._snapshot(format="png", output_path=str(png_path), session_id=sid)
            png_rel = str(png_path.relative_to(run_dir))
            return ScreenObservation(
                turn=turn,
                mode=mode.value,
                plain_text=plain,
                text=f"Turn {turn} — terminal screenshot attached.",
                image_relative=png_rel,
                svg_file=svg_rel,
            )
        if mode == ObservationMode.SVG:
            return ScreenObservation(
                turn=turn,
                mode=mode.value,
                plain_text=plain,
                text=f"Turn {turn} — terminal screenshot attached.",
                image_relative=svg_rel,
                svg_file=svg_rel,
            )
        raise KeyboardDriverError(f"Unknown observation mode: {mode}")
