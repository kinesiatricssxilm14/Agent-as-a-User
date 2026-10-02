from __future__ import annotations

import json
import os
import re
from typing import Iterable

from oracle import agent_tui
from oracle.result import CheckResult

SEMANTIC_TAG_RE = re.compile(r"</?(fg|bg):([^>]+)>")

# Common ANSI / 256-color tokens that look like terminal defaults on dark themes.
_DARK_TERMINAL_NEUTRAL_FG = frozenset(
    {
        "white",
        "light_white",
        "gray",
        "light_gray",
        "color_7",
        "color_15",
        "color_252",
        "color_254",
        "color_255",
        "color_231",
    }
)
_DARK_TERMINAL_NEUTRAL_BG = frozenset(
    {
        "black",
        "color_0",
        "color_16",
        "color_232",
        "color_233",
        "color_234",
    }
)
_LIGHT_TERMINAL_NEUTRAL_FG = frozenset({"black", "color_0", "color_16", "color_232"})
_LIGHT_TERMINAL_NEUTRAL_BG = frozenset(
    {"white", "light_white", "color_7", "color_15", "color_255", "color_231"}
)


def _normalize(text: str) -> str:
    return re.sub(r"\s+", " ", text.strip().lower())


def _normalize_color(color: str | None) -> str | None:
    if color is None:
        return None
    return color.strip().lower().replace("-", "_")


def _neutral_fg_colors() -> frozenset[str]:
    theme = os.environ.get("ORACLE_TERMINAL_THEME", "dark").strip().lower()
    if theme == "light":
        return _LIGHT_TERMINAL_NEUTRAL_FG
    return _DARK_TERMINAL_NEUTRAL_FG


def _neutral_bg_colors() -> frozenset[str]:
    theme = os.environ.get("ORACLE_TERMINAL_THEME", "dark").strip().lower()
    if theme == "light":
        return _LIGHT_TERMINAL_NEUTRAL_BG
    return _DARK_TERMINAL_NEUTRAL_BG


def infer_visual_baseline(spans: list[dict[str, str | None]]) -> dict[str, object]:
    """Infer the screen's normal fg/bg so explicit default-looking colors can be ignored."""
    total_chars = sum(len(span["text"]) for span in spans) or 1
    implicit_chars = sum(
        len(span["text"])
        for span in spans
        if span["fg"] is None and span["bg"] is None
    )
    implicit_ratio = implicit_chars / total_chars

    fg_counts: dict[str, int] = {}
    bg_counts: dict[str, int] = {}
    for span in spans:
        count = len(span["text"])
        if span["fg"]:
            fg_counts[span["fg"]] = fg_counts.get(span["fg"], 0) + count
        if span["bg"]:
            bg_counts[span["bg"]] = bg_counts.get(span["bg"], 0) + count

    modal_fg = max(fg_counts, key=fg_counts.get) if fg_counts else None
    modal_bg = max(bg_counts, key=bg_counts.get) if bg_counts else None

    # When much of the screen is unstyled, treat terminal defaults as baseline=None.
    use_implicit_baseline = implicit_ratio >= 0.15

    return {
        "implicit_ratio": implicit_ratio,
        "baseline_fg": None if use_implicit_baseline else modal_fg,
        "baseline_bg": None if use_implicit_baseline else modal_bg,
        "modal_fg": modal_fg,
        "modal_bg": modal_bg,
    }


def is_distinct_fg(color: str, baseline: dict[str, object]) -> bool:
    """True when fg is visually different from the screen's normal body text."""
    norm = _normalize_color(color)
    if not norm:
        return False
    ref = baseline.get("baseline_fg")
    if ref is None:
        return norm not in {_normalize_color(c) for c in _neutral_fg_colors()}
    return not _color_matches(color, str(ref))


def is_distinct_bg(color: str, baseline: dict[str, object]) -> bool:
    """True when bg is visually different from the screen's normal background."""
    norm = _normalize_color(color)
    if not norm:
        return False
    ref = baseline.get("baseline_bg")
    if ref is None:
        return norm not in {_normalize_color(c) for c in _neutral_bg_colors()}
    return not _color_matches(color, str(ref))


def _distinct_fg_values(values: Iterable[str], baseline: dict[str, object]) -> list[str]:
    return [value for value in values if is_distinct_fg(value, baseline)]


def _distinct_bg_values(values: Iterable[str], baseline: dict[str, object]) -> list[str]:
    return [value for value in values if is_distinct_bg(value, baseline)]


def _color_matches(actual: str | None, expected: str | None) -> bool:
    if expected is None:
        return True
    if actual is None:
        return False
    return _normalize_color(actual) == _normalize_color(expected)


