from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Any

from oracle.fingerprint import generate_fingerprints, load_fingerprints
from oracle.fingerprint_scope import resolve_fingerprint_scope
from oracle.result import CheckResult, OracleResult
from oracle.spec import ProjectSpec


class OracleRunner:
    """Load machine spec from bench.spec.json; run verification via oracle/Txx.sh."""

    def __init__(self, project_dir: Path):
        self.project_dir = Path(project_dir).resolve()
        self.spec_loader = ProjectSpec(self.project_dir)
        self.spec = self.spec_loader.spec
        self.spec_path = self.spec_loader.spec_path
        self.project_id = self.spec_loader.project_id

    def list_tasks(self) -> list[str]:
        return self.spec_loader.list_tasks()

    def get_task(self, task_id: str) -> dict[str, Any]:
        return self.spec_loader.get_task(task_id)

    def get_observation(self, task_id: str) -> str:
        return self.spec_loader.get_observation(task_id)

    def inject_description(self, task_id: str, fingerprints: dict[str, str]) -> str:
        return self.spec_loader.inject_description(task_id, fingerprints)

    def oracle_script(self, task_id: str) -> Path:
        path = self.project_dir / "oracle" / f"{task_id}.sh"
        if not path.is_file():
            raise FileNotFoundError(
                f"oracle script required: {path}\n"
                f"Verification logic must live in oracle/{task_id}.sh."
            )
        return path

    def fingerprint_path(self, task_id: str, run_id: str | None = None) -> Path:
        del run_id  # stable per task: .oracle/T01_fingerprints.json
        return self.project_dir / ".oracle" / f"{task_id}_fingerprints.json"

    def ensure_all_fingerprints(
        self,
        *,
        refresh: bool = False,
    ) -> dict[str, dict[str, str]]:
        out: dict[str, dict[str, str]] = {}
        for task_id in self.list_tasks():
            out[task_id] = self.ensure_fingerprints(task_id, refresh=refresh)
        return out

    def ensure_fingerprints(
        self,
        task_id: str,
        *,
        run_id: str | None = None,
        overrides: dict[str, str] | None = None,
        persist: bool = True,
        refresh: bool = False,
    ) -> dict[str, str]:
        path = self.fingerprint_path(task_id, run_id)
        if refresh and path.is_file():
            path.unlink()
        if path.is_file():
            return load_fingerprints(path)
        scope = resolve_fingerprint_scope(self.project_dir)
        fps = generate_fingerprints(
            project_id=self.project_id,
            task_id=task_id,
            run_id=run_id,
            overrides=overrides,
            scope=scope,
        )
        if persist:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(fps, indent=2, ensure_ascii=False) + "\n")
        return fps

    def run(
        self,
        task_id: str,
        *,
        run_id: str | None = None,
        extra_env: dict[str, str] | None = None,
        fingerprint_file: Path | None = None,
    ) -> OracleResult:
        task = self.get_task(task_id)
        observation = self.get_observation(task_id)
        script = self.oracle_script(task_id)
        fp_path = fingerprint_file or self.fingerprint_path(task_id, run_id)
        if not fp_path.is_file():
            self.ensure_fingerprints(task_id, run_id=run_id)

        eval_root = Path(__file__).resolve().parent.parent
        oracle_lib = eval_root / "scripts" / "oracle_lib.sh"

        env = os.environ.copy()
        env.update(
            {
                "ORACLE_PROJECT_DIR": str(self.project_dir),
                "ORACLE_TASK_ID": task_id,
                "ORACLE_OBSERVATION": observation,
                "ORACLE_FP_FILE": str(fp_path),
                "BENCH_PROJECT_ID": self.project_id,
                "BENCH_ORACLE_LIB": str(oracle_lib),
            }
        )
        if extra_env:
            env.update(extra_env)

        proc = subprocess.run(
            ["bash", str(script)],
            cwd=str(self.project_dir),
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        combined = (proc.stdout or "") + (proc.stderr or "")
        passed = proc.returncode == 0
        fingerprints: dict[str, str] = {}
        if fp_path.is_file():
            try:
                fingerprints = load_fingerprints(fp_path)
            except (json.JSONDecodeError, OSError):
                pass

        return OracleResult(
            project_id=self.project_id,
            task_id=task_id,
            passed=passed,
            checks=[
                CheckResult(
                    check_id=script.name,
                    check_type="oracle_sh",
                    passed=passed,
                    message="oracle script exited 0" if passed else f"oracle script failed (exit {proc.returncode})",
                    details={
                        "script": str(script),
                        "observation": observation,
                        "description": task.get("description", ""),
                        "run_id": run_id,
                        "output_tail": combined[-2000:] if combined else "",
                    },
                )
            ],
            fingerprints=fingerprints,
        )
