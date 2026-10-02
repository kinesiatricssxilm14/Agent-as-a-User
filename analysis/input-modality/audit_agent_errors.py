#!/usr/bin/env python3
"""Audit agent-error stops and API transport failures across all attempts."""

from __future__ import annotations

import csv
import json
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
AGENT = ROOT / "agent"
TASKS = ROOT / "agent_tasks.csv"
OUT = Path(__file__).resolve().parent
APPS = {
    "original-tui",
    "deepseek-harness-v4-pro",
    "claude-code-opus-5",
    "codex-gpt-5.6-sol",
}
MODELS = {
    "plain": "deepseek-v4-pro",
    "semantic": "deepseek-v4-pro",
    "png": "deepseek-v4-flash-vision-exp",
}


def classify(errors):
    joined = "\n".join(errors).lower()
    if "maximum context length" in joined:
        return "context_overflow_400"
    if "connection error" in joined or "request timed out" in joined:
        return "transport_error"
    if "command '['tmux'" in joined:
        return "tmux_send_error"
    if "invalid key" in joined:
        return "invalid_key_action"
    if "expecting value: line 1 column 1" in joined:
        return "empty_action_after_reasoning"
    return "malformed_action_json"


def in_tri_modal_scope(row):
    return (
        row["app"] in APPS
        and row["observation"] in MODELS
        and row["judge_model"] == MODELS[row["observation"]]
    )


def is_transport_or_service_error(error):
    error = error.lower()
    return (
        "connection error" in error
        or "request timed out" in error
        or "insufficient balance" in error
        or any(f"error code: {code}" in error for code in (402, 429, 500, 502, 503, 504))
    )


with TASKS.open(newline="") as f:
    all_rows = list(csv.DictReader(f))
rows = [r for r in all_rows if r["fail_kind"] == "agent_error_limit"]

audit = []
for row in rows:
    transcript = AGENT / row["artifact_relpath"] / "transcript.jsonl"
    turns = [json.loads(line) for line in transcript.read_text().splitlines() if line.strip()]
    terminal = []
    for turn in reversed(turns):
        if turn.get("action_ok", True):
            break
        terminal.append(turn)
    terminal.reverse()
    terminal_errors = [turn.get("action_error") or "" for turn in terminal]
    all_errors = [turn.get("action_error") or "" for turn in turns if not turn.get("action_ok", True)]
    tri_modal = in_tri_modal_scope(row)
    audit.append(
        {
            "app": row["app"],
            "judge_model": row["judge_model"],
            "observation": row["observation"],
            "repeat": row["repeat"],
            "project": row["project"],
            "task_id": row["task_id"],
            "tri_modal_scope": int(tri_modal),
            "terminal_cause": classify(terminal_errors),
            "terminal_error_count": len(terminal),
            "terminal_reasoning_nonempty": int(
                all(bool((turn.get("reasoning_content") or "").strip()) for turn in terminal)
            ),
            "terminal_raw_action_empty": int(
                all(not bool((turn.get("raw_agent_response") or "").strip()) for turn in terminal)
            ),
            "had_transport_error_earlier": int(
                any(is_transport_or_service_error(error) for error in all_errors)
            ),
            "had_invalid_key_earlier": int(any("invalid key" in error.lower() for error in all_errors)),
            "artifact_relpath": row["artifact_relpath"],
        }
    )

with (OUT / "agent_error_audit.csv").open("w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=list(audit[0]))
    writer.writeheader()
    writer.writerows(audit)

transport_audit = []
for row in all_rows:
    transcript = AGENT / row["artifact_relpath"] / "transcript.jsonl"
    turns = [json.loads(line) for line in transcript.read_text().splitlines() if line.strip()]
    errors = [
        turn.get("action_error") or ""
        for turn in turns
        if is_transport_or_service_error(turn.get("action_error") or "")
    ]
    if not errors:
        continue
    transport_audit.append(
        {
            "app": row["app"],
            "judge_model": row["judge_model"],
            "observation": row["observation"],
            "repeat": row["repeat"],
            "project": row["project"],
            "task_id": row["task_id"],
            "passed": row["passed"],
            "stop_reason": row["stop_reason"],
            "tri_modal_scope": int(in_tri_modal_scope(row)),
            "transport_error_turns": len(errors),
            "connection_errors": sum("connection error" in error.lower() for error in errors),
            "request_timeouts": sum("request timed out" in error.lower() for error in errors),
            "service_http_errors": sum(
                any(f"error code: {code}" in error.lower() for code in (429, 500, 502, 503, 504))
                for error in errors
            ),
            "billing_errors": sum(
                "error code: 402" in error.lower() or "insufficient balance" in error.lower()
                for error in errors
            ),
            "artifact_relpath": row["artifact_relpath"],
        }
    )

transport_fields = [
    "app", "judge_model", "observation", "repeat", "project", "task_id",
    "passed", "stop_reason", "tri_modal_scope", "transport_error_turns",
    "connection_errors", "request_timeouts", "service_http_errors",
    "billing_errors", "artifact_relpath",
]
with (OUT / "api_transport_audit.csv").open("w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=transport_fields)
    writer.writeheader()
    writer.writerows(transport_audit)

tri_transport = [row for row in transport_audit if row["tri_modal_scope"]]
summary = {
    "n_attempts": len(audit),
    "terminal_causes": dict(Counter(row["terminal_cause"] for row in audit)),
    "tri_modal_scope": {
        "n_attempts": sum(row["tri_modal_scope"] for row in audit),
        "terminal_causes": dict(
            Counter(row["terminal_cause"] for row in audit if row["tri_modal_scope"])
        ),
        "by_observation": {
            mode: dict(
                Counter(
                    row["terminal_cause"]
                    for row in audit
                    if row["tri_modal_scope"] and row["observation"] == mode
                )
            )
            for mode in MODELS
        },
    },
    "earlier_recovered_errors": {
        "attempts_with_transport_error": sum(row["had_transport_error_earlier"] for row in audit),
        "attempts_with_invalid_key": sum(row["had_invalid_key_earlier"] for row in audit),
    },
    "transport_or_service_errors_all_attempts": {
        "affected_attempts": len(transport_audit),
        "error_turns": sum(row["transport_error_turns"] for row in transport_audit),
    },
    "transport_or_service_errors_tri_modal_scope": {
        "affected_attempts": len(tri_transport),
        "error_turns": sum(row["transport_error_turns"] for row in tri_transport),
        "by_observation": dict(Counter(row["observation"] for row in tri_transport)),
        "by_outcome": dict(Counter(row["passed"] for row in tri_transport)),
        "by_stop_reason": dict(Counter(row["stop_reason"] for row in tri_transport)),
    },
}
(OUT / "agent_error_audit.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
