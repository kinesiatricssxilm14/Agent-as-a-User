from __future__ import annotations

import json
import random
import sys
from pathlib import Path
from typing import Callable

from ..config import RunConfig
from ..recorder import RunRecorder, sanitize_run_id_part
from .bench_cleanup import kill_bench_tmux_sessions
from .harness_log import HarnessLogger
from .runner import HumanRunConfig, HumanSuiteRunner
from .suite_scheduler import (
    SuiteScheduleState,
    artifact_task_key,
    failed_task_refs,
    init_suite_state,
    pick_next_project,
    remaining_presentation_order,
    restore_rng,
    save_suite_state,
)
from .suite_warm_pool import SuiteLookaheadWarmer


class InterleavedSuiteRunner:
    """Run a multi-project suite with per-project task order and random project picks."""

    def __init__(
        self,
        *,
        suite_root: Path,
        bench_script: Path,
        output_dir: Path,
        human: HumanRunConfig,
        observation_mode: str,
        rebuild: bool,
        max_turns: int = 200,
        seed: int | None = None,
        state: SuiteScheduleState | None = None,
        state_path: Path | None = None,
        log: Callable[[str], None] | None = None,
        lookahead: int = 3,
        confirm_between_tasks: bool = True,
        retry_failed_only: bool = False,
    ) -> None:
        self.suite_root = suite_root.expanduser().resolve()
        self.bench_script = bench_script
        self.output_dir = output_dir.expanduser().resolve()
        self.human = human
        self.observation_mode = observation_mode
        self.rebuild = rebuild
        self.max_turns = max_turns
        self.state_path = state_path
        self.lookahead = max(0, lookahead)
        self.confirm_between_tasks = confirm_between_tasks
        self.retry_failed_only = retry_failed_only

        if state is None:
            if seed is None:
                seed = random.randint(0, 2**31 - 1)
            self.state = init_suite_state(
                self.suite_root,
                operator_id=human.operator_id,
                seed=seed,
            )
        else:
            self.state = state

        self._rng = self._restore_rng(self.state)
        self.recorder = self._build_recorder()
        self.harness = HarnessLogger(self.recorder.run_dir / "harness.log")
        self._log = log or self.harness.user
        self._warmer: SuiteLookaheadWarmer | None = None
        if self.lookahead > 0 and not self.rebuild:
            self._warmer = SuiteLookaheadWarmer(
                suite_root=self.suite_root,
                bench_script=self.bench_script,
                output_dir=self.output_dir,
                log=self.harness.diag,
            )

        self._retry_queue: list[tuple[str, str]] = []
        self._retry_done: set[tuple[str, str]] = set()
        if self.retry_failed_only:
            self._retry_queue = failed_task_refs(self.state)
            if not self._retry_queue:
                raise ValueError("English-only text FAIL English-only text（failed_task_refs English-only text）")

    @staticmethod
    def _restore_rng(state: SuiteScheduleState) -> random.Random:
        return restore_rng(state)

    def _build_recorder(self) -> RunRecorder:
        if self.state.run_dir:
            run_dir = Path(self.state.run_dir)
            run_id = run_dir.name
            run_dir.mkdir(parents=True, exist_ok=True)
            recorder = RunRecorder.__new__(RunRecorder)
            recorder.run_id = run_id
            recorder.run_dir = run_dir
            recorder.project_dir = self.suite_root
            recorder._tasks = []
            recorder._started_at = self.state.started_at
            recorder._meta = {
                "run_id": run_id,
                "started_at": self.state.started_at,
                "project_dir": str(self.suite_root),
                "framework": "human-record",
                "config": self._recorder_config(),
            }
            meta_path = run_dir / "meta.json"
            if not meta_path.is_file():
                recorder._write_json(meta_path, recorder._meta)
            return recorder

        label = sanitize_run_id_part(self.human.operator_id) or "human"
        suite_label = sanitize_run_id_part(self.suite_root.name) or "suite"
        suffix = "-retry" if self.retry_failed_only else ""
        recorder = RunRecorder(
            self.output_dir,
            project_dir=self.suite_root,
            config_snapshot=self._recorder_config(),
            framework="human-record",
            runner_label=f"{label}-{suite_label}{suffix}",
            observation_mode=self.observation_mode,
        )
        if not self.retry_failed_only:
            self.state.run_dir = str(recorder.run_dir)
            self._persist_state()
        return recorder

    def _recorder_config(self) -> dict:
        return {
            "framework": "human-record-interleaved",
            "suite_root": str(self.suite_root),
            "operator_id": self.human.operator_id,
            "seed": self.state.seed,
            "observation_mode": self.observation_mode,
            "type_idle_ms": self.human.type_idle_ms,
            "submit_key": self.human.submit_key,
            "rebuild": self.rebuild,
            "lookahead": self.lookahead,
            "retry_failed_only": self.retry_failed_only,
        }

    def _persist_state(self) -> None:
        if self.state_path is not None:
            save_suite_state(self.state, self.state_path)

    def _upcoming_tasks(self) -> list[tuple[str, str]]:
        if self.retry_failed_only:
            return [
                ref for ref in self._retry_queue if ref not in self._retry_done
            ]
        return remaining_presentation_order(self.state)

    def _warm_lookahead(self, upcoming: list[tuple[str, str]]) -> None:
        if self._warmer is None or not upcoming:
            return
        self._warmer.ensure_warming(upcoming[: self.lookahead])

    def _print_preview(self, upcoming: list[tuple[str, str]]) -> None:
        if not upcoming:
            return
        self._log("\n📋 English-only text（English-only text）:")
        for idx, (project, task_id) in enumerate(upcoming[: max(self.lookahead, 3)], start=1):
            self._log(f"   {idx}. {project} {task_id}")
        if len(upcoming) > max(self.lookahead, 3):
            self._log(f"   … English-only text {len(upcoming) - max(self.lookahead, 3)} English-only text")

    def _feedback_and_confirm(
        self,
        *,
        project: str,
        task_id: str,
        passed: bool,
        verify_line: str,
        upcoming: list[tuple[str, str]],
    ) -> bool:
        if passed:
            status = "✅ PASS"
        elif "ABORT" in verify_line or "session lost" in verify_line.lower():
            status = "⚠️ ABORT"
        else:
            status = "❌ FAIL"
        self._log(f"\n{status}  {project} {task_id}")
        self._log(f"    {verify_line}")
        if "ABORT" in verify_line or "session lost" in verify_line.lower():
            self._log("    （English-only text，English-only text；English-only text retry-failed English-only text）")
        if upcoming:
            nxt_project, nxt_task = upcoming[0]
            self._log(f"    English-only text: {nxt_project} {nxt_task}")
        else:
            self._log("    （English-only text）")
        if not self.confirm_between_tasks:
            return True
        self._log("\n  [Enter] English-only text    [q] English-only text")
        try:
            choice = input("> ").strip().lower()
        except EOFError:
            return True
        if choice in {"q", "quit", "exit", "n", "no", "English-only text"}:
            return False
        return True

    def _cleanup_bench_sessions(self) -> None:
        if self._warmer is not None:
            self._warmer.shutdown()
        kill_bench_tmux_sessions(log=self.harness.diag)

    def _record_completion(
        self,
        *,
        project_name: str,
        task_id: str,
        artifact_key: str,
        passed: bool,
        verify_line: str = "",
        stop_reason: str = "",
        turn_count: int = 0,
        error: str | None = None,
        retry_attempt: bool = False,
        attempt: int | None = None,
        archived_previous: str | None = None,
    ) -> None:
        entry = {
            "project": project_name,
            "task_id": task_id,
            "artifact_key": artifact_key,
            "passed": passed,
            "verify_line": verify_line,
            "stop_reason": stop_reason,
            "turn_count": turn_count,
        }
        if attempt is not None:
            entry["attempt"] = attempt
        if archived_previous:
            entry["archived_previous"] = archived_previous
        if error:
            entry["error"] = error
        if retry_attempt:
            entry["retry_attempt"] = True
            entry["retry_passed"] = passed
        self.state.completed.append(entry)
        self._persist_state()

    def _read_attempt_meta(self, artifact_key: str) -> dict:
        path = self.recorder.run_dir / artifact_key / "attempt.json"
        if not path.is_file():
            return {}
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {}
        return data if isinstance(data, dict) else {}

    def run(self, *, interface: str = "terminal", continue_on_error: bool = False) -> dict:
        self._log(f"Suite root: {self.suite_root}")
        self._log(f"Operator: {self.human.operator_id}")
        self._log(f"Seed: {self.state.seed}")
        self._log(f"Tasks: {self.state.completed_count}/{self.state.total_tasks} done")
        self._log(f"Run logs: {self.recorder.run_dir}")
        if self.state_path:
            self._log(f"State file: {self.state_path}")
        if self.retry_failed_only:
            self._log(f"Retry mode: {len(self._retry_queue)} failed task(s)")

        upcoming = self._upcoming_tasks()
        self._print_preview(upcoming)
        self._warm_lookahead(upcoming)

        try:
            if self.retry_failed_only:
                self._run_retry_queue(interface=interface, continue_on_error=continue_on_error)
            else:
                self._run_interleaved(interface=interface, continue_on_error=continue_on_error)
        finally:
            if self._warmer is not None:
                self._warmer.shutdown()

        return self._finalize_summary()

    def _run_interleaved(self, *, interface: str, continue_on_error: bool) -> None:
        while True:
            upcoming = self._upcoming_tasks()
            if not upcoming:
                self._log("Suite complete.")
                break

            picked = pick_next_project(self.state, self._rng)
            if picked is None:
                self._log("Suite complete.")
                break

            project_name = picked.project
            task_id = picked.advance()
            if not self._run_single_task(
                project_name,
                task_id,
                interface=interface,
                continue_on_error=continue_on_error,
                retry_attempt=False,
            ):
                self._cleanup_bench_sessions()
                self._log("\nEnglish-only text，English-only text --resume English-only text。")
                break

            upcoming = self._upcoming_tasks()
            self._warm_lookahead(upcoming)

    def _run_retry_queue(self, *, interface: str, continue_on_error: bool) -> None:
        for project_name, task_id in self._retry_queue:
            if (project_name, task_id) in self._retry_done:
                continue
            if not self._run_single_task(
                project_name,
                task_id,
                interface=interface,
                continue_on_error=continue_on_error,
                retry_attempt=True,
            ):
                self._cleanup_bench_sessions()
                self._log("\nEnglish-only text，English-only text --resume English-only text。")
                break
            self._retry_done.add((project_name, task_id))
            upcoming = self._upcoming_tasks()
            self._warm_lookahead(upcoming)
        self._log("Retry queue complete.")

    def _run_single_task(
        self,
        project_name: str,
        task_id: str,
        *,
        interface: str,
        continue_on_error: bool,
        retry_attempt: bool,
    ) -> bool:
        artifact_key = artifact_task_key(project_name, task_id)
        ordinal = self.state.completed_count + 1
        upcoming = self._upcoming_tasks()
        # Remove current from upcoming preview for confirm line
        rest = [(p, t) for p, t in upcoming if not (p == project_name and t == task_id)]

        self.harness.diag(
            f"=== [{ordinal}/{self.state.total_tasks}] {project_name} {task_id} ==="
        )

        project_dir = self.suite_root / project_name
        cfg = RunConfig(
            project_dir=project_dir,
            bench_script=self.bench_script,
            output_dir=self.output_dir,
            max_turns=self.max_turns,
            observation_mode=self._observation_mode_enum(),
            rebuild=self.rebuild,
        )
        runner = HumanSuiteRunner(
            cfg,
            human=self.human,
            recorder=self.recorder,
            harness=self.harness,
        )

        checkout = None
        if self._warmer is not None:
            checkout = self._warmer.claim(project_name, task_id)
            if checkout is not None:
                self.harness.diag(f"✅ English-only text slot-{checkout.slot.slot_id}")

        try:
            result = runner.run_task(
                task_id,
                interface=interface,
                artifact_task_id=artifact_key,
                checkout=checkout,
            )
            attempt_meta = self._read_attempt_meta(artifact_key)
            self._record_completion(
                project_name=project_name,
                task_id=task_id,
                artifact_key=artifact_key,
                passed=result.passed,
                verify_line=result.verify.line,
                stop_reason=result.stop_reason,
                turn_count=len(result.turns),
                retry_attempt=retry_attempt,
                attempt=attempt_meta.get("attempt"),
                archived_previous=attempt_meta.get("archived_previous"),
            )
            return self._feedback_and_confirm(
                project=project_name,
                task_id=task_id,
                passed=result.passed,
                verify_line=result.verify.line,
                upcoming=rest,
            )
        except Exception as exc:
            self._log(f"Task error: {project_name} {task_id}: {exc}")
            attempt_meta = self._read_attempt_meta(artifact_key)
            self._record_completion(
                project_name=project_name,
                task_id=task_id,
                artifact_key=artifact_key,
                passed=False,
                error=str(exc),
                retry_attempt=retry_attempt,
                attempt=attempt_meta.get("attempt"),
                archived_previous=attempt_meta.get("archived_previous"),
            )
            if not self._feedback_and_confirm(
                project=project_name,
                task_id=task_id,
                passed=False,
                verify_line=str(exc),
                upcoming=rest,
            ):
                return False
            if not continue_on_error:
                raise
            return True
        finally:
            if self._warmer is not None:
                self._warmer.release_used(project_name, task_id)

    def _finalize_summary(self) -> dict:
        failed = failed_task_refs(self.state)
        summary = self.recorder.finalize()
        suite_summary = {
            **summary,
            "suite_root": str(self.suite_root),
            "operator_id": self.human.operator_id,
            "seed": self.state.seed,
            "presentation": "retry-failed" if self.retry_failed_only else "interleaved-random-frontier",
            "state_file": str(self.state_path) if self.state_path else None,
            "schedule": self.state.completed,
            "failed_tasks": [
                {"project": p, "task_id": t} for p, t in failed
            ],
            "failed_count": len(failed),
        }
        self.recorder._write_json(self.recorder.run_dir / "suite_summary.json", suite_summary)
        self.recorder._write_json(
            self.recorder.run_dir / "failed_tasks.json",
            {"failed": suite_summary["failed_tasks"], "count": len(failed)},
        )
        self._persist_state()

        self._log("\n========== English-only text ==========")
        self._log(f"English-only text: {summary['passed']}/{summary['task_count']}")
        if failed:
            self._log(f"❌ FAIL English-only text {len(failed)} English-only text:")
            for project, task_id in failed:
                self._log(f"   - {project} {task_id}")
            self._log("\nEnglish-only text FAIL: keyboard-agent human run-suite --retry-failed --operator "
                      f"{self.human.operator_id}")
        else:
            self._log("🎉 English-only text！")
        return suite_summary

    def _observation_mode_enum(self):
        from ..models import ObservationMode

        return ObservationMode(self.observation_mode)
