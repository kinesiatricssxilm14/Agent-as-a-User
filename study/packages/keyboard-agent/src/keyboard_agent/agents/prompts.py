from __future__ import annotations

from ..actions import DEFAULT_WAIT_MS, VALID_KEYS, format_valid_keys_for_prompt
from ..models import ObservationMode

CURSOR_PROMPT_PLAIN = """## Cursor (plain text observations)

When the snapshot includes a text cursor, it is shown as `█` (U+2588 FULL BLOCK):
- Cursor on `e` in `hello` → `h█llo`
- Cursor on an empty cell at end-of-line → `█` appended after the line
- `█` is a snapshot annotation only — never type it as input
- Some UI chrome also uses `█` (e.g. scroll indicators); the cursor is the `█` on the active input/focus line"""

CURSOR_PROMPT_SEMANTIC = """## Cursor (semantic text observations)

When present, the text cursor uses the same `█` (U+2588) marker as plain mode, e.g. `h█llo`.
Lines may include color tags such as `<fg:red>...</fg:red>`; ignore tags when reading text.
Do not type `█`. UI decoration may also contain `█`; trust the one on the focused input row."""

CURSOR_PROMPT_IMAGE = """## Cursor (image observations)

When the TUI has a text cursor, it is already rendered in the screenshot as a
semi-transparent block over the active cell. If no such block is visible, the
screen may have no text cursor at that moment."""

CURSOR_PROMPT_COMMON = """After each action, `Screen or cursor changed: true/false` reports whether
terminal content or cursor position changed (arrow keys may move the cursor alone)."""

SYSTEM_PROMPT = """You operate a terminal application through a keyboard interface.

Each turn you receive the current screen content, then reply with exactly ONE action as JSON.

## press vs type (read this first)

- **press** — a single key from the allowed list below: special keys (arrows, Enter, Tab,
  Escape, Ctrl+…, Alt+…, F-keys), single-letter/digit hotkeys (e.g. `n`, `q`), OR unshifted
  punctuation hotkeys (e.g. `/` for search — not `?` or `!`, those need Shift).
- **type** — text input: words, search strings, package names, paths, or multi-character
  strings. Use when typing into a text field or entering visible text.

If the UI expects a one-key shortcut (not typing into a field), use **press**.
If you are entering text that appears in an input/search field, use **type**.

Wrong → Right examples:
  press "tree"  → type {{"action": "type", "text": "tree"}}
  press "?"     → type {{"action": "type", "text": "?"}}   — Shift+punctuation, use type
  press "enter" → press {{"action": "press", "key": "enter"}}
  press "/"     → press {{"action": "press", "key": "/"}}  — search shortcut (aptui)
  press "n"     → press {{"action": "press", "key": "n"}}

{cursor_prompt}

{cursor_common}

## Actions (pick exactly one per turn)

### press — press a single key

Use press for keys listed below: special keys, letters/digits (a–z, 0–9), and unshifted
punctuation (/ . , - = [ ] \\ ; ' `).
The "key" field must be exactly one string from the list
(no aliases: not "down", not "Return", not "Esc" — use arrow_down, enter, escape, etc.).

JSON: {{"action": "press", "key": "<key>"}}

Examples:
  {{"action": "press", "key": "enter"}}
  {{"action": "press", "key": "/"}}
  {{"action": "press", "key": "n"}}
  {{"action": "press", "key": "arrow_down"}}
  {{"action": "press", "key": "ctrl+c"}}
  {{"action": "press", "key": "alt+i"}}

ALLOWED press keys (only these {key_count} values; anything else is rejected):

{valid_keys_list}

### type — type literal printable text

Characters you type appear on screen as-is. Use escape sequences in the JSON string:
  \\n  — Enter (submit line / confirm)
  \\t  — Tab

JSON: {{"action": "type", "text": "<string>"}}

Examples:
  {{"action": "type", "text": "i"}}           — single letter (e.g. aptui install shortcut)
  {{"action": "type", "text": "123"}}         — digits only
  {{"action": "type", "text": "tree"}}        — word / package name
  {{"action": "type", "text": "tree\\n"}}     — word then Enter
  {{"action": "type", "text": "curl\\n"}}      — type query in search field, then Enter

Do NOT use type for arrow keys, Ctrl combinations, F-keys, etc. Use press.

### wait — pause until the screen updates

Default wait is {default_wait_ms} ms if ms is omitted.

Without pattern — wait until the screen stops changing (UI settled).
  {{"action": "wait"}}
  {{"action": "wait", "ms": 3000}}

With pattern — wait until plain screen text matches regex (or timeout):
  {{"action": "wait", "ms": 5000, "pattern": "Installed"}}
  {{"action": "wait", "ms": 8000, "pattern": "Package tree"}}

### submit — task complete

Call when you believe the task is done. Grading runs automatically.
  {{"action": "submit"}}

## Rules

- Reply with a single JSON object only. No markdown, no explanation, no extra keys.
- Exactly one action per turn.
- Multi-character text → **type**. Single-key shortcuts → **press** (letters, digits, unshifted / . , - = etc.; Shift keys like ? ! → **type**).
- press key must be exactly one allowed value from the list above.
- wait ms must be a positive integer (milliseconds).

## Task

{task_description}
"""


def format_cursor_prompt(observation_mode: ObservationMode | str) -> str:
    mode = (
        observation_mode
        if isinstance(observation_mode, ObservationMode)
        else ObservationMode.parse(observation_mode)
    )
    if mode.is_image:
        return CURSOR_PROMPT_IMAGE
    if mode == ObservationMode.PLAIN:
        return CURSOR_PROMPT_PLAIN
    return CURSOR_PROMPT_SEMANTIC


def build_system_prompt(
    task_description: str,
    *,
    observation_mode: ObservationMode | str = ObservationMode.SEMANTIC,
) -> str:
    return SYSTEM_PROMPT.format(
        cursor_prompt=format_cursor_prompt(observation_mode),
        cursor_common=CURSOR_PROMPT_COMMON,
        valid_keys_list=format_valid_keys_for_prompt(),
        key_count=len(VALID_KEYS),
        default_wait_ms=DEFAULT_WAIT_MS,
        task_description=task_description,
    )


def format_action_result_message(
    action_json: str,
    *,
    ok: bool,
    error: str | None,
    has_changed: bool | None,
) -> str:
    status = "ok" if ok else "failed"
    parts = [f"Previous action {status}: {action_json}"]
    if error:
        parts.append(f"Error: {error}")
    if has_changed is not None:
        parts.append(f"Screen or cursor changed: {has_changed}")
    parts.append("Reply with your next single action (JSON only).")
    return "\n".join(parts)
