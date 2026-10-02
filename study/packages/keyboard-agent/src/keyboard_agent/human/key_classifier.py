from __future__ import annotations

from ..actions import VALID_KEYS

# US QWERTY: Shift+digit and shifted punctuation (directly typable on Mac/Windows).
US_SHIFT_SYMBOLS: tuple[str, ...] = (
    "!",
    "@",
    "#",
    "$",
    "%",
    "^",
    "&",
    "*",
    "(",
    ")",
    "_",
    "+",
    "{",
    "}",
    "|",
    ":",
    '"',
    "<",
    ">",
    "?",
    "~",
)


def is_type_key(key: str) -> bool:
    """Single printable ASCII characters count as type (not press)."""
    if len(key) != 1:
        return False
    code = ord(key)
    return 32 <= code <= 126


def is_press_turn_key(key: str) -> bool:
    """Special keys from the agent press set (excluding type keys)."""
    if is_type_key(key):
        return False
    normalized = key.lower() if len(key) == 1 and key.isalpha() else key
    return normalized in VALID_KEYS or key.startswith(("ctrl+", "alt+"))


def keystrokes_to_type_text(keystrokes: list[dict]) -> str:
    parts: list[str] = []
    for ks in keystrokes:
        key = ks["key"]
        if len(key) == 1 and is_type_key(key):
            parts.append(key)
    return "".join(parts)
