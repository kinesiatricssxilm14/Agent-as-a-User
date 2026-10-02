#!/usr/bin/env python3
"""Build tasks.csv and suite tables from this merged pack. Read-only on artifacts."""
from __future__ import annotations

import csv
import json
import math
import re
from collections import Counter, defaultdict
from datetime import datetime
from pathlib import Path
from statistics import median

ROOT = Path(__file__).resolve().parent
NAV = {
    "arrow_up", "arrow_down", "arrow_left", "arrow_right", "tab", "shift_tab",
    "page_up", "page_down", "home", "end", "up", "down", "left", "right",
}
HELP = {"?", "f1", "question", "shift+/"}
ESC = {"escape", "esc"}
EDIT = {"backspace", "delete", "back_space"}
CONFIRM = {"enter", "return"}
HELP_RE = re.compile(r"(press\s*\?|\bhelp\b|\bf1\b|keybind|\bshortcuts?\b)", re.I)
KEYCHIP_RE = re.compile(
    r"\b(Enter|Esc|Escape|Tab|Shift|Ctrl|PgUp|PgDn|F1|Space|Backspace|Delete|↑|↓|←|→)\b"
)


def parse_dt(s: str | None):
    if not s:
        return None
    s = s.replace("Z", "+00:00")
    try:
        return datetime.fromisoformat(s)
    except ValueError:
        return None


def key_bucket(k: str) -> str:
    kl = k.lower()
    if kl in NAV:
        return "nav"
    if kl in HELP or k == "?":
        return "help"
    if kl in ESC:
        return "escape"
    if kl in EDIT:
        return "edit"
    if kl in CONFIRM:
        return "enter"
    if k == "/":
        return "search"
    if kl == "q":
        return "quit"
    if len(k) == 1 and k.isalpha():
        return "letter"
    if len(k) == 1 and k.isdigit():
        return "digit"
    return "other"


def parse_keys(path: Path) -> dict:
    out = {
        "n_keys": 0, "n_unique_keys": 0, "keys_nav": 0, "keys_help": 0,
        "keys_escape": 0, "keys_edit": 0, "keys_enter": 0, "keys_search": 0,
        "keys_quit": 0, "keys_letter": 0, "keys_digit": 0, "keys_other": 0,
        "keys_thrash": 0, "used_help": 0, "used_escape": 0, "used_nav": 0,
    }
    if not path.is_file():
        return out
    buckets = Counter()
    uniq = set()
    n = 0
    prev = None
    run = 0
    thrash = 0
    for line in path.read_text(errors="replace").splitlines():
        if not line.strip():
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        k = str(obj.get("key") or "")
        n += 1
        uniq.add(k)
        b = key_bucket(k)
        buckets[b] += 1
        if k == prev:
            run += 1
            if run >= 3:
                thrash += 1
        else:
            run = 1
            prev = k
    out["n_keys"] = n
    out["n_unique_keys"] = len(uniq)
    for b in ("nav", "help", "escape", "edit", "enter", "search", "quit", "letter", "digit", "other"):
        out[f"keys_{b}"] = buckets[b]
    out["keys_thrash"] = thrash
    out["used_help"] = int(buckets["help"] > 0)
    out["used_escape"] = int(buckets["escape"] > 0)
    out["used_nav"] = int(buckets["nav"] > 0)
    return out


def scan_transcript(path: Path) -> dict:
    out = {"n_transcript_turns": 0, "noop_turns": 0, "changed_turns": 0,
           "action_fail_turns": 0, "dead_turns": 0}
    if not path.is_file():
        return out
    with path.open(errors="replace") as f:
        for line in f:
            if '"turn"' not in line:
                continue
            out["n_transcript_turns"] += 1
            if '"has_changed": false' in line:
                out["noop_turns"] += 1
            elif '"has_changed": true' in line:
                out["changed_turns"] += 1
            if '"action_ok": false' in line:
                out["action_fail_turns"] += 1
            if '"container_alive": false' in line:
                out["dead_turns"] += 1
    return out


def screen_metrics(plain: str) -> dict:
    lines = plain.splitlines()
    footer = "\n".join(lines[-4:]) if lines else ""
    return {
        "screen0_lines": len(lines),
        "screen0_chars": len(plain),
        "screen0_help_hint": int(bool(HELP_RE.search(plain))),
        "screen0_keychips": len(KEYCHIP_RE.findall(plain)),
        "screen0_box": sum(plain.count(c) for c in "─│┌┐└┘╭╮╰╯┏┓┗┛━┃"),
        "screen0_emoji": len(re.findall(r"[\U0001F300-\U0001FAFF]", plain)),
        "screen0_footer_chars": len(footer),
    }


