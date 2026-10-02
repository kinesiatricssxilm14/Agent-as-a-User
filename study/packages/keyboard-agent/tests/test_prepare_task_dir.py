from __future__ import annotations

import json
from pathlib import Path

from keyboard_agent.recorder import RunRecorder


def _make_recorder(tmp_path: Path) -> RunRecorder:
    run_dir = tmp_path / "run"
    run_dir.mkdir()
    recorder = RunRecorder.__new__(RunRecorder)
    recorder.run_id = "test-run"
    recorder.run_dir = run_dir
    recorder.project_dir = tmp_path
    recorder._tasks = []
    recorder._started_at = "t0"
    recorder._meta = {}
    return recorder


def test_prepare_archives_prior_attempt(tmp_path: Path):
    recorder = _make_recorder(tmp_path)
    key = "P07-ec/T03"
    first = recorder.task_dir(key)
    (first / "result.json").write_text('{"passed": false}', encoding="utf-8")
    (first / "keystrokes.jsonl").write_text("{}\n", encoding="utf-8")
    (first / "turns" / "turn_001.svg").write_text("<svg/>", encoding="utf-8")

    meta = recorder.prepare_task_dir(key)
    assert meta["attempt"] == 2
    assert meta["archived_previous"] == "P07-ec/T03.attempt01"

    archived = recorder.run_dir / "P07-ec" / "T03.attempt01"
    assert archived.is_dir()
    assert (archived / "result.json").is_file()
    assert (archived / "turns" / "turn_001.svg").is_file()
    assert (archived / "attempt.json").is_file()

    current = recorder.run_dir / key
    assert current.is_dir()
    assert not (current / "result.json").exists()
    assert json.loads((current / "attempt.json").read_text(encoding="utf-8"))["attempt"] == 2


def test_prepare_keeps_multiple_archives(tmp_path: Path):
    recorder = _make_recorder(tmp_path)
    key = "P08-snip/T02"

    for _ in range(3):
        d = recorder.task_dir(key)
        (d / "result.json").write_text('{"passed": false}', encoding="utf-8")
        meta = recorder.prepare_task_dir(key)

    assert meta["attempt"] == 4
    assert (recorder.run_dir / "P08-snip" / "T02.attempt01").is_dir()
    assert (recorder.run_dir / "P08-snip" / "T02.attempt02").is_dir()
    assert (recorder.run_dir / "P08-snip" / "T02.attempt03").is_dir()
    assert (recorder.run_dir / key / "attempt.json").is_file()


def test_prepare_noop_on_empty_dir(tmp_path: Path):
    recorder = _make_recorder(tmp_path)
    key = "P01-aptui/T01"
    meta = recorder.prepare_task_dir(key)
    assert meta["attempt"] == 1
    assert meta["archived_previous"] is None
    assert not list((recorder.run_dir / "P01-aptui").glob("T01.attempt*"))
