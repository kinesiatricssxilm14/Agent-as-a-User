"""Shared startup phase labels for terminal logs and web UI."""

from __future__ import annotations

from dataclasses import dataclass, field
import threading


PHASE_CONNECTING = "connecting"
PHASE_STOPPING = "stopping"
PHASE_STARTING = "starting"
PHASE_WAITING_TUI = "waiting_tui"
PHASE_READY = "ready"
PHASE_ACTIVE = "active"
PHASE_SUBMITTED = "submitted"


@dataclass
class StartupStatus:
    phase: str = PHASE_CONNECTING
    message: str = "⏳ English-only text…"
    task_id: str = ""
    description: str = ""
    _lock: threading.Lock = field(default_factory=threading.Lock, repr=False)

    def set(
        self,
        phase: str,
        message: str,
        *,
        task_id: str | None = None,
        description: str | None = None,
    ) -> None:
        with self._lock:
            self.phase = phase
            self.message = message
            if task_id is not None:
                self.task_id = task_id
            if description is not None:
                self.description = description

    def snapshot(self) -> dict[str, str | bool]:
        with self._lock:
            return {
                "phase": self.phase,
                "message": self.message,
                "task_id": self.task_id,
                "description": self.description,
                "ready": self.phase in (PHASE_READY, PHASE_ACTIVE),
                "active": self.phase == PHASE_ACTIVE,
            }


def waiting_message(reason: str) -> str:
    return f"⏳ English-only text TUI English-only text（{reason}）…"


def ready_message() -> str:
    return "✅ English-only text！English-only text。"
