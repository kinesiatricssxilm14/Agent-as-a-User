from __future__ import annotations

import json
import os
import random
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .batch import discover_bench_projects


def _find_suite_benchmark() -> Path | None:
    here = Path(__file__).resolve()
    for parent in here.parents:
        for rel in (
            "benchmark_cn",
            "benchmark_en",
            "benchmark",
            "curated-study-kit/benchmark_cn",
            "curated-study-kit/benchmark_en",
            "curated-study-kit/benchmark",
        ):
            candidate = parent / rel
            if candidate.is_dir() and any(candidate.glob("P*/bench.spec.json")):
                return candidate.resolve()
    return None


def default_suite_root() -> Path:
    if env := os.environ.get("BENCH_SUITE_ROOT"):
        return Path(env).expanduser().resolve()
    if found := _find_suite_benchmark():
        return found
    raise FileNotFoundError(
        "benchmark suite not found; set BENCH_SUITE_ROOT or run from curated-study-kit"
    )


def load_project_task_ids(project_dir: Path) -> list[str]:
    spec_path = project_dir / "bench.spec.json"
    if not spec_path.is_file():
        raise ValueError(f"bench.spec.json not found: {spec_path}")
    spec = json.loads(spec_path.read_text(encoding="utf-8"))
    tasks = spec.get("tasks") or []
    if not tasks:
        raise ValueError(f"no tasks in {spec_path}")
    return [str(task["id"]) for task in tasks]


@dataclass
class ProjectProgress:
    project: str
    task_ids: list[str]
    next_index: int = 0

    @property
    def remaining(self) -> int:
        return max(0, len(self.task_ids) - self.next_index)

    def has_remaining(self) -> bool:
        return self.next_index < len(self.task_ids)

    def peek_task(self) -> str:
        if not self.has_remaining():
            raise IndexError(f"no remaining tasks for {self.project}")
        return self.task_ids[self.next_index]

    def advance(self) -> str:
        task_id = self.peek_task()
        self.next_index += 1
        return task_id


