from __future__ import annotations

import json
import random
from pathlib import Path

import pytest

from keyboard_agent.human.suite_scheduler import (
    ProjectProgress,
    init_suite_state,
    pick_next_project,
    remaining_presentation_order,
    restore_rng,
    simulate_presentation_order,
)


def _write_spec(project_dir: Path, task_ids: list[str]) -> None:
    project_dir.mkdir(parents=True, exist_ok=True)
    tasks = [{"id": tid, "description": f"do {tid}", "observation": "screen"} for tid in task_ids]
    (project_dir / "bench.spec.json").write_text(
        json.dumps({"project_id": "P99", "slug": "demo", "tasks": tasks}),
        encoding="utf-8",
    )


def test_within_project_tasks_stay_in_order(tmp_path: Path):
    _write_spec(tmp_path / "P01-a", ["T01", "T02", "T03"])
    _write_spec(tmp_path / "P02-b", ["T01", "T02"])
    state = init_suite_state(tmp_path, operator_id="P001", seed=7)
    order = simulate_presentation_order(state, seed=7)

    p01 = [task for project, task in order if project == "P01-a"]
    p02 = [task for project, task in order if project == "P02-b"]
    assert p01 == ["T01", "T02", "T03"]
    assert p02 == ["T01", "T02"]


def test_frontier_is_random_across_projects(tmp_path: Path):
    _write_spec(tmp_path / "P01-a", ["T01"])
    _write_spec(tmp_path / "P02-b", ["T01"])
    _write_spec(tmp_path / "P03-c", ["T01"])

    first_projects = set()
    for seed in range(20):
        state = init_suite_state(tmp_path, operator_id="P001", seed=seed)
        first = simulate_presentation_order(state, seed=seed)[0][0]
        first_projects.add(first)
    assert len(first_projects) > 1


def test_pick_next_project_only_from_active_frontiers():
    import random

    state_projects = {
        "P01": ProjectProgress(project="P01", task_ids=["T01", "T02"], next_index=2),
        "P02": ProjectProgress(project="P02", task_ids=["T01", "T02"], next_index=0),
        "P03": ProjectProgress(project="P03", task_ids=["T01"], next_index=1),
    }
    from keyboard_agent.human.suite_scheduler import SuiteScheduleState

    state = SuiteScheduleState(projects=state_projects, seed=1)
    rng = random.Random(1)
    picked = pick_next_project(state, rng)
    assert picked is not None
    assert picked.project == "P02"
    assert picked.peek_task() == "T01"


def test_failed_task_refs_uses_last_attempt():
    from keyboard_agent.human.suite_scheduler import SuiteScheduleState, failed_task_refs

    state = SuiteScheduleState(
        completed=[
            {"project": "P01-a", "task_id": "T01", "passed": False},
            {"project": "P01-a", "task_id": "T01", "passed": True},
            {"project": "P02-b", "task_id": "T02", "passed": False},
        ]
    )
    assert failed_task_refs(state) == [("P02-b", "T02")]


def test_init_suite_state_rejects_missing_root(tmp_path: Path):
    with pytest.raises(ValueError, match="Benchmark root does not exist"):
        init_suite_state(tmp_path / "missing", operator_id="P001", seed=1)


def test_remaining_order_resumes_rng_stream(tmp_path: Path):
    _write_spec(tmp_path / "P01-a", ["T01", "T02"])
    _write_spec(tmp_path / "P02-b", ["T01", "T02"])
    _write_spec(tmp_path / "P03-c", ["T01", "T02"])
    state = init_suite_state(tmp_path, operator_id="P001", seed=42)
    rng = random.Random(42)

    for _ in range(4):
        picked = pick_next_project(state, rng)
        assert picked is not None
        task_id = picked.advance()
        state.completed.append(
            {
                "project": picked.project,
                "task_id": task_id,
                "artifact_key": f"{picked.project}/{task_id}",
                "passed": True,
            }
        )

    actual_next = pick_next_project(state, rng)
    assert actual_next is not None
    actual_task = actual_next.peek_task()

    upcoming = remaining_presentation_order(state)
    assert upcoming
    assert upcoming[0] == (actual_next.project, actual_task)
