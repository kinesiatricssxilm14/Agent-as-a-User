from __future__ import annotations

from pathlib import Path

import pytest

from keyboard_agent.human.batch import discover_bench_projects


def test_discover_bench_projects_sorts_only_valid_projects(tmp_path: Path):
    valid_2 = tmp_path / "P02-kairo"
    valid_1 = tmp_path / "P01-aptui"
    invalid = tmp_path / "notes"
    missing_spec = tmp_path / "P03-flow"
    for path in (valid_2, valid_1, invalid, missing_spec):
        path.mkdir()
    (valid_2 / "bench.spec.json").write_text("{}", encoding="utf-8")
    (valid_1 / "bench.spec.json").write_text("{}", encoding="utf-8")
    (invalid / "bench.spec.json").write_text("{}", encoding="utf-8")

    assert discover_bench_projects(tmp_path) == [valid_1, valid_2]


def test_discover_bench_projects_rejects_missing_root(tmp_path: Path):
    with pytest.raises(ValueError, match="Benchmark root does not exist"):
        discover_bench_projects(tmp_path / "missing")
