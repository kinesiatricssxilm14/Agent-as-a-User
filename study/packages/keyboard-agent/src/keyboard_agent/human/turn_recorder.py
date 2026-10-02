from __future__ import annotations

import threading
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Callable, Literal

from ..keyboard_driver import KeyboardDriver
from ..models import ScreenObservation
from .key_classifier import is_press_turn_key, is_type_key, keystrokes_to_type_text


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


OperationKind = Literal["type", "press"]


@dataclass
class KeystrokeRecord:
    seq: int
    pressed_at: str
    interval_ms: float | None
    key: str
    raw_bytes: str

    def to_dict(self) -> dict:
        return {
            "seq": self.seq,
            "pressed_at": self.pressed_at,
            "interval_ms": self.interval_ms,
            "key": self.key,
            "raw_bytes": self.raw_bytes,
        }


@dataclass
class HumanOperation:
    turn: int
    kind: OperationKind
    started_at: str
    ended_at: str
    duration_ms: float
    gap_before_ms: float | None
    keystrokes: list[KeystrokeRecord] = field(default_factory=list)
    observation: ScreenObservation | None = None

    def to_action_dict(self) -> dict:
        ks = [k.to_dict() for k in self.keystrokes]
        if self.kind == "press":
            key = self.keystrokes[0].key if self.keystrokes else ""
            return {
                "action": "press",
                "key": key,
                "human_keystrokes": ks,
                "gap_before_ms": self.gap_before_ms,
            }
        text = keystrokes_to_type_text(ks)
        return {
            "action": "type",
            "text": text,
            "human_keystrokes": ks,
            "gap_before_ms": self.gap_before_ms,
        }