@dataclass
class SuiteScheduleState:
    version: int = 1
    suite_root: str = ""
    operator_id: str = "human"
    seed: int = 0
    started_at: str = ""
    updated_at: str = ""
    run_dir: str | None = None
    projects: dict[str, ProjectProgress] = field(default_factory=dict)
    completed: list[dict[str, Any]] = field(default_factory=list)

    @property
    def total_tasks(self) -> int:
        return sum(len(p.task_ids) for p in self.projects.values())

    @property
    def completed_count(self) -> int:
        return len(self.completed)

    @property
    def remaining_count(self) -> int:
        return sum(p.remaining for p in self.projects.values())

    def to_dict(self) -> dict[str, Any]:
        return {
            "version": self.version,
            "suite_root": self.suite_root,
            "operator_id": self.operator_id,
            "seed": self.seed,
            "started_at": self.started_at,
            "updated_at": self.updated_at,
            "run_dir": self.run_dir,
            "projects": {
                name: {
                    "project": prog.project,
                    "task_ids": prog.task_ids,
                    "next_index": prog.next_index,
                }
                for name, prog in self.projects.items()
            },
            "completed": self.completed,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SuiteScheduleState:
        projects = {}
        for name, raw in (data.get("projects") or {}).items():
            projects[name] = ProjectProgress(
                project=str(raw.get("project", name)),
                task_ids=list(raw.get("task_ids") or []),
                next_index=int(raw.get("next_index", 0)),
            )
        return cls(
            version=int(data.get("version", 1)),
            suite_root=str(data.get("suite_root", "")),
            operator_id=str(data.get("operator_id", "human")),
            seed=int(data.get("seed", 0)),
            started_at=str(data.get("started_at", "")),
            updated_at=str(data.get("updated_at", "")),
            run_dir=data.get("run_dir"),
            projects=projects,
            completed=list(data.get("completed") or []),
        )


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def init_suite_state(
    suite_root: Path,
    *,
    operator_id: str,
    seed: int,
    project_names: list[str] | None = None,
) -> SuiteScheduleState:
    suite_root = suite_root.expanduser().resolve()
    projects = discover_bench_projects(suite_root)
    if project_names:
        wanted = set(project_names)
        projects = [p for p in projects if p.name in wanted]
        missing = wanted - {p.name for p in projects}
        if missing:
            raise ValueError(f"projects not found in suite root: {sorted(missing)}")
    if not projects:
        raise ValueError(f"no benchmark projects under {suite_root}")

    progress: dict[str, ProjectProgress] = {}
    for project_dir in projects:
        progress[project_dir.name] = ProjectProgress(
            project=project_dir.name,
            task_ids=load_project_task_ids(project_dir),
        )
    now = _utc_now()
    return SuiteScheduleState(
        suite_root=str(suite_root),
        operator_id=operator_id,
        seed=seed,
        started_at=now,
        updated_at=now,
        projects=progress,
    )


def pick_next_project(
    state: SuiteScheduleState,
    rng: random.Random,
) -> ProjectProgress | None:
    active = [prog for prog in state.projects.values() if prog.has_remaining()]
    if not active:
        return None
    return rng.choice(active)


def restore_rng(state: SuiteScheduleState) -> random.Random:
    """Replay completed picks so RNG matches a resumed suite run."""
    shadow = SuiteScheduleState.from_dict(state.to_dict())
    for prog in shadow.projects.values():
        prog.next_index = 0
    rng = random.Random(state.seed)
    for _ in range(state.completed_count):
        picked = pick_next_project(shadow, rng)
        if picked is None:
            break
        picked.advance()
    return rng


def simulate_presentation_order(
    state: SuiteScheduleState,
    *,
    seed: int | None = None,
    rng: random.Random | None = None,
) -> list[tuple[str, str]]:
    """Return full (project, task_id) order for dry-run / audit."""
    if rng is None:
        if state.completed_count == 0:
            rng = random.Random(state.seed if seed is None else seed)
        else:
            rng = restore_rng(state)
    clones = {
        name: ProjectProgress(
            project=prog.project,
            task_ids=list(prog.task_ids),
            next_index=prog.next_index,
        )
        for name, prog in state.projects.items()
    }
    shadow = SuiteScheduleState(
        suite_root=state.suite_root,
        operator_id=state.operator_id,
        seed=state.seed,
        projects=clones,
    )
    order: list[tuple[str, str]] = []
    while True:
        picked = pick_next_project(shadow, rng)
        if picked is None:
            break
        task_id = picked.advance()
        order.append((picked.project, task_id))
    return order


def suite_state_path(output_dir: Path, *, operator_id: str, suite_root: Path) -> Path:
    suite_name = suite_root.name or "suite"
    safe_op = operator_id.replace("/", "-")
    safe_suite = suite_name.replace("/", "-")
    return (output_dir / ".suite_state" / f"{safe_op}__{safe_suite}.json").resolve()


def save_suite_state(state: SuiteScheduleState, path: Path) -> None:
    state.updated_at = _utc_now()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state.to_dict(), indent=2, ensure_ascii=False), encoding="utf-8")


def load_suite_state(path: Path) -> SuiteScheduleState:
    if not path.is_file():
        raise FileNotFoundError(f"suite state not found: {path}")
    return SuiteScheduleState.from_dict(json.loads(path.read_text(encoding="utf-8")))


def artifact_task_key(project_name: str, task_id: str) -> str:
    return f"{project_name}/{task_id}"


def remaining_presentation_order(
    state: SuiteScheduleState,
    *,
    seed: int | None = None,
) -> list[tuple[str, str]]:
    """Tasks still to run from current frontier state."""
    return simulate_presentation_order(state, seed=seed)


def failed_task_refs(state: SuiteScheduleState) -> list[tuple[str, str]]:
    """(project, task_id) pairs whose latest recorded attempt did not pass."""
    last_pass: dict[tuple[str, str], bool] = {}
    for entry in state.completed:
        ref = (str(entry["project"]), str(entry["task_id"]))
        last_pass[ref] = bool(entry.get("passed"))
    return [ref for ref, passed in last_pass.items() if not passed]


def suite_manifest(suite_root: Path) -> dict[str, Any]:
    """Project/task counts for distribution docs."""
    suite_root = suite_root.expanduser().resolve()
    projects = discover_bench_projects(suite_root)
    rows = []
    total = 0
    for project_dir in projects:
        task_ids = load_project_task_ids(project_dir)
        total += len(task_ids)
        rows.append({"project": project_dir.name, "task_count": len(task_ids)})
    return {
        "suite_root": str(suite_root),
        "project_count": len(rows),
        "task_count": total,
        "projects": rows,
    }