def load_result_summary(path: Path) -> dict:
    empty = {
        "passed": None, "stop_reason": "", "turn_count": 0,
        "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0,
        "reasoning_tokens": 0, "cached_prompt_tokens": 0,
        "verify_line": "", "verify_type": "", "started_at": "", "ended_at": "",
        "duration_s": "",
    }
    if not path.is_file():
        return empty
    data = json.loads(path.read_text())
    tu = data.get("total_token_usage") or {}
    v = data.get("verify") if isinstance(data.get("verify"), dict) else {}
    line = str(v.get("line") or "")
    passed = bool(data.get("passed"))
    if line:
        passed = "PASS" in line
    ll = line.lower()
    if "(screen)" in ll:
        vtype = "screen"
    elif "(shell)" in ll:
        vtype = "shell"
    elif "(filesystem)" in ll:
        vtype = "filesystem"
    else:
        vtype = "other"
    a = parse_dt(data.get("started_at"))
    b = parse_dt(data.get("ended_at"))
    dur = (b - a).total_seconds() if a and b else None
    return {
        "passed": passed,
        "stop_reason": data.get("stop_reason") or "",
        "turn_count": int(data.get("turn_count") or 0),
        "prompt_tokens": int(tu.get("prompt_tokens") or 0),
        "completion_tokens": int(tu.get("completion_tokens") or 0),
        "total_tokens": int(tu.get("total_tokens") or 0),
        "reasoning_tokens": int(tu.get("reasoning_tokens") or tu.get("thinking_tokens") or 0),
        "cached_prompt_tokens": int(tu.get("cached_prompt_tokens") or 0),
        "verify_line": line,
        "verify_type": vtype,
        "started_at": data.get("started_at") or "",
        "ended_at": data.get("ended_at") or "",
        "duration_s": round(dur, 3) if dur is not None else "",
    }


def fail_kind(passed: bool | None, stop: str) -> str:
    if passed:
        return "pass"
    if stop == "agent_submit":
        return "wrong_submit"
    return stop or "unknown"


def mean(xs):
    return sum(xs) / len(xs) if xs else 0.0


def qtile(xs, q):
    if not xs:
        return 0.0
    s = sorted(xs)
    i = (len(s) - 1) * q
    lo, hi = int(math.floor(i)), int(math.ceil(i))
    if lo == hi:
        return s[lo]
    return s[lo] * (hi - i) + s[hi] * (i - lo)


def summarize(xs):
    if not xs:
        return {"n": 0, "mean": "", "median": "", "p25": "", "p75": ""}
    return {
        "n": len(xs),
        "mean": round(mean(xs), 2),
        "median": round(median(xs), 2),
        "p25": round(qtile(xs, 0.25), 2),
        "p75": round(qtile(xs, 0.75), 2),
    }


