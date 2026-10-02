from __future__ import annotations

from pathlib import Path

from keyboard_agent.recorder import allocate_run_dir, make_run_id, sanitize_run_id_part


def test_make_run_id():
    project = Path("/path/to/anonymous-artifact")
    assert make_run_id(
        runner_label="deepseek-v4-flash",
        project_dir=project,
        observation_mode="semantic",
        ts="20260622T034203Z",
    ) == "deepseek-v4-flash-P01-aptui-semantic-20260622T034203Z"
    assert make_run_id(
        runner_label="P001",
        project_dir=project,
        observation_mode="plain",
        ts="20260622T034203Z",
    ) == "P001-P01-aptui-plain-20260622T034203Z"
    assert make_run_id(
        runner_label="human",
        project_dir=project,
        observation_mode="svg",
        ts="20260622T034203Z",
    ) == "human-P01-aptui-svg-20260622T034203Z"


def test_sanitize_run_id_part():
    assert sanitize_run_id_part("gpt-4o/mini") == "gpt-4o-mini"


def test_allocate_run_dir_unique(tmp_path):
    run_id = "human-P01-aptui-semantic-20260622T034203Z"
    first_id, first_dir = allocate_run_dir(tmp_path, run_id)
    first_dir.mkdir(parents=True, exist_ok=True)
    second_id, second_dir = allocate_run_dir(tmp_path, run_id)
    assert first_id == run_id
    assert second_id == f"{run_id}-2"
    assert first_dir != second_dir
