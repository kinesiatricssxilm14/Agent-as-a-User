from __future__ import annotations

import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable

from .agent_keystrokes import expand_action_to_keystrokes
from .agents import KeyboardAgent, create_agent
from .bench_client import BenchClient, BenchError
from .config import RunConfig
from .keyboard_driver import KeyboardDriver, TmuxSessionGoneError
from .models import StopReason, TaskRunResult, TokenUsage, TurnRecord
from .recorder import RunRecorder
from .startup_wait import StartupNotReadyError, load_startup_wait_config, wait_for_tui_ready


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _log_screen_text(observation) -> str:
    if observation.mode in ("plain", "semantic"):
        return observation.text or observation.plain_text
    return observation.plain_text


def _turn_record_from_observation(observation) -> dict:
    return {
        "screen_text": _log_screen_text(observation),
        "observation_mode": observation.mode,
        "svg_file": observation.svg_file,
        "llm_image_file": observation.image_relative,
    }


def _reasoning_from_turn(agent_turn) -> str | None:
    return getattr(agent_turn, "reasoning_content", None)


def _verify_screen_path(task_dir: Path) -> Path:
    return task_dir / "verify_screen.plain.txt"


def _persist_verify_screen(task_dir: Path, observation) -> None:
    """Keep last good plain frame for oracle check_screen after TUI quit."""
    text = (observation.plain_text or "").rstrip("\n")
    if not text.strip():
        return
    _verify_screen_path(task_dir).write_text(text + "\n", encoding="utf-8")


