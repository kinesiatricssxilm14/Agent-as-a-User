#!/usr/bin/env python3
"""Explore when one observation condition wins or loses.

Primary strata come directly from benchmark metadata. Secondary demand flags
are deterministic keyword rules applied to task descriptions before outcomes
are joined. PNG contrasts remain confounded with judge model.
"""

from __future__ import annotations

import csv
import json
import math
import random
import re
import statistics
from collections import defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TASKS_CSV = ROOT / "agent_tasks.csv"
BENCHMARK = ROOT.parent / "curated-study-kit" / "benchmark_en"
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
MODES = ("plain", "semantic", "png")


def has_any(text: str, terms: tuple[str, ...]) -> bool:
    return any(term in text for term in terms)


def classify_task(task: dict) -> dict:
    text = task["description"].lower()
    task_type = task["type"]
    return {
        "verify_type": task["observation"],
        "difficulty": task["difficulty"],
        "task_type": task_type,
        "intent": "mutate" if task_type in {"create", "edit", "delete"} else "inspect",
        "search_filter": has_any(
            text, ("find ", "search", "filter", "grep", "regex", "match ", "sql", " where ", "fuzzy")
        ),
        "sort_rank": has_any(
            text, ("sort", "group", "order", "largest", "top 5", "top five", "three largest", "descending", "ascending")
        ),
        "exact_syntax_entry": "{{" in text
        or has_any(text, ("regex ", "run grep", "run tail", "select ", "condition ", "uri ", "regexp mode")),
        "complete_output": has_any(
            text, ("complete", "full ", "all ", "at least", "row count", "display count", "file count", "key count")
        ),
        "details_inspection": has_any(
            text, ("details", "full content", "complete content", "description", "dependency", "information for")
        ),
        "explicit_visual_cue": has_any(
            text, ("highlight", "background", "cursor", "focus", "three-way", "conflict lines")
        ),
        "structured_collection": has_any(
            text,
            (
                " list",
                "table",
                "column",
                "row",
                "record",
                "package",
                "container",
                "image",
                "network",
                "volume",
                "key",
                "card",
            ),
        ),
        "state_mutation": task_type in {"create", "edit", "delete"}
        or has_any(
            text,
            (
                "create ",
                "add ",
                "install",
                "uninstall",
                "upgrade",
                "rename",
                "resolve",
                "mark ",
                "change ",
                "delete",
                "replace",
                "save",
                "export",
                "set ",
            ),
        ),
        "destructive": has_any(text, ("delete", "uninstall", "remove", "discard")),
        "external_or_async": has_any(text, ("install", "uninstall", "upgrade", "connect", "docker")),
        "multi_step_explicit": " then " in text or text.count(";") >= 1 or text.count(". ") >= 1,
    }


taxonomy = []
for spec_path in sorted(BENCHMARK.glob("P*/bench.spec.json")):
    spec = json.loads(spec_path.read_text())
    project = spec_path.parent.name
    for task in spec["tasks"]:
        oracle_path = spec_path.parent / "oracle" / f"{task['id']}.sh"
        oracle_text = oracle_path.read_text() if oracle_path.exists() else ""
        checks = sorted(set(re.findall(r"\b(check_[a-z_]+)\b", oracle_text)))
        channels = set()
        for check in checks:
            if check.startswith("check_screen"):
                channels.add("screen")
            elif check.startswith("check_filesystem"):
                channels.add("filesystem")
            elif check.startswith("check_shell"):
                channels.add("shell")
        row = {
            "project": project,
            "task_id": task["id"],
            "project_description": spec["description"],
            "task_description": task["description"],
            "oracle_checks": ";".join(checks),
            "oracle_channel": next(iter(channels)) if len(channels) == 1 else "hybrid" if channels else "unknown",
        }
        row.update(classify_task(task))
        taxonomy.append(row)

if len(taxonomy) != 84:
    raise RuntimeError(f"Expected 84 benchmark tasks, found {len(taxonomy)}")

