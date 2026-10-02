from __future__ import annotations

import sys
import threading
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterator


class HarnessLogger:
    """Suite harness logging: operator messages vs file-only diagnostics."""

    def __init__(self, path: Path) -> None:
        self.path = path.expanduser().resolve()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._quiet_depth = 0
        self._quiet_lock = threading.Lock()

    def _write(self, msg: str) -> None:
        stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        with self.path.open("a", encoding="utf-8") as handle:
            handle.write(f"[{stamp}] {msg}\n")

    def _is_quiet(self) -> bool:
        with self._quiet_lock:
            return self._quiet_depth > 0

    def user(self, msg: str) -> None:
        """Operator-facing message (stderr when not in task-quiet mode)."""
        self._write(msg)
        if not self._is_quiet():
            print(msg, file=sys.stderr)

    def diag(self, msg: str) -> None:
        """Diagnostics only — never pollute the operator terminal."""
        self._write(msg)

    @contextmanager
    def attach_quiet(self) -> Iterator[None]:
        """Suppress stderr while the operator is doing a task (re-entrant)."""
        with self._quiet_lock:
            self._quiet_depth += 1
        try:
            yield
        finally:
            with self._quiet_lock:
                self._quiet_depth -= 1
