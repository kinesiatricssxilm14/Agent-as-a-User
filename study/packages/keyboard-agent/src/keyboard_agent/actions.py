from __future__ import annotations

import json
import re
from typing import Any

from .models import AgentAction

# Mirror agent-tui/src/agent_tui/tmux_backend.py VALID_KEYS
# Unshifted US keyboard punctuation only (exclude Shift keys like ? ! @ # …)
PRESS_PUNCTUATION_KEYS: tuple[str, ...] = (
    "/", ".", ",", "-", "=", "[", "]", "\\", ";", "'", "`",
)

VALID_KEYS: tuple[str, ...] = (
    *[chr(i) for i in range(ord("a"), ord("z") + 1)],
    *[chr(i) for i in range(ord("0"), ord("9") + 1)],
    *PRESS_PUNCTUATION_KEYS,
    *[f"ctrl+{chr(i)}" for i in range(ord("a"), ord("z") + 1)],
    *[f"alt+{chr(i)}" for i in range(ord("a"), ord("z") + 1)],
    "alt+\\",
    "alt+|",
    "arrow_up",
    "arrow_down",
    "arrow_left",
    "arrow_right",
    "page_up",
    "page_down",
    "home",
    "end",
    "enter",
    "tab",
    "space",
    "escape",
    "backspace",
    "delete",
    *[f"f{i}" for i in range(1, 11)],
)

DEFAULT_WAIT_MS = 3000

ACTION_JSON_SCHEMA = {
    "press": {"required": ["key"], "forbidden": ["text", "ms", "pattern"]},
    "type": {"required": ["text"], "forbidden": ["key", "ms", "pattern"]},
    "wait": {"required": [], "forbidden": ["key", "text"], "optional": ["ms", "pattern"]},
    "submit": {"required": [], "forbidden": ["key", "text", "ms", "pattern"]},
}


def valid_keys_text() -> str:
    return ", ".join(VALID_KEYS)


def format_valid_keys_for_prompt() -> str:
    """Full explicit key list for the system prompt (must match VALID_KEYS)."""
    letters = ", ".join(chr(i) for i in range(ord("a"), ord("z") + 1))
    digits = ", ".join(chr(i) for i in range(ord("0"), ord("9") + 1))
    ctrl = ", ".join(f"ctrl+{chr(i)}" for i in range(ord("a"), ord("z") + 1))
    alt = ", ".join(
        [*(f"alt+{chr(i)}" for i in range(ord("a"), ord("z") + 1)), "alt+\\", "alt+|"]
    )
    arrows = "arrow_up, arrow_down, arrow_left, arrow_right"
    nav = "page_up, page_down, home, end"
    edit = "enter, tab, space, escape, backspace, delete"
    fn = ", ".join(f"f{i}" for i in range(1, 11))
    punct = ", ".join(PRESS_PUNCTUATION_KEYS)
    complete = valid_keys_text()
    return "\n".join([
        "Complete list (copy key strings exactly):",
        complete,
        "",
        "Grouped reference:",
        f"  Letters: {letters}",
        f"  Digits: {digits}",
        f"  Punctuation (unshifted): {punct}",
        f"  Ctrl: {ctrl}",
        f"  Alt: {alt}",
        f"  Arrows: {arrows}",
        f"  Navigation: {nav}",
        f"  Editing: {edit}",
        f"  Function: {fn}",
    ])


def parse_agent_json(raw: str) -> AgentAction:
    """Parse one JSON object from LLM output (strip markdown fences if present)."""
    text = raw.strip()
    fence = re.search(r"```(?:json)?\s*([\s\S]*?)```", text)
    if fence:
        text = fence.group(1).strip()
    if not text.startswith("{"):
        match = re.search(r"\{[\s\S]*\}", text)
        if match:
            text = match.group(0)
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as exc:
        raise ValueError(f"Agent must return a single JSON object: {exc}") from exc
    return validate_action_payload(payload)


def validate_action_payload(payload: Any) -> AgentAction:
    if not isinstance(payload, dict):
        raise ValueError("Action must be a JSON object")

    action = payload.get("action")
    if action not in ACTION_JSON_SCHEMA:
        raise ValueError(f"Unknown action {action!r}. Must be one of: press, type, wait, submit")

    spec = ACTION_JSON_SCHEMA[action]
    for key in spec["required"]:
        if payload.get(key) in (None, ""):
            raise ValueError(f"Action {action!r} requires field {key!r}")

    for key in spec["forbidden"]:
        if key in payload and payload[key] not in (None, ""):
            raise ValueError(f"Action {action!r} must not include field {key!r}")

    extra = set(payload) - {"action", "key", "text", "ms", "pattern"}
    if extra:
        raise ValueError(f"Unexpected fields: {', '.join(sorted(extra))}")

    if action == "press":
        raw_key = str(payload["key"])
        key = raw_key.lower() if len(raw_key) == 1 and raw_key.isalpha() else raw_key
        if key not in VALID_KEYS:
            raise ValueError(
                f"Invalid key {key!r}. Valid press keys: {valid_keys_text()}"
            )
        return AgentAction(action="press", key=key)

    if action == "type":
        return AgentAction(action="type", text=str(payload["text"]))

    if action == "wait":
        ms = payload.get("ms", DEFAULT_WAIT_MS)
        if not isinstance(ms, int) or ms <= 0:
            raise ValueError("wait.ms must be a positive integer (milliseconds)")
        pattern = payload.get("pattern")
        return AgentAction(action="wait", ms=ms, pattern=str(pattern) if pattern else None)

    return AgentAction(action="submit")
