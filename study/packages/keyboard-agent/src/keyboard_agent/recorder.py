from __future__ import annotations

import json
import os
import re
import shutil
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .models import TaskRunResult, TokenUsage, TurnRecord


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


_INVALID_RUN_ID_CHARS = re.compile(r'[\\/:*?"<>|]+')
_ATTEMPT_DIR_RE = re.compile(r"^(?P<base>.+)\.attempt(?P<n>\d+)$")
_TASK_ARTIFACT_MARKERS = (
    "result.json",
    "keystrokes.jsonl",
    "transcript.jsonl",
    "live.json",
    "initial_screen.json",
)


def sanitize_run_id_part(value: str) -> str:
    cleaned = _INVALID_RUN_ID_CHARS.sub("-", value.strip())
    return cleaned.strip("-") or "unknown"


def make_run_id(
    *,
    runner_label: str,
    project_dir: Path,
    observation_mode: str = "semantic",
    ts: str | None = None,
) -> str:
    """Build run id: <model|human>-<Pxx-slug>-<mode>-<YYYYMMDDTHHMMSSZ>."""
    stamp = ts or datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    label = sanitize_run_id_part(runner_label)
    project = sanitize_run_id_part(project_dir.name)
    mode = sanitize_run_id_part(observation_mode)
    return f"{label}-{project}-{mode}-{stamp}"


def allocate_run_dir(base_dir: Path, run_id: str) -> tuple[str, Path]:
    """Return a unique run_id and directory under base_dir."""
    candidate = run_id
    suffix = 2
    while (base_dir / candidate).exists():
        candidate = f"{run_id}-{suffix}"
        suffix += 1
    run_dir = (base_dir / candidate).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    return candidate, run_dir