def parse_semantic_spans(semantic: str) -> list[dict[str, str | None]]:
    """Split agent-tui semantic snapshot into styled text spans."""
    spans: list[dict[str, str | None]] = []
    current_fg: str | None = None
    current_bg: str | None = None
    fg_stack: list[str | None] = []
    bg_stack: list[str | None] = []
    pos = 0

    for match in SEMANTIC_TAG_RE.finditer(semantic):
        if match.start() > pos:
            text = semantic[pos : match.start()]
            if text:
                spans.append({"text": text, "fg": current_fg, "bg": current_bg})
        tag = match.group(0)
        kind = match.group(1)
        color = match.group(2)
        if tag.startswith("</"):
            if kind == "fg":
                current_fg = fg_stack.pop() if fg_stack else None
            else:
                current_bg = bg_stack.pop() if bg_stack else None
        elif kind == "fg":
            fg_stack.append(current_fg)
            current_fg = color
        else:
            bg_stack.append(current_bg)
            current_bg = color
        pos = match.end()

    if pos < len(semantic):
        text = semantic[pos:]
        if text:
            spans.append({"text": text, "fg": current_fg, "bg": current_bg})
    return spans


def semantic_plain_text(semantic: str) -> str:
    return "".join(span["text"] for span in parse_semantic_spans(semantic))


def plain_text_from_spans(spans: list[dict[str, str | None]]) -> str:
    return "".join(span["text"] for span in spans)


def parse_styled_snapshot(raw: str) -> tuple[str, list[dict[str, str | None]]]:
    """Return (plain_text, styled_spans). Semantic markup is for colors only."""
    spans = parse_semantic_spans(raw)
    return plain_text_from_spans(spans), spans


def screen_plain_text(text: str) -> str:
    """Strip semantic color/style tags for text-only screen checks."""
    if SEMANTIC_TAG_RE.search(text):
        return semantic_plain_text(text)
    return text


def _char_styles(spans: list[dict[str, str | None]]) -> list[dict[str, str | None]]:
    styles: list[dict[str, str | None]] = []
    for span in spans:
        style = {"fg": span["fg"], "bg": span["bg"]}
        styles.extend(style for _ in span["text"])
    return styles


def find_text_styles(
    spans: list[dict[str, str | None]],
    text: str,
    *,
    plain: str | None = None,
    case_insensitive: bool = True,
) -> list[dict[str, str | None]]:
    """Return fg/bg for each plain-text occurrence; semantic tags are not matched."""
    plain = plain if plain is not None else plain_text_from_spans(spans)
    flags = re.IGNORECASE if case_insensitive else 0
    styles = _char_styles(spans)
    hits: list[dict[str, str | None]] = []
    for match in re.finditer(re.escape(text), plain, flags):
        chunk = styles[match.start() : match.end()]
        if not chunk:
            continue
        fg_values = {s["fg"] for s in chunk if s["fg"]}
        bg_values = {s["bg"] for s in chunk if s["bg"]}
        hits.append(
            {
                "fg": next(iter(fg_values)) if len(fg_values) == 1 else None,
                "bg": next(iter(bg_values)) if len(bg_values) == 1 else None,
                "fg_values": sorted(fg_values),
                "bg_values": sorted(bg_values),
            }
        )
    return hits


def _style_matches(
    style: dict,
    *,
    fg: str | None,
    bg: str | None,
    colored: bool,
    highlighted: bool,
    baseline: dict[str, object],
) -> bool:
    chunk_fgs = style.get("fg_values") or []
    chunk_bgs = style.get("bg_values") or []
    distinct_fgs = _distinct_fg_values(chunk_fgs, baseline)
    distinct_bgs = _distinct_bg_values(chunk_bgs, baseline)
    has_colored = bool(distinct_fgs)
    has_highlighted = bool(distinct_bgs)
    has_any_style = has_colored or has_highlighted

    if fg is not None and not any(_color_matches(value, fg) for value in chunk_fgs):
        return False
    if bg is not None and not any(_color_matches(value, bg) for value in chunk_bgs):
        return False

    if colored and highlighted:
        if not has_any_style:
            return False
    elif colored and not has_colored:
        return False
    elif highlighted and not has_highlighted:
        return False

    return True


def _describe_style_want(
    *,
    fg: str | None,
    bg: str | None,
    colored: bool,
    highlighted: bool,
) -> str:
    parts: list[str] = []
    if fg:
        parts.append(f"fg={fg}")
    if bg:
        parts.append(f"bg={bg}")
    if colored and highlighted:
        parts.append("visually distinct fg or bg (not terminal default)")
    elif colored:
        parts.append("visually distinct fg (not terminal default)")
    elif highlighted:
        parts.append("visually distinct bg (not terminal default)")
    return ", ".join(parts) if parts else "plain text"


