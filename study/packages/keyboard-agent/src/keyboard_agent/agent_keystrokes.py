"""Expand LLM agent actions into human-compatible keystroke rows."""

from __future__ import annotations

from typing import Any


def text_to_keys(text: str) -> list[str]:
    """Map typed text to the same key names human recording uses."""
    keys: list[str] = []
    for ch in text:
        if ch == "\n":
            keys.append("enter")
        elif ch == "\t":
            keys.append("tab")
        elif ch in ("\x7f", "\b"):
            keys.append("backspace")
        elif ch == "\x1b":
            keys.append("escape")
        else:
            keys.append(ch)
    return keys


def expand_action_to_keystrokes(
    action: dict[str, Any] | None,
    *,
    seq_start: int,
    pressed_at: str,
    interval_ms: float | None = None,
) -> list[dict[str, Any]]:
    """
    Produce keystrokes.jsonl rows for a successful TUI action.

    - press → one keystroke
    - type  → one keystroke per character (\\n → enter, …)
    - wait / submit / errors → no keystrokes (nothing sent to the TUI)
    """
    if not action:
        return []
    kind = action.get("action")
    rows: list[dict[str, Any]] = []
    seq = seq_start

    if kind == "press":
        key = action.get("key")
        if not key:
            return []
        rows.append(
            {
                "seq": seq,
                "pressed_at": pressed_at,
                "interval_ms": interval_ms,
                "key": key,
                "raw_bytes": "",
            }
        )
        return rows

    if kind == "type":
        text = action.get("text")
        if text is None:
            return []
        for i, key in enumerate(text_to_keys(str(text))):
            rows.append(
                {
                    "seq": seq + i,
                    "pressed_at": pressed_at,
                    "interval_ms": interval_ms if i == 0 else 0.0,
                    "key": key,
                    "raw_bytes": "",
                }
            )
        return rows

    return []