def main() -> None:
    # Rerun tree uses claude-code-opus-5; this pack (like 08-19) stores the same
    # suites as claude-code-opus-5. Alias so replaced_by_rerun stays aligned.
    app_alias = {"claude-code-opus-5": "claude-code-opus-5"}
    replaced_keys = set()
    rerun_root = Path("/path/to/anonymous-artifact")
    for result in rerun_root.glob("*/*/*/*/*/*/result.json"):
        app, judge, obs, repeat, project, task = result.parent.relative_to(rerun_root).parts
        app = app_alias.get(app, app)
        replaced_keys.add((app, judge, obs, repeat, project, task))

    rows = []
    for family in ("oracle", "generated"):
        fam_root = ROOT / family
        if not fam_root.is_dir():
            continue
        for result in fam_root.glob("*/*/*/*/suite/*/*/result.json"):
            rel = result.parent.relative_to(fam_root)
            # {app}/{judge}/{obs}/{repeat}/suite/{project}/{task}
            app, judge, obs, repeat, _suite, project, task = rel.parts
            if ".attempt" in task:
                continue
            rec = load_result_summary(result)
            live = result.parent / "live.json"
            if rec["duration_s"] == "" and live.is_file():
                try:
                    lj = json.loads(live.read_text())
                except json.JSONDecodeError:
                    lj = {}
                a = parse_dt(lj.get("started_at"))
                b = parse_dt(lj.get("updated_at") or lj.get("ended_at"))
                if a and b:
                    rec["duration_s"] = round((b - a).total_seconds(), 3)
            ks = parse_keys(result.parent / "keystrokes.jsonl")
            tr = scan_transcript(result.parent / "transcript.jsonl")
            sc = {}
            isp = result.parent / "initial_screen.json"
            if isp.is_file():
                try:
                    raw = json.loads(isp.read_text())
                    sc = screen_metrics(raw.get("plain_text") or "")
                except json.JSONDecodeError:
                    sc = {}
            n_svg = len(list((result.parent / "turns").glob("*.svg"))) if (result.parent / "turns").is_dir() else 0
            replaced = (app, judge, obs, repeat, project, task) in replaced_keys
            tc = rec["turn_count"] or 0
            row = {
                "family": family,
                "app": app,
                "judge_model": judge,
                "observation": obs,
                "repeat": repeat,
                "project": project,
                "task_id": task,
                "replaced_by_rerun": int(replaced),
                "artifact_relpath": str(result.parent.relative_to(ROOT)),
                **rec,
                "fail_kind": fail_kind(rec["passed"], rec["stop_reason"]),
                **ks,
                **tr,
                **sc,
                "n_svgs": n_svg,
                "prompt_per_turn": round(rec["prompt_tokens"] / tc, 3) if tc else "",
                "reasoning_per_turn": round(rec["reasoning_tokens"] / tc, 3) if tc else "",
                "keys_per_turn": round(ks["n_keys"] / tc, 3) if tc else "",
                "noop_rate": round(tr["noop_turns"] / tr["n_transcript_turns"], 4) if tr["n_transcript_turns"] else "",
                "nav_frac": round(ks["keys_nav"] / ks["n_keys"], 4) if ks["n_keys"] else "",
            }
            rows.append(row)

    rows.sort(key=lambda r: (r["family"], r["app"], r["judge_model"], r["observation"], r["repeat"], r["project"], r["task_id"]))
    fieldnames = list(rows[0].keys()) if rows else []
    tables = ROOT / "tables"
    tables.mkdir(exist_ok=True)
    with (ROOT / "tasks.csv").open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fieldnames)
        w.writeheader()
        w.writerows(rows)
    with (tables / "tasks.csv").open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fieldnames)
        w.writeheader()
        w.writerows(rows)

    # suite overview
    groups = defaultdict(list)
    for r in rows:
        groups[(r["family"], r["app"], r["judge_model"], r["observation"], r["repeat"])].append(r)
    suite_rows = []
    for g, items in sorted(groups.items()):
        family, app, judge, obs, repeat = g
        p1 = sum(1 for x in items if x["passed"])
        by = defaultdict(list)
        for x in items:
            by[(x["project"], x["task_id"])].append(x["passed"])
        # this group is one repeat, Pass@1 == p1
        suite_rows.append({
            "family": family,
            "app": app,
            "judge_model": judge,
            "observation": obs,
            "repeat": repeat,
            "n": len(items),
            "n_passed": p1,
            "pass_rate": round(p1 / len(items), 4) if items else 0,
            "n_replaced": sum(x["replaced_by_rerun"] for x in items),
            "wrong_submit": sum(1 for x in items if x["fail_kind"] == "wrong_submit"),
            "timeout": sum(1 for x in items if x["fail_kind"] == "max_turns"),
            "session_lost": sum(1 for x in items if x["fail_kind"] == "session_lost"),
            "turns_median": summarize([x["turn_count"] for x in items])["median"],
            "reasoning_median": summarize([x["reasoning_tokens"] for x in items])["median"],
            "keys_median": summarize([x["n_keys"] for x in items])["median"],
            "duration_s_median": summarize([float(x["duration_s"]) for x in items if x["duration_s"] != ""])["median"],
            "noop_rate_mean": round(mean([float(x["noop_rate"]) for x in items if x["noop_rate"] != ""]), 4) if any(x["noop_rate"] != "" for x in items) else "",
        })
    with (tables / "suite_overview.csv").open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(suite_rows[0].keys()))
        w.writeheader()
        w.writerows(suite_rows)

    # condition-level (pool 3 repeats)
    cond = defaultdict(list)
    for r in rows:
        cond[(r["family"], r["app"], r["judge_model"], r["observation"])].append(r)
    cond_rows = []
    for g, items in sorted(cond.items()):
        family, app, judge, obs = g
        by = defaultdict(list)
        for x in items:
            by[(x["project"], x["task_id"])].append(bool(x["passed"]))
        p3 = sum(1 for v in by.values() if any(v))
        always = sum(1 for v in by.values() if sum(v) == 3)
        never = sum(1 for v in by.values() if sum(v) == 0)
        p1 = sum(1 for x in items if x["passed"])
        cond_rows.append({
            "family": family, "app": app, "judge_model": judge, "observation": obs,
            "n_attempts": len(items), "n_tasks": len(by),
            "pass_at_1": p1, "pass_at_1_rate": round(p1 / len(items), 4),
            "pass_at_3": p3, "pass_at_3_rate": round(p3 / len(by), 4) if by else 0,
            "always_3of3": always, "never_0of3": never,
            "n_replaced": sum(x["replaced_by_rerun"] for x in items),
            "wrong_submit": sum(1 for x in items if x["fail_kind"] == "wrong_submit"),
            "timeout": sum(1 for x in items if x["fail_kind"] == "max_turns"),
            "session_lost": sum(1 for x in items if x["fail_kind"] == "session_lost"),
            "reasoning_median": summarize([x["reasoning_tokens"] for x in items])["median"],
            "keys_median": summarize([x["n_keys"] for x in items])["median"],
            "turns_median": summarize([x["turn_count"] for x in items])["median"],
        })
    with (tables / "condition_overview.csv").open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(cond_rows[0].keys()))
        w.writeheader()
        w.writerows(cond_rows)

    n_pass = sum(1 for r in rows if r["passed"])
    n_rep = sum(r["replaced_by_rerun"] for r in rows)
    print(json.dumps({
        "n_rows": len(rows),
        "n_passed": n_pass,
        "n_replaced": n_rep,
        "n_suites": len(suite_rows),
        "n_conditions": len(cond_rows),
        "fail_kind": dict(Counter(r["fail_kind"] for r in rows)),
        "verify_type": dict(Counter(r["verify_type"] for r in rows)),
    }, indent=2))


if __name__ == "__main__":
    main()