def text_is_highlighted(
    text: str,
    *,
    raw: str | None = None,
    fg: str | None = None,
    bg: str | None = None,
    colored: bool = False,
    highlighted: bool = False,
    case_insensitive: bool = True,
    spans: list[dict[str, str | None]] | None = None,
    plain: str | None = None,
) -> tuple[bool, str]:
    """Match ``text`` on plain screen content; use semantic spans only for fg/bg."""
    if spans is None or plain is None:
        if raw is None:
            raise ValueError("text_is_highlighted requires raw snapshot or spans+plain")
        plain, spans = parse_styled_snapshot(raw)
    flags = re.IGNORECASE if case_insensitive else 0
    if not re.search(re.escape(text), plain, flags):
        return False, f"text not found on screen: {text!r}"

    style_requested = fg is not None or bg is not None or colored or highlighted
    if not style_requested:
        return True, f"text found: {text!r}"

    baseline = infer_visual_baseline(spans)

    for style in find_text_styles(spans, text, plain=plain, case_insensitive=case_insensitive):
        if _style_matches(
            style,
            fg=fg,
            bg=bg,
            colored=colored,
            highlighted=highlighted,
            baseline=baseline,
        ):
            chunk_fgs = style.get("fg_values") or []
            chunk_bgs = style.get("bg_values") or []
            return True, f"text {text!r} styled fg={chunk_fgs or None} bg={chunk_bgs or None}"

    want = _describe_style_want(fg=fg, bg=bg, colored=colored, highlighted=highlighted)
    return False, f"text {text!r} found but not styled as expected ({want})"


def screen_has_color(
    *,
    fg: str | None = None,
    bg: str | None = None,
    min_chars: int = 1,
    raw: str | None = None,
    spans: list[dict[str, str | None]] | None = None,
) -> tuple[bool, str]:
    """Check span colors; only plain span text is used for previews."""
    if spans is None:
        if raw is None:
            raise ValueError("screen_has_color requires raw snapshot or spans")
        _, spans = parse_styled_snapshot(raw)
    if not fg and not bg:
        return False, "screen-color check needs --fg and/or --bg"

    for span in spans:
        text = span["text"]
        if len(text.strip()) < min_chars:
            continue
        fg_ok = fg is None or _color_matches(span["fg"], fg)
        bg_ok = bg is None or _color_matches(span["bg"], bg)
        if fg_ok and bg_ok and ((fg and span["fg"]) or (bg and span["bg"])):
            preview = text.strip()[:40]
            return True, f"found fg={span['fg']} bg={span['bg']} on {preview!r}"
    want = []
    if fg:
        want.append(f"fg={fg}")
    if bg:
        want.append(f"bg={bg}")
    return False, f"no span with {' '.join(want)}"


def match_patterns(text: str, patterns: Iterable[str], *, regex: bool = False) -> list[str]:
    matched: list[str] = []
    for pat in patterns:
        if regex:
            if re.search(pat, text, re.MULTILINE | re.IGNORECASE):
                matched.append(pat)
        elif pat in text or _normalize(pat) in _normalize(text):
            matched.append(pat)
    return matched


def run_screen_absent_check(
    check_id: str,
    *,
    patterns: list[str],
    session: str | None = None,
    fmt: str = "plain",
    regex: bool = False,
    snapshot_path: str | None = None,
    require_all: bool = True,
) -> CheckResult:
    """Inverse of run_screen_check: pass when pattern(s) are NOT on screen."""
    try:
        if snapshot_path:
            from pathlib import Path

            raw = Path(snapshot_path).read_text(encoding="utf-8", errors="replace")
        else:
            raw = agent_tui.snapshot(session=session, fmt=fmt)
    except agent_tui.AgentTuiError as exc:
        return CheckResult(
            check_id=check_id,
            check_type="screen_absent",
            passed=False,
            message=f"screen capture failed: {exc}",
        )

    text = screen_plain_text(raw)
    text_hits = match_patterns(text, patterns, regex=regex)
    if require_all:
        passed = len(text_hits) == 0
        if passed:
            msg = f"screen does not contain forbidden pattern(s): {patterns}"
        else:
            msg = f"screen still shows forbidden pattern(s): {text_hits}"
    else:
        passed = len(text_hits) < len(patterns)
        absent = [p for p in patterns if p not in text_hits]
        if passed:
            msg = f"screen absent pattern(s): {absent}"
        else:
            msg = f"screen contains all forbidden pattern(s): {text_hits}"

    preview = text[:800] + ("..." if len(text) > 800 else "")
    return CheckResult(
        check_id=check_id,
        check_type="screen_absent",
        passed=passed,
        message=msg,
        details={
            "forbidden_patterns": patterns,
            "found_patterns": text_hits,
            "absent_patterns": [p for p in patterns if p not in text_hits],
            "preview": preview,
        },
    )


