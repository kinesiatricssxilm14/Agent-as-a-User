from __future__ import annotations

import json
import os
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .container import container_running
from .models import TaskInfo, VerifyResult


class BenchError(RuntimeError):
    pass


class BenchClient:
    """Thin wrapper around evaluation/scripts/bench.sh."""

    def __init__(
        self,
        project_dir: str | Path,
        bench_script: str | Path | None = None,
    ) -> None:
        self.project_dir = Path(project_dir).resolve()
        if bench_script is None:
            repo_root = self.project_dir
            for _ in range(6):
                candidate = repo_root / "evaluation" / "scripts" / "bench.sh"
                if candidate.is_file():
                    bench_script = candidate
                    break
                if repo_root.parent == repo_root:
                    break
                repo_root = repo_root.parent
        if bench_script is None:
            env_bench = os.environ.get("BENCH")
            if env_bench and Path(env_bench).is_file():
                bench_script = env_bench
        if bench_script is None:
            raise BenchError(
                "Cannot find bench.sh. Set BENCH=/path/to/evaluation/scripts/bench.sh "
                "or pass bench_script= explicitly."
            )
        self.bench_script = Path(bench_script).resolve()
        if not self.bench_script.is_file():
            raise BenchError(f"bench.sh not found: {self.bench_script}")

    @property
    def env(self) -> dict[str, str]:
        merged = os.environ.copy()
        merged["BENCH_PROJECT"] = str(self.project_dir)
        return merged

    def _run(
        self,
        *cmd_args: str,
        capture: bool = True,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        cmd = [str(self.bench_script), *cmd_args]
        return subprocess.run(
            cmd,
            env=env if env is not None else self.env,
            text=True,
            capture_output=capture,
            check=False,
        )

    def build(self, *, rebuild: bool = False) -> None:
        args = ["build"]
        if rebuild:
            args.append("--rebuild")
        proc = self._run(*args, capture=False)
        if proc.returncode != 0:
            raise BenchError(f"bench build failed (exit {proc.returncode})")

    def start(self, task_id: str, *, rebuild: bool = False, no_build: bool = False) -> None:
        args = ["start", task_id, "--human"]
        if rebuild:
            args.append("--rebuild")
        if no_build:
            args.append("--no-build")
        proc = self._run(*args, capture=True)
        if proc.returncode != 0:
            detail = (proc.stderr or proc.stdout or "").strip()
            raise BenchError(f"bench start {task_id} failed (exit {proc.returncode}): {detail}")

    def start_all(self, *, rebuild: bool = False) -> None:
        args = ["start-all", "--human"]
        if rebuild:
            args.append("--rebuild")
        proc = self._run(*args, capture=True)
        if proc.returncode != 0:
            detail = (proc.stderr or proc.stdout or "").strip()
            raise BenchError(f"bench start-all failed (exit {proc.returncode}): {detail}")

    def task(self) -> dict[str, Any]:
        proc = self._run("task")
        if proc.returncode != 0:
            raise BenchError(proc.stderr.strip() or "bench task failed")
        return json.loads(proc.stdout)

    def verify(self, *, screen_snapshot: Path | None = None) -> VerifyResult:
        env = self.env
        if screen_snapshot is not None:
            snap = Path(screen_snapshot)
            if snap.is_file():
                env = {**env, "ORACLE_SCREEN_SNAPSHOT": str(snap.resolve())}
        proc = self._run("verify", "--human", "--quiet", env=env)
        stderr = proc.stderr.strip()
        stdout = proc.stdout.strip()
        blob = stderr or stdout
        match = re.search(r"\[(PASS|FAIL)\]\s+(\S+)\s+(\S+)\s+\(([^)]+)\)", blob)
        if not match:
            raise BenchError(f"Could not parse verify output: {blob!r}")
        status, project_id, task_id, obs = match.groups()
        details = [ln.strip() for ln in stderr.splitlines() if ln.strip().startswith("✗")]
        return VerifyResult(
            passed=status == "PASS",
            line=f"[{status}] {project_id} {task_id} ({obs})",
            details=details,
            recorded_at=datetime.now(timezone.utc).isoformat(),
        )

    def next(self) -> bool:
        proc = self._run("next", "--human", capture=True)
        return proc.returncode == 0

    def stop(self) -> None:
        self._run("stop", capture=False)

    def has_active_session(self) -> bool:
        return (self.project_dir / ".oracle" / "current_session.json").is_file()

    def current_session(self) -> dict[str, Any]:
        path = self.project_dir / ".oracle" / "current_session.json"
        if not path.is_file():
            raise BenchError("No active bench session. Run start-all or start first.")
        return json.loads(path.read_text())

    def active_task_id(self) -> str:
        return self.current_session()["active_task_id"]

    def task_manifest(self, task_id: str) -> dict[str, Any]:
        path = self.project_dir / ".oracle" / f"session_{task_id}.json"
        if not path.is_file():
            raise BenchError(f"Missing session file: {path}")
        return json.loads(path.read_text())

    def container_id(self, task_id: str) -> str:
        return str(self.task_manifest(task_id).get("container_id", ""))

    def session_id(self, task_id: str) -> str:
        return str(self.task_manifest(task_id).get("session_id", ""))

    def is_container_alive(self, task_id: str) -> bool:
        return container_running(self.container_id(task_id))

    def load_task_info(self, task_id: str | None = None) -> TaskInfo:
        payload = self.task()
        tid = task_id or payload["task_id"]
        manifest = self.task_manifest(tid)
        current = self.current_session()
        return TaskInfo.from_bench_task(
            payload,
            session_id=str(manifest.get("session_id", "")),
            project_id=current.get("project_id", ""),
            container_id=str(manifest.get("container_id", "")),
        )