class RunRecorder:
    """All artifacts for one suite run live under runs/<run_id>/."""

    def __init__(
        self,
        base_dir: Path,
        *,
        project_dir: Path,
        config_snapshot: dict[str, Any],
        framework: str = "observe-act",
        runner_label: str | None = None,
        observation_mode: str = "semantic",
        run_dir: Path | None = None,
        artifact_prefix: str | None = None,
    ) -> None:
        self.project_dir = project_dir
        self.artifact_prefix = sanitize_run_id_part(artifact_prefix) if artifact_prefix else None
        self._tasks: list[TaskRunResult] = []
        if run_dir is not None:
            self.run_dir = Path(run_dir).expanduser().resolve()
            self.run_dir.mkdir(parents=True, exist_ok=True)
            self.run_id = self.run_dir.name
            self._started_at = _utc_now()
            meta_path = self.run_dir / "meta.json"
            if meta_path.is_file():
                try:
                    existing = json.loads(meta_path.read_text(encoding="utf-8"))
                    if isinstance(existing, dict) and existing.get("started_at"):
                        self._started_at = str(existing["started_at"])
                except json.JSONDecodeError:
                    pass
            self._meta = {
                "run_id": self.run_id,
                "started_at": self._started_at,
                "project_dir": str(project_dir),
                "framework": framework,
                "config": config_snapshot,
                "shared_run_dir": True,
            }
            if not meta_path.is_file():
                self._write_json(meta_path, self._meta)
        else:
            label = runner_label or "run"
            run_id = make_run_id(
                runner_label=label,
                project_dir=project_dir,
                observation_mode=observation_mode,
            )
            self.run_id, self.run_dir = allocate_run_dir(base_dir, run_id)
            self._started_at = _utc_now()
            self._meta = {
                "run_id": self.run_id,
                "started_at": self._started_at,
                "project_dir": str(project_dir),
                "framework": framework,
                "config": config_snapshot,
            }
            self._write_json(self.run_dir / "meta.json", self._meta)

    def artifact_key(self, task_id: str) -> str:
        """Return on-disk key; optionally nest under project (Pxx-slug/T01)."""
        if not self.artifact_prefix:
            return task_id
        prefix = self.artifact_prefix
        if task_id == prefix or task_id.startswith(prefix + "/"):
            return task_id
        return f"{prefix}/{task_id}"

    def task_dir(self, task_id: str) -> Path:
        d = self.run_dir / self.artifact_key(task_id)
        d.mkdir(parents=True, exist_ok=True)
        (d / "turns").mkdir(exist_ok=True)
        return d

    def prepare_task_dir(self, task_id: str) -> dict[str, Any]:
        """Ensure a fresh task dir; archive any prior attempt instead of overwriting.

        Layout:
          P07-ec/T03/                 # latest attempt
          P07-ec/T03.attempt01/       # first attempt (kept)
          P07-ec/T03.attempt02/       # second attempt (kept)
        """
        key = self.artifact_key(task_id)
        task_path = self.run_dir / key
        archived_previous: str | None = None
        prior_attempt = self._max_archived_attempt(task_path)

        if self._task_dir_has_artifacts(task_path):
            archive_n = prior_attempt + 1
            archive_path = self._attempt_archive_path(task_path, archive_n)
            while archive_path.exists():
                archive_n += 1
                archive_path = self._attempt_archive_path(task_path, archive_n)
            task_path.rename(archive_path)
            self._write_json(
                archive_path / "attempt.json",
                {
                    "attempt": archive_n,
                    "archived_at": _utc_now(),
                    "artifact_key": key,
                    "archived_from": key,
                },
            )
            summary_name = f"{key.replace('/', '__')}_summary.json"
            summary_path = self.run_dir / summary_name
            if summary_path.is_file():
                shutil.move(str(summary_path), str(archive_path / summary_name))
            archived_previous = str(archive_path.relative_to(self.run_dir))
            prior_attempt = archive_n

        attempt = prior_attempt + 1
        fresh = self.task_dir(task_id)
        meta = {
            "attempt": attempt,
            "artifact_key": key,
            "prepared_at": _utc_now(),
            "archived_previous": archived_previous,
        }
        self._write_json(fresh / "attempt.json", meta)
        return meta

    @staticmethod
    def _task_dir_has_artifacts(task_path: Path) -> bool:
        if not task_path.is_dir():
            return False
        if any((task_path / name).is_file() for name in _TASK_ARTIFACT_MARKERS):
            return True
        turns = task_path / "turns"
        return turns.is_dir() and any(turns.iterdir())

    @staticmethod
    def _attempt_archive_path(task_path: Path, attempt: int) -> Path:
        return task_path.parent / f"{task_path.name}.attempt{attempt:02d}"

    @staticmethod
    def _max_archived_attempt(task_path: Path) -> int:
        parent = task_path.parent
        if not parent.is_dir():
            return 0
        prefix = f"{task_path.name}.attempt"
        max_n = 0
        for child in parent.iterdir():
            if not child.is_dir() or not child.name.startswith(prefix):
                continue
            match = _ATTEMPT_DIR_RE.match(child.name)
            if match and match.group("base") == task_path.name:
                max_n = max(max_n, int(match.group("n")))
        return max_n

    def record_initial_screen(self, task_id: str, *, observation) -> None:
        task_dir = self.task_dir(task_id)
        self._write_json(
            task_dir / "initial_screen.json",
            {
                "turn": observation.turn,
                "observation_mode": observation.mode,
                "text": observation.text,
                "plain_text": observation.plain_text,
                "svg_file": observation.svg_file,
                "llm_image_file": observation.image_relative,
                "recorded_at": _utc_now(),
            },
        )

    def append_turn_jsonl(self, task_id: str, turn: TurnRecord) -> None:
        path = self.task_dir(task_id) / "transcript.jsonl"
        self._append_jsonl_line(path, turn.to_dict())

    def append_keystroke_jsonl(self, task_id: str, keystroke: dict[str, Any]) -> None:
        path = self.task_dir(task_id) / "keystrokes.jsonl"
        self._append_jsonl_line(path, keystroke)

    def begin_task(
        self,
        task_id: str,
        *,
        description: str,
        agent_id: str,
    ) -> None:
        task_dir = self.task_dir(task_id)
        self._write_json(
            task_dir / "live.json",
            {
                "task_id": task_id,
                "status": "in_progress",
                "description": description,
                "agent_id": agent_id,
                "started_at": _utc_now(),
                "turn_count": 0,
                "run_dir": str(self.run_dir),
            },
        )

    def update_live_task(
        self,
        task_id: str,
        *,
        turn_count: int,
        last_turn: int | None = None,
        stop_reason: str | None = None,
        status: str = "in_progress",
    ) -> None:
        task_dir = self.task_dir(task_id)
        live_path = task_dir / "live.json"
        payload: dict[str, Any] = {
            "task_id": task_id,
            "status": status,
            "turn_count": turn_count,
            "updated_at": _utc_now(),
            "run_dir": str(self.run_dir),
        }
        if last_turn is not None:
            payload["last_turn"] = last_turn
        if stop_reason is not None:
            payload["stop_reason"] = stop_reason
        if live_path.is_file():
            try:
                existing = json.loads(live_path.read_text(encoding="utf-8"))
                if isinstance(existing, dict):
                    payload = {**existing, **payload}
            except json.JSONDecodeError:
                pass
        self._write_json(live_path, payload)

    def _task_summary_entry(self, result: TaskRunResult) -> dict[str, Any]:
        key = self.artifact_key(result.task_id)
        return {
            "task_id": result.task_id,
            "artifact_key": key,
            "project": self.project_dir.name,
            "passed": result.passed,
            "stop_reason": result.stop_reason,
            "turn_count": len(result.turns),
            "verify_line": result.verify.line,
            "token_usage": result.total_token_usage.to_dict(),
        }

    def _merge_summary_tasks(self, new_entries: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Keep prior suite tasks when appending projects into a shared run_dir."""
        by_key: dict[str, dict[str, Any]] = {}
        summary_path = self.run_dir / "summary.json"
        if summary_path.is_file():
            try:
                existing = json.loads(summary_path.read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                existing = None
            if isinstance(existing, dict):
                for item in existing.get("tasks") or []:
                    if not isinstance(item, dict):
                        continue
                    key = str(item.get("artifact_key") or item.get("task_id") or "")
                    if key:
                        by_key[key] = item
        for item in new_entries:
            key = str(item.get("artifact_key") or item.get("task_id") or "")
            if key:
                by_key[key] = item
        return list(by_key.values())

    def write_partial_summary(self, *, interrupted: bool = False) -> None:
        finished = _utc_now()
        tasks = self._merge_summary_tasks([self._task_summary_entry(t) for t in self._tasks])
        passed = sum(1 for t in tasks if t.get("passed"))
        summary = {
            "run_id": self.run_id,
            "run_dir": str(self.run_dir),
            "started_at": self._started_at,
            "finished_at": finished,
            "interrupted": interrupted,
            "task_count": len(tasks),
            "passed": passed,
            "failed": len(tasks) - passed,
            "tasks": tasks,
        }
        self._write_json(self.run_dir / "summary.json", summary)

    def record_task(
        self,
        result: TaskRunResult,
        *,
        artifact_task_id: str | None = None,
    ) -> None:
        self._tasks.append(result)
        record_key = self.artifact_key(artifact_task_id or result.task_id)
        task_dir = self.task_dir(record_key)
        self._write_json(task_dir / "result.json", result.to_dict())
        self._write_json(self.run_dir / f"{record_key.replace('/', '__')}_summary.json", {
            "task_id": result.task_id,
            "artifact_key": record_key,
            "project": self.project_dir.name,
            "passed": result.passed,
            "stop_reason": result.stop_reason,
            "turn_count": len(result.turns),
            "verify": result.verify.to_dict(),
            "total_token_usage": result.total_token_usage.to_dict(),
        })

    def finalize(self) -> dict[str, Any]:
        finished = _utc_now()
        tasks = self._merge_summary_tasks([self._task_summary_entry(t) for t in self._tasks])
        passed = sum(1 for t in tasks if t.get("passed"))
        total_usage = TokenUsage()
        for entry in tasks:
            usage = entry.get("token_usage") or {}
            if not isinstance(usage, dict):
                continue
            total_usage.prompt_tokens += int(usage.get("prompt_tokens") or 0)
            total_usage.completion_tokens += int(usage.get("completion_tokens") or 0)
            total_usage.total_tokens += int(usage.get("total_tokens") or 0)
            total_usage.reasoning_tokens += int(usage.get("reasoning_tokens") or 0)
            total_usage.estimated_reasoning_tokens += int(
                usage.get("estimated_reasoning_tokens") or 0
            )
            total_usage.cached_prompt_tokens += int(usage.get("cached_prompt_tokens") or 0)

        summary = {
            "run_id": self.run_id,
            "run_dir": str(self.run_dir),
            "started_at": self._started_at,
            "finished_at": finished,
            "duration_note": "see per-task started_at/ended_at for details",
            "task_count": len(tasks),
            "passed": passed,
            "failed": len(tasks) - passed,
            "total_token_usage": total_usage.to_dict(),
            "tasks": tasks,
        }
        self._write_json(self.run_dir / "summary.json", summary)
        return summary

    @staticmethod
    def _append_jsonl_line(path: Path, data: Any) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(data, ensure_ascii=False) + "\n")
            fh.flush()
            os.fsync(fh.fileno())

    @staticmethod
    def _write_json(path: Path, data: Any) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")
        with path.open("rb") as fh:
            os.fsync(fh.fileno())