def run_screen_check(
    check_id: str,
    *,
    patterns: list[str],
    session: str | None = None,
    fmt: str = "plain",
    regex: bool = False,
    colors: list[str] | None = None,
    snapshot_path: str | None = None,
    require_all: bool = True,
) -> CheckResult:
    try:
        if snapshot_path:
            from pathlib import Path

            raw = Path(snapshot_path).read_text(encoding="utf-8", errors="replace")
        else:
            raw = agent_tui.snapshot(session=session, fmt=fmt)
    except agent_tui.AgentTuiError as exc:
        return CheckResult(
            check_id=check_id,
            check_type="screen",
            passed=False,
            message=f"screen capture failed: {exc}",
        )

    text = screen_plain_text(raw)
    text_hits = match_patterns(text, patterns, regex=regex)
    del colors  # color checks belong to check_screen_highlight / check_screen_color

    patterns_ok = (
        len(text_hits) == len(patterns)
        if require_all
        else len(text_hits) >= 1
    )
    passed = patterns_ok

    if passed:
        msg = f"screen contains required fingerprint(s): {text_hits}"
    else:
        missing = [p for p in patterns if p not in text_hits]
        msg = f"screen missing fingerprint(s): {missing}"

    preview = text[:800] + ("..." if len(text) > 800 else "")
    return CheckResult(
        check_id=check_id,
        check_type="screen",
        passed=passed,
        message=msg,
        details={
            "matched_patterns": text_hits,
            "missing_patterns": [p for p in patterns if p not in text_hits],
            "preview": preview,
        },
    )


def _load_semantic_snapshot(
    *,
    session: str | None = None,
    snapshot_path: str | None = None,
) -> tuple[str, list[dict[str, str | None]]]:
    """Load semantic snapshot and split into plain text + styled spans."""
    if snapshot_path:
        from pathlib import Path

        raw = Path(snapshot_path).read_text(encoding="utf-8", errors="replace")
    elif session:
        raw = agent_tui.snapshot(session=session, fmt="semantic")
    else:
        raise ValueError("screen highlight/color checks require session or snapshot")
    return parse_styled_snapshot(raw)


def run_screen_highlight_check(
    check_id: str,
    *,
    text: str,
    session: str | None = None,
    fg: str | None = None,
    bg: str | None = None,
    colored: bool = False,
    highlighted: bool = False,
    snapshot_path: str | None = None,
) -> CheckResult:
    try:
        plain, spans = _load_semantic_snapshot(session=session, snapshot_path=snapshot_path)
    except (agent_tui.AgentTuiError, ValueError) as exc:
        return CheckResult(
            check_id=check_id,
            check_type="screen_highlight",
            passed=False,
            message=f"screen capture failed: {exc}",
        )

    baseline = infer_visual_baseline(spans)
    passed, message = text_is_highlighted(
        text,
        fg=fg,
        bg=bg,
        colored=colored,
        highlighted=highlighted,
        spans=spans,
        plain=plain,
    )
    preview = plain[:800] + ("..." if len(plain) > 800 else "")
    return CheckResult(
        check_id=check_id,
        check_type="screen_highlight",
        passed=passed,
        message=message,
        details={
            "text": text,
            "fg": fg,
            "bg": bg,
            "colored": colored,
            "highlighted": highlighted,
            "visual_baseline": baseline,
            "styles": find_text_styles(spans, text, plain=plain),
            "preview": preview,
        },
    )


def run_screen_color_check(
    check_id: str,
    *,
    session: str | None = None,
    fg: str | None = None,
    bg: str | None = None,
    snapshot_path: str | None = None,
) -> CheckResult:
    try:
        plain, spans = _load_semantic_snapshot(session=session, snapshot_path=snapshot_path)
    except (agent_tui.AgentTuiError, ValueError) as exc:
        return CheckResult(
            check_id=check_id,
            check_type="screen_color",
            passed=False,
            message=f"screen capture failed: {exc}",
        )

    passed, message = screen_has_color(fg=fg, bg=bg, spans=spans)
    preview = plain[:800] + ("..." if len(plain) > 800 else "")
    return CheckResult(
        check_id=check_id,
        check_type="screen_color",
        passed=passed,
        message=message,
        details={
            "fg": fg,
            "bg": bg,
            "preview": preview,
        },
    )


def run_screen_json_check(
    check_id: str,
    *,
    patterns: list[str],
    session: str | None = None,
) -> CheckResult:
    raw = agent_tui.snapshot(session=session, fmt="json")
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return run_screen_check(check_id, patterns=patterns, session=session, fmt="plain")
    lines = data.get("lines") or data.get("content") or []
    text = "\n".join(lines) if isinstance(lines, list) else str(lines)
    matched = match_patterns(text, patterns)
    return CheckResult(
        check_id=check_id,
        check_type="screen",
        passed=bool(matched),
        message="json snapshot pattern check",
        details={
            "matched": matched,
            "line_count": len(lines) if isinstance(lines, list) else 0,
        },
    )