taxonomy_index = {(r["project"], r["task_id"]): r for r in taxonomy}

with TASKS_CSV.open(newline="") as f:
    raw_rows = list(csv.DictReader(f))


def selected(row):
    if row["app"] not in APPS or row["observation"] not in MODES:
        return False
    expected = IMAGE_MODEL if row["observation"] == "png" else TEXT_MODEL
    return row["judge_model"] == expected


rows = [r for r in raw_rows if selected(r)]
for row in rows:
    row["passed_bool"] = row["passed"].lower() == "true"

# Interface properties are measured without outcomes, using Plain initial screens.
surface_values = defaultdict(lambda: defaultdict(list))
for row in rows:
    if row["observation"] != "plain":
        continue
    key = (row["app"], row["project"])
    for field in ("screen0_chars", "screen0_lines", "screen0_keychips", "screen0_help_hint", "screen0_box"):
        if row[field] != "":
            surface_values[key][field].append(float(row[field]))

surfaces = {}
for key, values in surface_values.items():
    surfaces[key] = {field: statistics.median(vals) for field, vals in values.items()}

density_cuts = {}
keychip_cuts = {}
for app in APPS:
    app_surfaces = [v for (surface_app, _), v in surfaces.items() if surface_app == app]
    density_values = sorted(v["screen0_chars"] for v in app_surfaces)
    keychip_values = sorted(v["screen0_keychips"] for v in app_surfaces)
    density_cuts[app] = (
        density_values[len(density_values) // 3],
        density_values[(2 * len(density_values)) // 3],
    )
    keychip_cuts[app] = keychip_values[(2 * len(keychip_values)) // 3]


def density_bin(app, value):
    low_cut, high_cut = density_cuts[app]
    if value <= low_cut:
        return "low"
    if value <= high_cut:
        return "medium"
    return "high"


# Build one app × task case, with k successes out of three for each mode.
successes = defaultdict(int)
attempt_counts = defaultdict(int)
fail_counts = defaultdict(lambda: defaultdict(int))
for row in rows:
    key = (row["app"], row["project"], row["task_id"], row["observation"])
    successes[key] += int(row["passed_bool"])
    attempt_counts[key] += 1
    fail_counts[key][row["fail_kind"]] += 1

cases = []
for app in APPS:
    for task in taxonomy:
        project, task_id = task["project"], task["task_id"]
        surface = surfaces[(app, project)]
        row = {
            "app": APP_LABELS[app],
            "project": project,
            "task_id": task_id,
            "description": task["task_description"],
            **{k: v for k, v in task.items() if k not in {"project", "task_id", "project_description", "task_description"}},
            "screen_chars": surface["screen0_chars"],
            "screen_lines": surface["screen0_lines"],
            "screen_keychips": surface["screen0_keychips"],
            "screen_help_hint": bool(surface["screen0_help_hint"]),
            "screen_box": bool(surface["screen0_box"]),
            "screen_density": density_bin(app, surface["screen0_chars"]),
            "keychips_high": surface["screen0_keychips"] >= keychip_cuts[app],
        }
        for mode in MODES:
            key = (app, project, task_id, mode)
            if attempt_counts[key] != 3:
                raise RuntimeError(f"Incomplete case: {key} has {attempt_counts[key]} attempts")
            row[mode + "_k"] = successes[key]
            row[mode + "_agent_error_limit"] = fail_counts[key]["agent_error_limit"]
            row[mode + "_session_lost"] = fail_counts[key]["session_lost"]
        row["semantic_minus_plain"] = row["semantic_k"] - row["plain_k"]
        row["png_minus_semantic"] = row["png_k"] - row["semantic_k"]
        row["png_minus_plain"] = row["png_k"] - row["plain_k"]
        cases.append(row)


def bootstrap_task_ci(selected_cases, delta_field, n_boot=5000, seed=20260825):
    by_project_task = defaultdict(lambda: defaultdict(list))
    for row in selected_cases:
        by_project_task[row["project"]][row["task_id"]].append(row[delta_field] / 3)
    if not by_project_task:
        return None, None
    rng = random.Random(seed)
    sampled = []
    projects = list(by_project_task)
    for _ in range(n_boot):
        draw = []
        for _ in projects:
            project = projects[rng.randrange(len(projects))]
            tasks = list(by_project_task[project])
            for _ in tasks:
                task = tasks[rng.randrange(len(tasks))]
                draw.append(statistics.fmean(by_project_task[project][task]))
        sampled.append(statistics.fmean(draw))
    sampled.sort()
    return sampled[int(0.025 * n_boot)], sampled[int(0.975 * n_boot)]


def exact_sign_p(positive, negative):
    n = positive + negative
    if n == 0:
        return 1.0
    k = min(positive, negative)
    return min(1.0, 2 * sum(math.comb(n, i) for i in range(k + 1)) / (2**n))


def summarize_stratum(dimension, level, selected_cases, contrast, delta_field):
    deltas = [r[delta_field] for r in selected_cases]
    better = sum(d > 0 for d in deltas)
    worse = sum(d < 0 for d in deltas)
    tie = sum(d == 0 for d in deltas)
    lo, hi = bootstrap_task_ci(selected_cases, delta_field)
    by_task = defaultdict(list)
    for row in selected_cases:
        by_task[(row["project"], row["task_id"])].append(row[delta_field])
    task_deltas = [statistics.fmean(v) for v in by_task.values()]
    task_better = sum(d > 0 for d in task_deltas)
    task_worse = sum(d < 0 for d in task_deltas)
    per_app = {}
    for app in APP_LABELS.values():
        app_deltas = [r[delta_field] for r in selected_cases if r["app"] == app]
        per_app[app] = statistics.fmean(app_deltas) / 3 if app_deltas else None
    return {
        "dimension": dimension,
        "level": str(level).lower(),
        "contrast": contrast,
        "n_cases": len(selected_cases),
        "n_task_templates": len({(r["project"], r["task_id"]) for r in selected_cases}),
        "task_templates_better": task_better,
        "task_templates_worse": task_worse,
        "p_task_sign": exact_sign_p(task_better, task_worse),
        "better": better,
        "tie": tie,
        "worse": worse,
        "better_pct": 100 * better / len(deltas),
        "worse_pct": 100 * worse / len(deltas),
        "strong_better": sum(d >= 2 for d in deltas),
        "strong_worse": sum(d <= -2 for d in deltas),
        "rescue_0_to_pass": sum(
            r[contrast.split("_vs_")[1] + "_k"] == 0 and r[contrast.split("_vs_")[0] + "_k"] > 0
            for r in selected_cases
        ),
        "harm_pass_to_0": sum(
            r[contrast.split("_vs_")[1] + "_k"] > 0 and r[contrast.split("_vs_")[0] + "_k"] == 0
            for r in selected_cases
        ),
        "mean_delta_pp": 100 * statistics.fmean(deltas) / 3,
        "ci_low_pp": 100 * lo,
        "ci_high_pp": 100 * hi,
        "apps_positive": sum(v is not None and v > 0 for v in per_app.values()),
        "apps_negative": sum(v is not None and v < 0 for v in per_app.values()),
        "oracle_delta_pp": 100 * per_app["Oracle"] if per_app["Oracle"] is not None else "",
        "harness_delta_pp": 100 * per_app["Harness"] if per_app["Harness"] is not None else "",
        "claude_delta_pp": 100 * per_app["Claude Code"] if per_app["Claude Code"] is not None else "",
        "tcodex_delta_pp": 100 * per_app["Codex"] if per_app["Codex"] is not None else "",
    }


dimensions = {
    "verify_type": sorted({r["verify_type"] for r in cases}),
    "oracle_channel": sorted({r["oracle_channel"] for r in cases}),
    "difficulty": ["easy", "medium", "hard"],
    "task_type": ["navigate", "create", "edit", "delete"],
    "intent": ["inspect", "mutate"],
    "screen_density": ["low", "medium", "high"],
    "screen_help_hint": [False, True],
    "keychips_high": [False, True],
}
binary_flags = [
    "search_filter",
    "sort_rank",
    "exact_syntax_entry",
    "complete_output",
    "details_inspection",
    "explicit_visual_cue",
    "structured_collection",
    "state_mutation",
    "destructive",
    "external_or_async",
    "multi_step_explicit",
]
for flag in binary_flags:
    dimensions[flag] = [False, True]

contrasts = {
    "semantic_vs_plain": "semantic_minus_plain",
    "png_vs_semantic": "png_minus_semantic",
    "png_vs_plain": "png_minus_plain",
}
strata = []
for dimension, levels in dimensions.items():
    for level in levels:
        subset = [r for r in cases if r[dimension] == level]
        if not subset:
            continue
        for contrast, delta_field in contrasts.items():
            strata.append(summarize_stratum(dimension, level, subset, contrast, delta_field))


def contrast_modes(contrast):
    later, earlier = contrast.split("_vs_")
    return later, earlier


def allowed_by_policy(row, contrast, policy):
    if policy == "all":
        return True
    later, earlier = contrast_modes(contrast)
    modes = (later, earlier)
    if policy == "no_agent_error_limit":
        return not any(row[mode + "_agent_error_limit"] for mode in modes)
    if policy == "no_runtime_loss":
        return not any(
            row[mode + "_agent_error_limit"] or row[mode + "_session_lost"]
            for mode in modes
        )
    raise ValueError(policy)


robustness = []
overall_robustness = []
for policy in ("all", "no_agent_error_limit", "no_runtime_loss"):
    for contrast, delta_field in contrasts.items():
        subset = [r for r in cases if allowed_by_policy(r, contrast, policy)]
        overall_robustness.append(
            {"filter": policy, **summarize_stratum("all", "all", subset, contrast, delta_field)}
        )
    for dimension, levels in dimensions.items():
        for level in levels:
            base = [r for r in cases if r[dimension] == level]
            for contrast, delta_field in contrasts.items():
                subset = [r for r in base if allowed_by_policy(r, contrast, policy)]
                if not subset:
                    continue
                result = summarize_stratum(dimension, level, subset, contrast, delta_field)
                robustness.append({"filter": policy, **result})


def add_bh(rows):
    ordered = sorted(enumerate(rows), key=lambda x: x[1]["p_task_sign"])
    m = len(rows)
    adjusted = [1.0] * m
    running = 1.0
    for rank in range(m - 1, -1, -1):
        idx, row = ordered[rank]
        running = min(running, row["p_task_sign"] * m / (rank + 1))
        adjusted[idx] = min(1.0, running)
    for row, value in zip(rows, adjusted):
        row["p_bh"] = value


for policy in ("all", "no_agent_error_limit", "no_runtime_loss"):
    for contrast in contrasts:
        add_bh([r for r in robustness if r["filter"] == policy and r["contrast"] == contrast])

# Task-template consistency across all four app implementations.
consistent = []
for task in taxonomy:
    project, task_id = task["project"], task["task_id"]
    task_cases = [r for r in cases if r["project"] == project and r["task_id"] == task_id]
    row = {
        "project": project,
        "task_id": task_id,
        "description": task["task_description"],
        "verify_type": task["verify_type"],
        "difficulty": task["difficulty"],
        "task_type": task["task_type"],
    }
    for name, delta_field in contrasts.items():
        deltas = [r[delta_field] for r in task_cases]
        pos, neg = sum(d > 0 for d in deltas), sum(d < 0 for d in deltas)
        row[name + "_sum"] = sum(deltas)
        row[name + "_apps_better"] = pos
        row[name + "_apps_worse"] = neg
        row[name + "_pattern"] = (
            "consistent_better"
            if pos >= 2 and neg == 0
            else "consistent_worse"
            if neg >= 2 and pos == 0
            else "mixed"
            if pos and neg
            else "weak_or_tie"
        )
    consistent.append(row)

def build_consistency(policy):
    output = []
    for task in taxonomy:
        project, task_id = task["project"], task["task_id"]
        base_cases = [r for r in cases if r["project"] == project and r["task_id"] == task_id]
        row = {
            "project": project,
            "task_id": task_id,
            "description": task["task_description"],
        }
        for name, delta_field in contrasts.items():
            task_cases = [r for r in base_cases if allowed_by_policy(r, name, policy)]
            deltas = [r[delta_field] for r in task_cases]
            pos, neg = sum(d > 0 for d in deltas), sum(d < 0 for d in deltas)
            row[name + "_n_apps"] = len(task_cases)
            row[name + "_sum"] = sum(deltas)
            row[name + "_apps_better"] = pos
            row[name + "_apps_worse"] = neg
            row[name + "_pattern"] = (
                "consistent_better"
                if len(task_cases) >= 2 and pos >= 2 and neg == 0
                else "consistent_worse"
                if len(task_cases) >= 2 and neg >= 2 and pos == 0
                else "mixed"
                if pos and neg
                else "weak_or_tie"
            )
        output.append(row)
    return output


consistent_no_agent = build_consistency("no_agent_error_limit")
consistent_clean = build_consistency("no_runtime_loss")


def write_csv(name, data):
    with (OUT / name).open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(data[0]))
        writer.writeheader()
        writer.writerows(data)


write_csv("task_taxonomy.csv", taxonomy)
write_csv("case_outcomes.csv", cases)
write_csv("strata_summary.csv", strata)
write_csv("overall_robustness.csv", overall_robustness)
write_csv("strata_robustness.csv", robustness)
write_csv("task_consistency.csv", consistent)
write_csv("task_consistency_no_agent.csv", consistent_no_agent)
write_csv("task_consistency_clean.csv", consistent_clean)

summary = {
    "scope": {
        "n_task_templates": len(taxonomy),
        "n_cases": len(cases),
        "case_definition": "app × benchmark task, with three repeats per modality",
        "density_cuts_chars_by_app": {
            APP_LABELS[app]: list(cuts) for app, cuts in density_cuts.items()
        },
        "high_keychips_cut_by_app": {
            APP_LABELS[app]: cut for app, cut in keychip_cuts.items()
        },
        "bootstrap": "resample projects, then tasks within project; preserve app/modality/repeats",
        "warning": "PNG contrasts confound observation modality with judge model.",
    },
    "overall": [
        summarize_stratum("all", "all", cases, contrast, delta_field)
        for contrast, delta_field in contrasts.items()
    ],
    "overall_robustness": overall_robustness,
    "task_consistency_counts": {
        contrast: {
            pattern: sum(r[contrast + "_pattern"] == pattern for r in consistent)
            for pattern in ("consistent_better", "consistent_worse", "mixed", "weak_or_tie")
        }
        for contrast in contrasts
    },
    "task_consistency_clean_counts": {
        contrast: {
            pattern: sum(r[contrast + "_pattern"] == pattern for r in consistent_clean)
            for pattern in ("consistent_better", "consistent_worse", "mixed", "weak_or_tie")
        }
        for contrast in contrasts
    },
    "task_consistency_no_agent_counts": {
        contrast: {
            pattern: sum(r[contrast + "_pattern"] == pattern for r in consistent_no_agent)
            for pattern in ("consistent_better", "consistent_worse", "mixed", "weak_or_tie")
        }
        for contrast in contrasts
    },
}
(OUT / "case_summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n")
print(f"Wrote {len(cases)} cases and {len(strata)} stratum rows to {OUT}")
