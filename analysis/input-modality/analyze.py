#!/usr/bin/env python3
"""Tri-modal comparison for the canonical Agent result bundle.

Uses only the Python standard library. Text conditions use deepseek-v4-pro;
PNG uses deepseek-v4-flash-vision-exp, so PNG contrasts are descriptive and
model-confounded. Plain-vs-semantic contrasts are clean within-model ablations.
"""

from __future__ import annotations

import csv
import json
import math
import random
import statistics
from collections import Counter, defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
AGENT = ROOT / "agent"
TASKS_CSV = ROOT / "agent_tasks.csv"
OUT = Path(__file__).resolve().parent
TEXT_MODEL = "deepseek-v4-pro"
IMAGE_MODEL = "deepseek-v4-flash-vision-exp"
APPS = [
    "original-tui",
    "deepseek-harness-v4-pro",
    "claude-code-opus-5",
    "codex-gpt-5.6-sol",
]
APP_LABELS = {
    "original-tui": "Oracle",
    "deepseek-harness-v4-pro": "Harness",
    "claude-code-opus-5": "Claude Code",
    "codex-gpt-5.6-sol": "Codex",
}
MODES = ["plain", "semantic", "png"]
VISUAL_TERMS = (
    "color",
    "colour",
    "highlight",
    "selected",
    "focused",
    "green",
    "red",
    "yellow",
    "blue",
    "icon",
    "checkmark",
    "bullet",
)


def yes(value: str) -> bool:
    return value.strip().lower() in {"1", "true", "yes"}


def num(value: str):
    if value == "":
        return None
    try:
        return int(value)
    except ValueError:
        try:
            return float(value)
        except ValueError:
            return None


