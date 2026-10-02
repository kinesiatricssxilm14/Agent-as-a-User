#!/usr/bin/env python3
"""Build deterministic rerun manifests for infrastructure-affected attempts."""

from __future__ import annotations

import csv
from collections import defaultdict
from pathlib import Path


HERE = Path(__file__).resolve().parent
KEY = ("app", "judge_model", "observation", "repeat", "project", "task_id")
DETAIL_FIELDS = (
    "rerun_id",
    *KEY,
    "reason",
    "tri_modal_scope",
    "passed_original",
    "stop_reason_original",
    "error_turns",
    "artifact_relpath",
)
CASE_FIELDS = (
    "app",
    "judge_model",
    "observation",
    "project",
    "task_id",
    "reasons",
    "n_attempts",
    "repeats",
    "original_passes",
    "error_turns",
)


def read_csv(name):
    with (HERE / name).open(newline="") as f:
        return list(csv.DictReader(f))


def attempt_key(row):
    return tuple(row[field] for field in KEY)


transport = read_csv("api_transport_audit.csv")
context = [
    row
    for row in read_csv("agent_error_audit.csv")
    if row["terminal_cause"] == "context_overflow_400"
]

attempts = {}
for row in transport:
    attempts[attempt_key(row)] = {
        **{field: row[field] for field in KEY},
        "reason": "transport_error",
        "tri_modal_scope": row["tri_modal_scope"],
        "passed_original": row["passed"],
        "stop_reason_original": row["stop_reason"],
        "error_turns": row["transport_error_turns"],
        "artifact_relpath": row["artifact_relpath"],
    }
for row in context:
    key = attempt_key(row)
    if key in attempts:
        attempts[key]["reason"] += "+context_overflow_400"
        attempts[key]["error_turns"] = str(
            int(attempts[key]["error_turns"]) + int(row["terminal_error_count"])
        )
    else:
        attempts[key] = {
            **{field: row[field] for field in KEY},
            "reason": "context_overflow_400",
            "tri_modal_scope": row["tri_modal_scope"],
            "passed_original": "False",
            "stop_reason_original": "agent_error_limit",
            "error_turns": row["terminal_error_count"],
            "artifact_relpath": row["artifact_relpath"],
        }


def write_manifests(stem, rows):
    rows = sorted(
        rows,
        key=lambda row: (
            row["app"],
            row["judge_model"],
            row["observation"],
            row["repeat"],
            row["project"],
            row["task_id"],
        ),
    )
    detailed = [{"rerun_id": i, **row} for i, row in enumerate(rows, 1)]
    with (HERE / f"rerun_required_{stem}.csv").open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=DETAIL_FIELDS)
        writer.writeheader()
        writer.writerows(detailed)

    grouped = defaultdict(list)
    fields = ("app", "judge_model", "observation", "project", "task_id")
    for row in rows:
        grouped[tuple(row[field] for field in fields)].append(row)
    cases = []
    for key, group in sorted(grouped.items()):
        cases.append(
            {
                **dict(zip(fields, key)),
                "reasons": ";".join(sorted({row["reason"] for row in group})),
                "n_attempts": len(group),
                "repeats": ";".join(sorted(row["repeat"] for row in group)),
                "original_passes": sum(row["passed_original"].lower() == "true" for row in group),
                "error_turns": sum(int(row["error_turns"]) for row in group),
            }
        )
    with (HERE / f"rerun_cases_{stem}.csv").open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=CASE_FIELDS)
        writer.writeheader()
        writer.writerows(cases)
    return len(detailed), len(cases)


all_count = write_manifests("all_conditions", list(attempts.values()))
tri_count = write_manifests(
    "tri_modal", [row for row in attempts.values() if row["tri_modal_scope"] == "1"]
)
print(f"all conditions: {all_count[0]} attempts in {all_count[1]} condition-cases")
print(f"tri-modal: {tri_count[0]} attempts in {tri_count[1]} condition-cases")
