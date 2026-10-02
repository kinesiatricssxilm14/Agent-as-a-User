from __future__ import annotations

"""Map browser KeyboardEvent payloads to agent-tui key names."""

_BROWSER_KEY_MAP: dict[str, str] = {
    "ArrowUp": "arrow_up",
    "ArrowDown": "arrow_down",
    "ArrowLeft": "arrow_left",
    "ArrowRight": "arrow_right",
    "PageUp": "page_up",
    "PageDown": "page_down",
    "Home": "home",
    "End": "end",
    "Enter": "enter",
    "Tab": "tab",
    " ": "space",
    "Escape": "escape",
    "Backspace": "backspace",
    "Delete": "delete",
    "F1": "f1",
    "F2": "f2",
    "F3": "f3",
    "F4": "f4",
    "F5": "f5",
    "F6": "f6",
    "F7": "f7",
    "F8": "f8",
    "F9": "f9",
    "F10": "f10",
    "F11": "f11",
}


def browser_key_to_agent(payload: dict) -> tuple[str, str | None]:
    """
    Convert a browser key event JSON object to (agent_key, type_char).
    Returns type_char when the key should be sent via type_text (single char).
    """
    key = str(payload.get("key", ""))
    code = str(payload.get("code", ""))
    if key in _BROWSER_KEY_MAP:
        return _BROWSER_KEY_MAP[key], None
    if payload.get("altKey"):
        if key == "\\":
            return "alt+\\", None
        if key == "|":
            return "alt+|", None
        if len(key) == 1 and key.isalpha():
            return f"alt+{key.lower()}", None
    if len(key) == 1:
        return key, key
    if code.startswith("Key") and len(code) == 4:
        letter = code[3].lower()
        return letter, letter
    if code.startswith("Digit") and len(code) == 6:
        digit = code[5]
        return digit, digit
    if payload.get("ctrlKey") and len(key) == 1 and "a" <= key.lower() <= "z":
        return f"ctrl+{key.lower()}", None
    return key, None