def median(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def mean(values):
    values = [v for v in values if v is not None]
    return statistics.fmean(values) if values else None


def pct(value):
    return round(100 * value, 2) if value is not None else None


def exact_mcnemar(b: int, c: int) -> float:
    n = b + c
    if n == 0:
        return 1.0
    k = min(b, c)
    tail = sum(math.comb(n, i) for i in range(k + 1)) / (2**n)
    return min(1.0, 2 * tail)


def bootstrap_mean_ci(diffs, n_boot=10000, seed=20260825):
    rng = random.Random(seed)
    n = len(diffs)
    samples = []
    for _ in range(n_boot):
        samples.append(sum(diffs[rng.randrange(n)] for _ in range(n)) / n)
    samples.sort()
    return samples[int(0.025 * n_boot)], samples[int(0.975 * n_boot)]


def holm(rows):
    ordered = sorted(enumerate(rows), key=lambda x: x[1]["p_raw"])
    running = 0.0
    adjusted = [1.0] * len(rows)
    m = len(rows)
    for rank, (idx, row) in enumerate(ordered):
        running = max(running, min(1.0, (m - rank) * row["p_raw"]))
        adjusted[idx] = running
    for row, value in zip(rows, adjusted):
        row["p_holm"] = value


def key_bucket(key: str) -> str:
    key = key.lower()
    if key in {"up", "down", "left", "right", "pageup", "pagedown", "home", "end", "j", "k", "h", "l"}:
        return "nav"
    if key in {"enter", "return"}:
        return "enter"
    if key in {"escape", "esc"}:
        return "escape"
    if key in {"backspace", "delete", "tab", "space"}:
        return "edit"
    if key in {"?", "f1"}:
        return "help"
    if key == "/":
        return "search"
    if key in {"q", "ctrl+c"}:
        return "quit"
    if len(key) == 1 and key.isdigit():
        return "digit"
    if len(key) == 1 and key.isalpha():
        return "letter"
    return "other"


def levenshtein_similarity(a, b):
    if not a and not b:
        return 1.0
    if not a or not b:
        return 0.0
    if len(a) > len(b):
        a, b = b, a
    previous = list(range(len(a) + 1))
    for i, x in enumerate(b, 1):
        current = [i]
        for j, y in enumerate(a, 1):
            current.append(min(current[-1] + 1, previous[j] + 1, previous[j - 1] + (x != y)))
        previous = current
    return 1 - previous[-1] / max(len(a), len(b))


with TASKS_CSV.open(newline="") as f:
    all_rows = list(csv.DictReader(f))

for row in all_rows:
    row["passed_bool"] = yes(row["passed"])
    for field in (
        "turn_count",
        "reasoning_tokens",
        "n_keys",
        "n_unique_keys",
        "noop_rate",
        "reasoning_per_turn",
        "prompt_per_turn",
        "keys_per_turn",
    ):
        row[field + "_num"] = num(row[field])


def selected(row, mode):
    judge = IMAGE_MODEL if mode == "png" else TEXT_MODEL
    return row["app"] in APPS and row["observation"] == mode and row["judge_model"] == judge


tri_rows = [row for row in all_rows if selected(row, row["observation"])]
index = {
    (r["app"], r["observation"], r["repeat"], r["project"], r["task_id"]): r
    for r in tri_rows
}


feature_cache = {}


def artifact_features(row):
    path = AGENT / row["artifact_relpath"]
    cache_key = str(path)
    if cache_key in feature_cache:
        return feature_cache[cache_key]

    keys = []
    key_file = path / "keystrokes.jsonl"
    if key_file.exists():
        with key_file.open() as f:
            for line in f:
                if line.strip():
                    keys.append(json.loads(line).get("key", ""))
    sequence = [key_bucket(k) for k in keys]

    first_changed_turn = None
    waits = 0
    first_reasoning = ""
    transcript_file = path / "transcript.jsonl"
    if transcript_file.exists():
        with transcript_file.open() as f:
            for line_no, line in enumerate(f, 1):
                if not line.strip():
                    continue
                turn = json.loads(line)
                if not first_reasoning:
                    first_reasoning = (turn.get("reasoning_content") or "").lower()
                action = turn.get("action") or {}
                if action.get("action") == "wait":
                    waits += 1
                if first_changed_turn is None and turn.get("has_changed") is True:
                    first_changed_turn = line_no

    result = {
        "sequence": sequence,
        "first_key": sequence[0] if sequence else "none",
        "first_changed_turn": first_changed_turn,
        "waits": waits,
        "visual_reasoning": any(term in first_reasoning for term in VISUAL_TERMS),
    }
    feature_cache[cache_key] = result
    return result


# Condition-level outcome and behavior.
condition_metrics = []
for app in APPS:
    for mode in MODES:
        rows = [r for r in tri_rows if r["app"] == app and r["observation"] == mode]
        features = [artifact_features(r) for r in rows]
        by_task = defaultdict(list)
        for row in rows:
            by_task[(row["project"], row["task_id"])].append(row["passed_bool"])
        ks = [sum(v) for v in by_task.values()]
        failures = Counter(r["fail_kind"] for r in rows if not r["passed_bool"])
        first_keys = Counter(f["first_key"] for f in features)
        condition_metrics.append(
            {
                "app": APP_LABELS[app],
                "mode": mode,
                "n": len(rows),
                "passed": sum(r["passed_bool"] for r in rows),
                "pass_rate": mean([r["passed_bool"] for r in rows]),
                "pass_at_3": sum(k > 0 for k in ks),
                "pass_at_3_rate": mean([k > 0 for k in ks]),
                "all_3": sum(k == 3 for k in ks),
                "never_0": sum(k == 0 for k in ks),
                "wrong_submit": failures["wrong_submit"],
                "max_turns": failures["max_turns"],
                "session_lost": failures["session_lost"],
                "agent_error_limit": failures["agent_error_limit"],
                "keys_median": median([r["n_keys_num"] for r in rows]),
                "turns_median": median([r["turn_count_num"] for r in rows]),
                "reasoning_per_turn_median": median([r["reasoning_per_turn_num"] for r in rows]),
                "prompt_per_turn_median": median([r["prompt_per_turn_num"] for r in rows]),
                "noop_rate_mean": mean([r["noop_rate_num"] for r in rows]),
                "first_changed_turn_median": median([f["first_changed_turn"] for f in features]),
                "waits_mean": mean([f["waits"] for f in features]),
                "visual_reasoning_rate": mean([f["visual_reasoning"] for f in features]),
                "first_key_nav_rate": first_keys["nav"] / len(rows),
                "first_key_search_rate": first_keys["search"] / len(rows),
            }
        )

# Pooled mode metrics.
for mode in MODES:
    rows = [r for r in tri_rows if r["observation"] == mode]
    by_app_task = defaultdict(list)
    for row in rows:
        by_app_task[(row["app"], row["project"], row["task_id"])].append(row["passed_bool"])
    ks = [sum(v) for v in by_app_task.values()]
    condition_metrics.append(
        {
            "app": "Pooled",
            "mode": mode,
            "n": len(rows),
            "passed": sum(r["passed_bool"] for r in rows),
            "pass_rate": mean([r["passed_bool"] for r in rows]),
            "pass_at_3": sum(k > 0 for k in ks),
            "pass_at_3_rate": mean([k > 0 for k in ks]),
            "all_3": sum(k == 3 for k in ks),
            "never_0": sum(k == 0 for k in ks),
            "wrong_submit": sum(r["fail_kind"] == "wrong_submit" for r in rows),
            "max_turns": sum(r["fail_kind"] == "max_turns" for r in rows),
            "session_lost": sum(r["fail_kind"] == "session_lost" for r in rows),
            "agent_error_limit": sum(r["fail_kind"] == "agent_error_limit" for r in rows),
            "keys_median": median([r["n_keys_num"] for r in rows]),
            "turns_median": median([r["turn_count_num"] for r in rows]),
            "reasoning_per_turn_median": median([r["reasoning_per_turn_num"] for r in rows]),
            "prompt_per_turn_median": median([r["prompt_per_turn_num"] for r in rows]),
            "noop_rate_mean": mean([r["noop_rate_num"] for r in rows]),
            "first_changed_turn_median": median([artifact_features(r)["first_changed_turn"] for r in rows]),
            "waits_mean": mean([artifact_features(r)["waits"] for r in rows]),
            "visual_reasoning_rate": mean([artifact_features(r)["visual_reasoning"] for r in rows]),
            "first_key_nav_rate": mean([artifact_features(r)["first_key"] == "nav" for r in rows]),
            "first_key_search_rate": mean([artifact_features(r)["first_key"] == "search" for r in rows]),
        }
    )


# Paired attempt outcome tests.
paired_tests = []
for app in APPS:
    for mode_a, mode_b, family in [
        ("plain", "semantic", "text_ablation"),
        ("semantic", "png", "image_exploratory"),
        ("plain", "png", "image_exploratory"),
    ]:
        attempt_pairs = []
        task_diffs = []
        tasks = sorted(
            {
                (key[3], key[4])
                for key in index
                if key[0] == app and key[1] == mode_a
            }
        )
        for project, task_id in tasks:
            task_a, task_b = [], []
            for repeat in ("r1", "r2", "r3"):
                row_a = index.get((app, mode_a, repeat, project, task_id))
                row_b = index.get((app, mode_b, repeat, project, task_id))
                if row_a is not None and row_b is not None:
                    attempt_pairs.append((row_a, row_b))
                    task_a.append(row_a["passed_bool"])
                    task_b.append(row_b["passed_bool"])
            if task_a:
                task_diffs.append((sum(task_b) - sum(task_a)) / len(task_a))
        a_only = sum(a["passed_bool"] and not b["passed_bool"] for a, b in attempt_pairs)
        b_only = sum(not a["passed_bool"] and b["passed_bool"] for a, b in attempt_pairs)
        tasks_a_better = sum(d < 0 for d in task_diffs)
        tasks_b_better = sum(d > 0 for d in task_diffs)
        lo, hi = bootstrap_mean_ci(task_diffs)
        paired_tests.append(
            {
                "family": family,
                "app": APP_LABELS[app],
                "contrast": f"{mode_b} - {mode_a}",
                "n_tasks": len(task_diffs),
                "n_attempt_pairs": len(attempt_pairs),
                "rate_a": mean([a["passed_bool"] for a, _ in attempt_pairs]),
                "rate_b": mean([b["passed_bool"] for _, b in attempt_pairs]),
                "delta_pp": 100 * mean(task_diffs),
                "ci_low_pp": 100 * lo,
                "ci_high_pp": 100 * hi,
                "attempt_a_only": a_only,
                "attempt_b_only": b_only,
                "tasks_a_better": tasks_a_better,
                "tasks_b_better": tasks_b_better,
                "p_raw": exact_mcnemar(tasks_a_better, tasks_b_better),
            }
        )

for family in {"text_ablation", "image_exploratory"}:
    holm([r for r in paired_tests if r["family"] == family])


# Verification-type split, pooled over four apps.
verify_metrics = []
verify_types = sorted({r["verify_type"] for r in tri_rows})
for verify_type in verify_types:
    for mode in MODES:
        rows = [r for r in tri_rows if r["verify_type"] == verify_type and r["observation"] == mode]
        verify_metrics.append(
            {
                "verify_type": verify_type,
                "mode": mode,
                "n": len(rows),
                "passed": sum(r["passed_bool"] for r in rows),
                "pass_rate": mean([r["passed_bool"] for r in rows]),
            }
        )


# Project-level PNG minus semantic effects, pooled over apps/repeats/tasks.
project_deltas = []
projects = sorted({r["project"] for r in tri_rows})
for project in projects:
    row = {"project": project}
    for mode in MODES:
        rows = [r for r in tri_rows if r["project"] == project and r["observation"] == mode]
        row[mode + "_n"] = len(rows)
        row[mode + "_rate"] = mean([r["passed_bool"] for r in rows])
    row["semantic_minus_plain_pp"] = 100 * (row["semantic_rate"] - row["plain_rate"])
    row["png_minus_semantic_pp"] = 100 * (row["png_rate"] - row["semantic_rate"])
    project_deltas.append(row)

app_project_deltas = []
for app in APPS:
    for project in projects:
        matching = [r for r in tri_rows if r["app"] == app and r["project"] == project]
        if not matching:
            continue
        row = {"app": APP_LABELS[app], "project": project}
        for mode in MODES:
            rows = [r for r in matching if r["observation"] == mode]
            row[mode + "_n"] = len(rows)
            row[mode + "_rate"] = mean([r["passed_bool"] for r in rows])
        row["semantic_minus_plain_pp"] = 100 * (row["semantic_rate"] - row["plain_rate"])
        row["png_minus_semantic_pp"] = 100 * (row["png_rate"] - row["semantic_rate"])
        app_project_deltas.append(row)


# Route similarity, first-action agreement, and repeat stability.
trajectory = []
for app in APPS:
    for mode_a, mode_b in [("plain", "semantic"), ("semantic", "png"), ("plain", "png")]:
        similarities = []
        pass_similarities = []
        first_same = []
        changed_deltas = []
        for repeat in ("r1", "r2", "r3"):
            for key, row_a in index.items():
                if key[0] != app or key[1] != mode_a or key[2] != repeat:
                    continue
                other_key = (app, mode_b, key[2], key[3], key[4])
                if other_key not in index:
                    continue
                row_b = index[other_key]
                fa, fb = artifact_features(row_a), artifact_features(row_b)
                sim = levenshtein_similarity(fa["sequence"], fb["sequence"])
                similarities.append(sim)
                if row_a["passed_bool"] and row_b["passed_bool"]:
                    pass_similarities.append(sim)
                first_same.append(fa["first_key"] == fb["first_key"])
                if fa["first_changed_turn"] is not None and fb["first_changed_turn"] is not None:
                    changed_deltas.append(fb["first_changed_turn"] - fa["first_changed_turn"])
        trajectory.append(
            {
                "kind": "cross_mode",
                "app": APP_LABELS[app],
                "comparison": f"{mode_a} vs {mode_b}",
                "n": len(similarities),
                "sequence_similarity_mean": mean(similarities),
                "sequence_similarity_median": median(similarities),
                "both_pass_similarity_mean": mean(pass_similarities),
                "first_key_agreement": mean(first_same),
                "first_changed_turn_delta_median": median(changed_deltas),
            }
        )

for app in APPS:
    for mode in MODES:
        similarities = []
        first_same = []
        for project, task_id in sorted({(r["project"], r["task_id"]) for r in tri_rows if r["app"] == app}):
            run_rows = [
                index.get((app, mode, repeat, project, task_id))
                for repeat in ("r1", "r2", "r3")
            ]
            run_rows = [r for r in run_rows if r is not None]
            for i in range(len(run_rows)):
                for j in range(i + 1, len(run_rows)):
                    fa, fb = artifact_features(run_rows[i]), artifact_features(run_rows[j])
                    similarities.append(levenshtein_similarity(fa["sequence"], fb["sequence"]))
                    first_same.append(fa["first_key"] == fb["first_key"])
        trajectory.append(
            {
                "kind": "intra_mode",
                "app": APP_LABELS[app],
                "comparison": mode,
                "n": len(similarities),
                "sequence_similarity_mean": mean(similarities),
                "sequence_similarity_median": median(similarities),
                "both_pass_similarity_mean": "",
                "first_key_agreement": mean(first_same),
                "first_changed_turn_delta_median": "",
            }
        )


def write_csv(name, rows):
    path = OUT / name
    with path.open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


write_csv("condition_metrics.csv", condition_metrics)
write_csv("paired_tests.csv", paired_tests)
write_csv("verify_metrics.csv", verify_metrics)
write_csv("project_deltas.csv", project_deltas)
write_csv("app_project_deltas.csv", app_project_deltas)
write_csv("trajectory_metrics.csv", trajectory)

summary = {
    "scope": {
        "attempts_per_mode": {mode: sum(r["observation"] == mode for r in tri_rows) for mode in MODES},
        "text_model": TEXT_MODEL,
        "image_model": IMAGE_MODEL,
        "warning": "PNG-vs-text comparisons confound observation modality with judge model.",
    },
    "condition_metrics": condition_metrics,
    "paired_tests": paired_tests,
    "verify_metrics": verify_metrics,
    "project_deltas": project_deltas,
    "app_project_deltas": app_project_deltas,
    "trajectory_metrics": trajectory,
}
(OUT / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n")
print(f"Wrote analysis to {OUT}")