class HumanTurnRecorder:
    """
    Turn rules for human input:
    - type keys (letters/digits/simple symbols): group into one turn; gap > type_idle_ms starts a new turn
    - press keys (arrows, ctrl+*, enter, etc.): each key is its own turn
    - submit keys are consumed by the harness and never forwarded to the TUI
    """

    def __init__(
        self,
        driver: KeyboardDriver,
        *,
        run_dir,
        type_idle_ms: float = 1000.0,
        submit_key: str = "ctrl+g ctrl+g ctrl+g",
        on_keystroke: Callable[[KeystrokeRecord], None] | None = None,
        on_keystroke_snapshot: Callable[[KeystrokeRecord, ScreenObservation | None], None]
        | None = None,
        on_operation: Callable[[HumanOperation], None] | None = None,
        on_operation_complete: Callable[[HumanOperation], None] | None = None,
        log: Callable[[str], None] | None = None,
    ) -> None:
        self.driver = driver
        self.run_dir = run_dir
        self.type_idle_ms = type_idle_ms
        self.submit_sequence = tuple(_parse_submit_sequence(submit_key))
        self.submit_key = " ".join(self.submit_sequence)
        self.on_keystroke = on_keystroke
        self.on_keystroke_snapshot = on_keystroke_snapshot
        self.on_operation = on_operation
        self.on_operation_complete = on_operation_complete
        self._log = log or (lambda _msg: None)

        self._seq = 0
        self._last_key_mono: float | None = None
        self._type_buffer: list[KeystrokeRecord] = []
        self._type_started_at: str | None = None
        self._type_started_mono: float | None = None
        self._turn = 0
        self._operations: list[HumanOperation] = []
        self._submitted = False
        self._submit_progress = 0
        self._idle_timer: threading.Timer | None = None
        self._snapshot_threads: list[threading.Thread] = []
        self._lock = threading.Lock()

    @property
    def operations(self) -> list[HumanOperation]:
        return list(self._operations)

    @property
    def submitted(self) -> bool:
        return self._submitted

    @property
    def keystroke_count(self) -> int:
        return self._seq

    def flush(self) -> None:
        with self._lock:
            self._cancel_idle_timer()
            if self._type_buffer:
                self._finalize_type_turn(gap_before_ms=None)

    def join_snapshots(self, timeout: float = 60.0) -> None:
        for thread in self._snapshot_threads:
            thread.join(timeout=timeout)

    def handle_key(
        self,
        key_name: str,
        *,
        raw_bytes: bytes | None = None,
    ) -> bool:
        """Record one key. Returns True when submit key ends the session."""
        submitted, consumed = self.handle_submit_candidate(
            key_name,
            raw_bytes=raw_bytes,
        )
        if consumed:
            return submitted

        key = _normalize_key(key_name)
        self._record_keystroke(key, raw_bytes=raw_bytes)
        return False

    def _record_keystroke(
        self,
        key: str,
        *,
        raw_bytes: bytes | None = None,
    ) -> KeystrokeRecord:
        now_mono = time.perf_counter()
        interval_ms: float | None = None
        if self._last_key_mono is not None:
            interval_ms = (now_mono - self._last_key_mono) * 1000.0

        self._seq += 1
        record = KeystrokeRecord(
            seq=self._seq,
            pressed_at=_utc_now(),
            interval_ms=interval_ms,
            key=key,
            raw_bytes=(raw_bytes or b"").hex(),
        )
        self._last_key_mono = now_mono

        if self.on_keystroke:
            self.on_keystroke(record)

        with self._lock:
            if is_type_key(key):
                if (
                    self._type_buffer
                    and interval_ms is not None
                    and interval_ms > self.type_idle_ms
                ):
                    self._finalize_type_turn(gap_before_ms=interval_ms)
                if not self._type_buffer:
                    self._type_started_at = record.pressed_at
                    self._type_started_mono = now_mono
                self._type_buffer.append(record)
                self._arm_type_idle_timer()
            elif is_press_turn_key(key):
                self._cancel_idle_timer()
                if self._type_buffer:
                    self._finalize_type_turn(gap_before_ms=None)
                self._finalize_press_turn(record)
            else:
                self._cancel_idle_timer()
                if self._type_buffer:
                    self._finalize_type_turn(gap_before_ms=None)
                self._finalize_press_turn(record)

        self._schedule_keystroke_snapshot(record)
        return record

    def handle_submit_candidate(
        self,
        key_name: str,
        *,
        raw_bytes: bytes | None = None,
    ) -> tuple[bool, bool]:
        """
        Check whether a key advances the submit sequence.

        Returns (submitted, consumed). Consumed submit-prefix keys should not be
        forwarded to the TUI, even before the full sequence is complete.
        """
        key = _normalize_key(key_name)
        if not self.submit_sequence:
            return False, False

        expected = self.submit_sequence[self._submit_progress]
        if key != expected:
            self._submit_progress = 0
            if key == self.submit_sequence[0]:
                self._record_keystroke(key, raw_bytes=raw_bytes)
                self._submit_progress = 1
                return False, True
            return False, False

        self._record_keystroke(key, raw_bytes=raw_bytes)
        self._submit_progress += 1
        if self._submit_progress < len(self.submit_sequence):
            return False, True

        self._submit_progress = 0
        self.submit()
        return True, True

    def submit(self) -> None:
        """End the session like an agent submit action."""
        with self._lock:
            self._cancel_idle_timer()
            if self._type_buffer:
                self._finalize_type_turn(gap_before_ms=None)
        self._submitted = True

    def _arm_type_idle_timer(self) -> None:
        self._cancel_idle_timer()
        self._idle_timer = threading.Timer(
            self.type_idle_ms / 1000.0,
            self._on_type_idle,
        )
        self._idle_timer.daemon = True
        self._idle_timer.start()

    def _cancel_idle_timer(self) -> None:
        if self._idle_timer is not None:
            self._idle_timer.cancel()
            self._idle_timer = None

    def _on_type_idle(self) -> None:
        with self._lock:
            if not self._type_buffer:
                return
            gap = self.type_idle_ms
            if self._last_key_mono is not None:
                gap = max(
                    (time.perf_counter() - self._last_key_mono) * 1000.0,
                    self.type_idle_ms,
                )
            self._finalize_type_turn(gap_before_ms=gap)

    def _finalize_type_turn(self, *, gap_before_ms: float | None) -> None:
        if not self._type_buffer:
            return
        self._turn += 1
        started_at = self._type_started_at or self._type_buffer[0].pressed_at
        ended_at = _utc_now()
        duration_ms = 0.0
        if self._type_started_mono is not None and self._last_key_mono is not None:
            duration_ms = (self._last_key_mono - self._type_started_mono) * 1000.0
        op = HumanOperation(
            turn=self._turn,
            kind="type",
            started_at=started_at,
            ended_at=ended_at,
            duration_ms=duration_ms,
            gap_before_ms=gap_before_ms,
            keystrokes=list(self._type_buffer),
        )
        self._type_buffer = []
        self._type_started_at = None
        self._type_started_mono = None
        self._append_operation(op)

    def _finalize_press_turn(self, record: KeystrokeRecord) -> None:
        self._turn += 1
        op = HumanOperation(
            turn=self._turn,
            kind="press",
            started_at=record.pressed_at,
            ended_at=record.pressed_at,
            duration_ms=0.0,
            gap_before_ms=record.interval_ms,
            keystrokes=[record],
        )
        self._append_operation(op)

    def _append_operation(self, op: HumanOperation) -> None:
        self._operations.append(op)
        if self.on_operation:
            self.on_operation(op)

    def _schedule_keystroke_snapshot(self, record: KeystrokeRecord) -> None:
        thread = threading.Thread(
            target=self._capture_keystroke_snapshot,
            args=(record,),
            daemon=True,
        )
        self._snapshot_threads.append(thread)
        thread.start()

    def _capture_keystroke_snapshot(self, record: KeystrokeRecord) -> None:
        obs: ScreenObservation | None = None
        try:
            obs = self.driver.observe(record.seq, run_dir=self.run_dir)
        except Exception as exc:
            self._log(f"  snapshot keystroke {record.seq} failed: {exc}")
        if self.on_keystroke_snapshot:
            self.on_keystroke_snapshot(record, obs)

def _normalize_key(key_name: str) -> str:
    if key_name.startswith("alt+"):
        suffix = key_name[4:]
        if len(suffix) == 1 and suffix.isalpha():
            return f"alt+{suffix.lower()}"
        return key_name
    if len(key_name) == 1 and key_name.isalpha():
        return key_name  # Shift+letter → uppercase; plain letter → lowercase
    return key_name.lower()


def _parse_submit_sequence(raw: str) -> list[str]:
    parts = [part.strip() for part in raw.replace(",", " ").split() if part.strip()]
    if not parts:
        return ["ctrl+g", "ctrl+g", "ctrl+g"]
    return [_normalize_key(part) for part in parts]
