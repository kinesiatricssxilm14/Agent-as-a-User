from __future__ import annotations

from dataclasses import dataclass, field

from .key_classifier import is_type_key


@dataclass
class KeyEvent:
    """One decoded key from terminal input."""

    key_name: str
    raw_bytes: bytes
    char: str | None = None
    is_printable: bool = False


@dataclass
class KeyParseState:
    buffer: bytearray = field(default_factory=bytearray)

    def feed(self, data: bytes) -> list[KeyEvent]:
        self.buffer.extend(data)
        events: list[KeyEvent] = []
        while self.buffer:
            event, consumed = _parse_one(bytes(self.buffer))
            if event is None:
                break
            events.append(event)
            del self.buffer[:consumed]
        return events


_CTRL_NAMES = {i + 1: f"ctrl+{chr(ord('a') + i)}" for i in range(26)}

# Bytes 28–31 are common ctrl combos not covered by ctrl+a..ctrl+z.
_EXTRA_CTRL_NAMES: dict[int, str] = {
    28: "ctrl+\\",
    29: "ctrl+]",
    30: "ctrl+^",
    31: "ctrl+_",
}

_ESCAPE_SEQUENCES: dict[bytes, str] = {
    b"\x1b[A": "arrow_up",
    b"\x1b[B": "arrow_down",
    b"\x1b[C": "arrow_right",
    b"\x1b[D": "arrow_left",
    b"\x1bOA": "arrow_up",
    b"\x1bOB": "arrow_down",
    b"\x1bOC": "arrow_right",
    b"\x1bOD": "arrow_left",
    b"\x1b[H": "home",
    b"\x1b[F": "end",
    b"\x1bOH": "home",
    b"\x1bOF": "end",
    b"\x1b[5~": "page_up",
    b"\x1b[6~": "page_down",
    b"\x1b[3~": "delete",
    b"\x1b[11~": "f1",
    b"\x1b[12~": "f2",
    b"\x1b[13~": "f3",
    b"\x1b[14~": "f4",
    b"\x1b[15~": "f5",
    b"\x1b[17~": "f6",
    b"\x1b[18~": "f7",
    b"\x1b[19~": "f8",
    b"\x1b[20~": "f9",
    b"\x1b[21~": "f10",
    b"\x1b[23~": "f11",
    b"\x1b[24~": "f12",
    b"\x1bOP": "f1",
    b"\x1bOQ": "f2",
    b"\x1bOR": "f3",
    b"\x1bOS": "f4",
    b"\x1b[[A": "f1",
    b"\x1b[[B": "f2",
    b"\x1b[[C": "f3",
    b"\x1b[[D": "f4",
    b"\x1b[[E": "f5",
}

_SINGLE_BYTE_KEYS: dict[int, str] = {
    9: "tab",
    10: "enter",
    13: "enter",
    27: "escape",
    32: "space",
    127: "backspace",
}


def _parse_one(buf: bytes) -> tuple[KeyEvent | None, int]:
    if not buf:
        return None, 0

    if buf[0:1] == b"\x1b":
        for length in range(min(len(buf), 8), 1, -1):
            prefix = buf[:length]
            if prefix in _ESCAPE_SEQUENCES:
                name = _ESCAPE_SEQUENCES[prefix]
                return KeyEvent(key_name=name, raw_bytes=prefix), length
        if len(buf) >= 2 and buf[1:2] == b"O":
            if len(buf) < 3:
                return None, 0
            fn_map = {b"P": "f1", b"Q": "f2", b"R": "f3", b"S": "f4"}
            fn = fn_map.get(buf[2:3])
            if fn:
                raw = buf[:3]
                return KeyEvent(key_name=fn, raw_bytes=raw), 3
        if len(buf) < 2:
            return None, 0
        if buf[1:2] == b"[":
            if len(buf) < 4:
                return None, 0
            if buf[2:3].isdigit():
                end = 3
                while end < len(buf) and buf[end:end + 1].isdigit():
                    end += 1
                if end >= len(buf) or buf[end:end + 1] != b"~":
                    return None, 0
                seq = buf[: end + 1]
                fn_map = {
                    b"\x1b[11~": "f1",
                    b"\x1b[12~": "f2",
                    b"\x1b[13~": "f3",
                    b"\x1b[14~": "f4",
                    b"\x1b[15~": "f5",
                    b"\x1b[17~": "f6",
                    b"\x1b[18~": "f7",
                    b"\x1b[19~": "f8",
                    b"\x1b[20~": "f9",
                    b"\x1b[21~": "f10",
                    b"\x1b[23~": "f11",
                    b"\x1b[24~": "f12",
                }
                name = fn_map.get(seq)
                if name:
                    return KeyEvent(key_name=name, raw_bytes=seq), len(seq)
            return None, 0
        if buf[1:2] not in (b"[", b"O") and 32 <= buf[1] <= 126:
            ch = chr(buf[1])
            if ch.isalpha():
                name = f"alt+{ch.lower()}"
            elif ch == "\\":
                name = "alt+\\"
            elif ch == "|":
                name = "alt+|"
            else:
                name = f"alt+{ch}"
            return KeyEvent(key_name=name, raw_bytes=buf[:2]), 2
        return KeyEvent(key_name="escape", raw_bytes=b"\x1b"), 1

    value = buf[0]
    if value in _SINGLE_BYTE_KEYS:
        name = _SINGLE_BYTE_KEYS[value]
        return KeyEvent(key_name=name, raw_bytes=buf[:1]), 1
    if value in _CTRL_NAMES:
        return KeyEvent(key_name=_CTRL_NAMES[value], raw_bytes=buf[:1]), 1
    if value in _EXTRA_CTRL_NAMES:
        return KeyEvent(key_name=_EXTRA_CTRL_NAMES[value], raw_bytes=buf[:1]), 1
    if 32 <= value <= 126:
        ch = chr(value)
        return KeyEvent(
            key_name=ch,
            raw_bytes=buf[:1],
            char=ch,
            is_printable=True,
        ), 1
    return KeyEvent(key_name=f"byte_{value}", raw_bytes=buf[:1]), 1


def key_name_to_raw_bytes(key: str) -> bytes | None:
    """Best-effort raw byte for a parsed key name."""
    if len(key) == 1:
        code = ord(key)
        if 32 <= code <= 126:
            return bytes([code])
    lowered = key.lower()
    if lowered.startswith("ctrl+") and len(lowered) == 6 and lowered[5].isalpha():
        byte = ord(lowered[5]) - ord("a") + 1
        if 1 <= byte <= 26:
            return bytes([byte])
    if lowered.startswith("alt+"):
        suffix = key[4:]
        if len(suffix) == 1:
            return b"\x1b" + suffix.encode("latin-1")
    return None


def keystrokes_to_actions(keystrokes: list[dict]) -> list[dict]:
    """Convert a burst of keystrokes into agent-compatible press/type actions."""
    actions: list[dict] = []
    text_buf = ""

    def flush_text() -> None:
        nonlocal text_buf
        if text_buf:
            actions.append({"action": "type", "text": text_buf})
            text_buf = ""

    for ks in keystrokes:
        key = ks["key"]
        if key in ("enter", "tab"):
            text_buf += "\n" if key == "enter" else "\t"
            continue
        if is_type_key(key):
            text_buf += key
            continue
        flush_text()
        actions.append({"action": "press", "key": key})

    flush_text()
    return actions