class SuiteRunner:
    """
    Observe-Act evaluation loop (ReAct-style action protocol, no LangChain).

    Each turn: observe screen → LLM picks one action → execute → record SVG + tokens → repeat.
    """

    def __init__(
        self,
        config: RunConfig,
        *,
        script_path: str | None = None,
        run_dir: Path | str | None = None,
        nest_project: bool | None = None,
    ) -> None:
        self.config = config
        self.script_path = script_path
        self.bench = BenchClient(config.project_dir, config.bench_script)
        config_snap = {
            "max_turns": config.max_turns,
            "observation_mode": config.observation_mode.value,
            "agent": config.agent,
            "llm_model": config.llm.model,
            "rebuild": config.rebuild,
            "snapshot_settle_sec": config.snapshot_settle_sec,
            "task_timeout_sec": config.task_timeout_sec,
            "max_consecutive_agent_errors": config.max_consecutive_agent_errors,
            "max_consecutive_action_errors": config.max_consecutive_action_errors,
        }
        shared = Path(run_dir).expanduser().resolve() if run_dir else None
        # Shared suite dirs nest as Pxx-slug/Txx so projects don't collide.
        if nest_project is None:
            nest_project = shared is not None
        prefix = config.project_dir.name if nest_project else None
        self.recorder = RunRecorder(
            config.output_dir,
            project_dir=config.project_dir,
            config_snapshot=config_snap,
            runner_label=config.llm.model if config.agent == "openai" else config.agent,
            observation_mode=config.observation_mode.value,
            run_dir=shared,
            artifact_prefix=prefix,
        )
        self._log: Callable[[str], None] = lambda msg: print(msg, file=sys.stderr)

    def run_suite(self) -> dict:
        try:
            self._log("Starting bench suite (start-all)...")
            self.bench.start_all(rebuild=self.config.rebuild)

            while True:
                try:
                    task_id = self.bench.active_task_id()
                except BenchError:
                    break

                current = self.bench.current_session()
                if (current.get("tasks") or {}).get(task_id, {}).get("status") == "done":
                    break

                self._log(f"=== Task {task_id} ===")
                result = self._run_one_task(task_id)
                self.recorder.record_task(result)
                self._log(f"{result.verify.line} (stop={result.stop_reason}, turns={len(result.turns)})")

                if not self.bench.next():
                    self._log("Suite complete.")
                    break

            return self.recorder.finalize()
        finally:
            self._log("Stopping bench session...")
            self.bench.stop()

    def run_current_task(self) -> TaskRunResult:
        task_id = self.bench.active_task_id()
        result = self._run_one_task(task_id)
        self.recorder.record_task(result)
        summary = self.recorder.finalize()
        self._log(f"Run artifacts: {summary['run_dir']}")
        return result

    def run_task(self, task_id: str) -> TaskRunResult:
        """One command: bench start → agent → verify → stop."""
        try:
            if self.bench.has_active_session():
                self._log("Stopping previous bench session...")
                self.bench.stop()
            self._log(f"Starting bench {task_id} (rebuild={self.config.rebuild})...")
            self.bench.start(task_id, rebuild=self.config.rebuild)
            self._log(f"=== Task {task_id} ===")
            result = self._run_one_task(task_id)
            self.recorder.record_task(result)
            summary = self.recorder.finalize()
            self._log(
                f"{result.verify.line} "
                f"(stop={result.stop_reason}, turns={len(result.turns)})"
            )
            self._log(f"Run artifacts: {summary['run_dir']}")
            return result
        finally:
            self._log("Stopping bench session...")
            self.bench.stop()

    def _commit_turn(
        self,
        *,
        task_id: str,
        turns: list[TurnRecord],
        record: TurnRecord,
        keystroke_seq: int,
        gap_before_ms: float | None,
    ) -> int:
        """Append transcript + matching keystrokes; return next keystroke seq."""
        action = dict(record.action)
        ks_rows = expand_action_to_keystrokes(
            action,
            seq_start=keystroke_seq,
            pressed_at=record.ended_at or record.started_at,
            interval_ms=gap_before_ms,
        )
        if ks_rows:
            action["human_keystrokes"] = ks_rows
            action["gap_before_ms"] = gap_before_ms
            record.action = action
            for row in ks_rows:
                self.recorder.append_keystroke_jsonl(task_id, row)
            keystroke_seq = ks_rows[-1]["seq"] + 1

        turns.append(record)
        self.recorder.append_turn_jsonl(task_id, record)
        self.recorder.update_live_task(
            task_id,
            turn_count=len(turns),
            last_turn=record.turn,
            status="in_progress",
        )
        return keystroke_seq

    def _observe_for_turn(
        self,
        *,
        driver: KeyboardDriver,
        turn: int,
        task_dir: Path,
        previous,
    ):
        """Capture turn screen; if TUI quit (e.g. q), reuse last good frame."""
        try:
            if not driver.session_alive():
                raise TmuxSessionGoneError(
                    f"tmux session gone ({driver.session_id})"
                )
            observation = driver.observe(turn, run_dir=self.recorder.run_dir)
            _persist_verify_screen(task_dir, observation)
            return observation, False
        except TmuxSessionGoneError as exc:
            self._log(
                f"  turn {turn}: TUI/session exited — keeping last screen for verify "
                f"({exc})"
            )
            return previous, True

    def _run_one_task(self, task_id: str) -> TaskRunResult:
        started_at = _utc_now()
        task = self.bench.load_task_info(task_id)
        task_dir = self.recorder.task_dir(task_id)
        turns_dir = task_dir / "turns"
        driver = KeyboardDriver(
            task.session_id,
            turns_dir,
            observation_mode=self.config.observation_mode,
            snapshot_settle_sec=self.config.snapshot_settle_sec,
        )
        agent = create_agent(
            self.config,
            task_id,
            run_dir=self.recorder.run_dir,
            script_path=self.script_path,
        )

        agent.begin_task(task)
        self.recorder.begin_task(
            task_id,
            description=task.description,
            agent_id=agent.agent_id,
        )

        turns: list[TurnRecord] = []
        stop_reason: StopReason = "agent_submit"
        total_usage = TokenUsage()
        task_started = time.perf_counter()
        consecutive_agent_errors = 0
        consecutive_action_errors = 0
        keystroke_seq = 1
        prev_action_ended_at: float | None = None

        startup_cfg = load_startup_wait_config(
            self.config.project_dir,
            self.config.startup_wait,
        )
        try:
            observation = wait_for_tui_ready(
                driver,
                run_dir=self.recorder.run_dir,
                config=startup_cfg,
                container_alive=lambda: self.bench.is_container_alive(task_id),
                log=self._log,
            )
        except StartupNotReadyError as exc:
            raise BenchError(str(exc)) from exc
        _persist_verify_screen(task_dir, observation)
        self.recorder.record_initial_screen(
            task_id,
            observation=observation,
        )

        for turn in range(1, self.config.max_turns + 1):
            if (
                self.config.task_timeout_sec is not None
                and time.perf_counter() - task_started > self.config.task_timeout_sec
            ):
                self._log(
                    "  task timeout "
                    f"after {self.config.task_timeout_sec:.1f}s — skipping task"
                )
                stop_reason = "task_timeout"
                break

            if not self.bench.is_container_alive(task_id):
                self._log(f"  turn {turn}: container exited — stopping agent loop")
                stop_reason = "container_exit"
                break

            if not driver.session_alive():
                self._log(
                    f"  turn {turn}: TUI/session already gone — stopping for verify"
                )
                stop_reason = "session_lost"
                break

            turn_started = time.perf_counter()
            turn_started_at = _utc_now()
            gap_before_ms = None
            if prev_action_ended_at is not None:
                gap_before_ms = (turn_started - prev_action_ended_at) * 1000.0

            agent_turn = agent.decide(observation)
            if agent_turn.token_usage:
                total_usage.prompt_tokens += agent_turn.token_usage.prompt_tokens
                total_usage.completion_tokens += agent_turn.token_usage.completion_tokens
                total_usage.total_tokens += agent_turn.token_usage.total_tokens
                total_usage.reasoning_tokens += agent_turn.token_usage.reasoning_tokens
                total_usage.estimated_reasoning_tokens += (
                    agent_turn.token_usage.estimated_reasoning_tokens
                )
                total_usage.cached_prompt_tokens += agent_turn.token_usage.cached_prompt_tokens

            # Always snapshot turn_NNN.svg for this transcript turn (human parity).
            # Errors / submit previously reused the prior frame and left holes (e.g. missing 002).
            session_gone = False
            if agent_turn.llm_error or agent_turn.parse_error or (
                agent_turn.action is not None and agent_turn.action.action == "submit"
            ):
                observation, session_gone = self._observe_for_turn(
                    driver=driver,
                    turn=turn,
                    task_dir=task_dir,
                    previous=observation,
                )
            # else: observe after execute below

            if agent_turn.llm_error:
                err = agent_turn.llm_error
                self._log(f"  turn {turn}: LLM error — {err}")
                record = TurnRecord(
                    turn=turn,
                    started_at=turn_started_at,
                    ended_at=_utc_now(),
                    duration_ms=(time.perf_counter() - turn_started) * 1000.0,
                    llm_latency_ms=agent_turn.llm_latency_ms,
                    token_usage=agent_turn.token_usage,
                    action={"action": "llm_error"},
                    action_ok=False,
                    action_error=err,
                    has_changed=None,
                    raw_agent_response=agent_turn.raw_response,
                    container_alive=self.bench.is_container_alive(task_id),
                    reasoning_content=_reasoning_from_turn(agent_turn),
                    **_turn_record_from_observation(observation),
                )
                keystroke_seq = self._commit_turn(
                    task_id=task_id,
                    turns=turns,
                    record=record,
                    keystroke_seq=keystroke_seq,
                    gap_before_ms=gap_before_ms,
                )
                prev_action_ended_at = time.perf_counter()
                if hasattr(agent, "record_action_feedback"):
                    agent.record_action_feedback(
                        ok=False,
                        error=err,
                        has_changed=None,
                        raw_response=agent_turn.raw_response or "{}",
                    )
                if session_gone:
                    stop_reason = "session_lost"
                    break
                consecutive_agent_errors += 1
                if consecutive_agent_errors >= self.config.max_consecutive_agent_errors:
                    self._log(
                        "  reached max_consecutive_agent_errors="
                        f"{self.config.max_consecutive_agent_errors} — skipping task"
                    )
                    stop_reason = "agent_error_limit"
                    break
                continue

            if agent_turn.parse_error:
                err = agent_turn.parse_error
                self._log(f"  turn {turn}: invalid action JSON — {err}")
                record = TurnRecord(
                    turn=turn,
                    started_at=turn_started_at,
                    ended_at=_utc_now(),
                    duration_ms=(time.perf_counter() - turn_started) * 1000.0,
                    llm_latency_ms=agent_turn.llm_latency_ms,
                    token_usage=agent_turn.token_usage,
                    action={"action": "parse_error", "raw": agent_turn.raw_response},
                    action_ok=False,
                    action_error=err,
                    has_changed=None,
                    raw_agent_response=agent_turn.raw_response,
                    container_alive=self.bench.is_container_alive(task_id),
                    reasoning_content=_reasoning_from_turn(agent_turn),
                    **_turn_record_from_observation(observation),
                )
                keystroke_seq = self._commit_turn(
                    task_id=task_id,
                    turns=turns,
                    record=record,
                    keystroke_seq=keystroke_seq,
                    gap_before_ms=gap_before_ms,
                )
                prev_action_ended_at = time.perf_counter()
                if hasattr(agent, "record_action_feedback"):
                    agent.record_action_feedback(
                        ok=False,
                        error=err,
                        has_changed=None,
                        raw_response=agent_turn.raw_response,
                    )
                if session_gone:
                    stop_reason = "session_lost"
                    break
                consecutive_agent_errors += 1
                if consecutive_agent_errors >= self.config.max_consecutive_agent_errors:
                    self._log(
                        "  reached max_consecutive_agent_errors="
                        f"{self.config.max_consecutive_agent_errors} — skipping task"
                    )
                    stop_reason = "agent_error_limit"
                    break
                continue

            action = agent_turn.action
            assert action is not None
            consecutive_agent_errors = 0

            if action.action == "submit":
                # observation already refreshed to turn_NNN.svg above
                record = TurnRecord(
                    turn=turn,
                    started_at=turn_started_at,
                    ended_at=_utc_now(),
                    duration_ms=(time.perf_counter() - turn_started) * 1000.0,
                    llm_latency_ms=agent_turn.llm_latency_ms,
                    token_usage=agent_turn.token_usage,
                    action=action.to_dict(),
                    action_ok=True,
                    action_error=None,
                    has_changed=None,
                    raw_agent_response=agent_turn.raw_response,
                    container_alive=True,
                    reasoning_content=_reasoning_from_turn(agent_turn),
                    **_turn_record_from_observation(observation),
                )
                keystroke_seq = self._commit_turn(
                    task_id=task_id,
                    turns=turns,
                    record=record,
                    keystroke_seq=keystroke_seq,
                    gap_before_ms=gap_before_ms,
                )
                stop_reason = "session_lost" if session_gone else "agent_submit"
                break

            exec_result = driver.execute(action)
            ok = bool(exec_result.get("success"))
            error = exec_result.get("error")
            has_changed = exec_result.get("has_changed")

            observation, session_gone = self._observe_for_turn(
                driver=driver,
                turn=turn,
                task_dir=task_dir,
                previous=observation,
            )
            if session_gone and error is None:
                error = "tui_session_exited"
            turn_ended_at = _utc_now()
            record = TurnRecord(
                turn=turn,
                started_at=turn_started_at,
                ended_at=turn_ended_at,
                duration_ms=(time.perf_counter() - turn_started) * 1000.0,
                llm_latency_ms=agent_turn.llm_latency_ms,
                token_usage=agent_turn.token_usage,
                action=action.to_dict(),
                action_ok=ok,
                action_error=str(error) if error else None,
                has_changed=has_changed,
                raw_agent_response=agent_turn.raw_response,
                container_alive=self.bench.is_container_alive(task_id),
                reasoning_content=_reasoning_from_turn(agent_turn),
                **_turn_record_from_observation(observation),
            )
            keystroke_seq = self._commit_turn(
                task_id=task_id,
                turns=turns,
                record=record,
                keystroke_seq=keystroke_seq,
                gap_before_ms=gap_before_ms,
            )
            prev_action_ended_at = time.perf_counter()

            agent.record_action_feedback(
                ok=ok,
                error=str(error) if error else None,
                has_changed=has_changed,
                raw_response=agent_turn.raw_response,
            )

            if session_gone:
                self._log(
                    f"  turn {turn}: TUI exited after action — verifying last screen"
                )
                stop_reason = "session_lost"
                break

            if not record.container_alive:
                self._log(f"  turn {turn}: container exited after action")
                stop_reason = "container_exit"
                break

            if not ok:
                self._log(f"  turn {turn}: action failed — {error}")
                consecutive_action_errors += 1
                if (
                    consecutive_action_errors
                    >= self.config.max_consecutive_action_errors
                ):
                    self._log(
                        "  reached max_consecutive_action_errors="
                        f"{self.config.max_consecutive_action_errors} — skipping task"
                    )
                    stop_reason = "action_error_limit"
                    break
            else:
                consecutive_action_errors = 0

        else:
            stop_reason = "max_turns"
            self._log(f"  reached max_turns={self.config.max_turns}")

        self.recorder.update_live_task(
            task_id,
            turn_count=len(turns),
            last_turn=turns[-1].turn if turns else None,
            stop_reason=stop_reason,
            status="finished",
        )

        screen_snap = _verify_screen_path(task_dir)
        if screen_snap.is_file():
            self._log(f"  verify using saved screen: {screen_snap.name}")
            verify = self.bench.verify(screen_snapshot=screen_snap)
        else:
            verify = self.bench.verify()
        ended_at = _utc_now()

        return TaskRunResult(
            task_id=task.task_id,
            description=task.description,
            observation=task.observation,
            passed=verify.passed,
            verify=verify,
            turns=turns,
            stop_reason=stop_reason,
            agent_id=agent.agent_id,
            started_at=started_at,
            ended_at=ended_at,
            total_token_usage=total_usage,
        )
