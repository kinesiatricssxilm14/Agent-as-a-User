from __future__ import annotations

import signal
import sys
from contextlib import nullcontext
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Callable

from dataclasses import replace

from ..bench_client import BenchClient, BenchError
from ..config import RunConfig
from ..keyboard_driver import KeyboardDriver
from ..models import StopReason, TaskRunResult, TokenUsage, TurnRecord
from ..recorder import RunRecorder, sanitize_run_id_part
from ..startup_wait import StartupNotReadyError, load_startup_wait_config, wait_for_tui_ready
from .harness_log import HarnessLogger
from .session import HumanSession
from .startup_status import (
    PHASE_ACTIVE,
    PHASE_READY,
    PHASE_STARTING,
    PHASE_STOPPING,
    PHASE_WAITING_TUI,
    StartupStatus,
    ready_message,
)
from .key_classifier import is_type_key
from .turn_recorder import HumanOperation, HumanTurnRecorder, KeystrokeRecord
from .warm_pool import PoolCheckout
from .web_server import run_startup_web_server, run_web_session


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


def _keystroke_to_action(record: KeystrokeRecord) -> dict:
    payload = record.to_dict()
    if is_type_key(record.key):
        return {
            "action": "type",
            "text": record.key,
            "human_keystrokes": [payload],
            "gap_before_ms": record.interval_ms,
        }
    return {
        "action": "press",
        "key": record.key,
        "human_keystrokes": [payload],
        "gap_before_ms": record.interval_ms,
    }


@dataclass
class HumanRunConfig:
    type_idle_ms: float = 1000.0
    submit_key: str = "ctrl+g ctrl+g ctrl+g"
    operator_id: str = "human"
    web_host: str = "127.0.0.1"
    web_port: int = 8765
    open_browser: bool = True
    task_pane_percent: int = -1

    @property
    def idle_threshold_ms(self) -> float:
        return self.type_idle_ms


@dataclass
class _ActiveTaskState:
    task_id: str
    record_key: str
    started_at: str
    turns: list[TurnRecord]
    turn_recorder: HumanTurnRecorder | None = None


class HumanSuiteRunner:
    """Bench suite driver for human operators with keystroke + operation recording."""

    def __init__(
        self,
        config: RunConfig,
        *,
        human: HumanRunConfig | None = None,
        recorder: RunRecorder | None = None,
        harness: HarnessLogger | None = None,
    ) -> None:
        self.config = config
        self.human = human or HumanRunConfig()
        self.bench = BenchClient(config.project_dir, config.bench_script)
        config_snap = {
            "max_turns": config.max_turns,
            "observation_mode": config.observation_mode.value,
            "framework": "human-record",
            "type_idle_ms": self.human.type_idle_ms,
            "submit_key": self.human.submit_key,
            "operator_id": self.human.operator_id,
            "rebuild": config.rebuild,
        }
        self.recorder = recorder or RunRecorder(
            config.output_dir,
            project_dir=config.project_dir,
            config_snapshot=config_snap,
            framework="human-record",
            runner_label=sanitize_run_id_part(self.human.operator_id) or "human",
            observation_mode=config.observation_mode.value,
        )
        self._log: Callable[[str], None] = (
            harness.user if harness is not None else (lambda msg: print(msg, file=sys.stderr))
        )
        self.harness = harness
        self._diag_log: Callable[[str], None] = (
            harness.diag if harness is not None else (lambda _msg: None)
        )
        self._active: _ActiveTaskState | None = None
        self._task_bench: BenchClient = self.bench
        self._previous_sigint = signal.getsignal(signal.SIGINT)

    def _install_interrupt_handler(self) -> None:
        signal.signal(signal.SIGINT, self._handle_interrupt)

    def _restore_interrupt_handler(self) -> None:
        signal.signal(signal.SIGINT, self._previous_sigint)

    def _handle_interrupt(self, signum, frame) -> None:
        self._log("\nInterrupted — saving progress...")
        active = self._active
        if active is not None:
            if active.turn_recorder is not None:
                active.turn_recorder.flush()
                active.turn_recorder.join_snapshots(timeout=5.0)
            self.recorder.update_live_task(
                active.record_key,
                turn_count=len(active.turns),
                status="interrupted",
                stop_reason="interrupted",
            )
        self.recorder.write_partial_summary(interrupted=True)
        self._restore_interrupt_handler()
        raise KeyboardInterrupt

    def run_suite(self, *, interface: str = "terminal") -> dict:
        self._install_interrupt_handler()
        try:
            return self._run_suite_body(interface=interface)
        finally:
            self._active = None
            self._restore_interrupt_handler()

    def _run_suite_body(self, *, interface: str = "terminal") -> dict:
        self._diag_log(f"Run logs: {self.recorder.run_dir}")
        try:
            if self.bench.has_active_session():
                self._diag_log("⏳ English-only text benchmark English-only text…")
                self.bench.stop()
            self._diag_log("⏳ English-only text benchmark English-only text（English-only text 1–2 English-only text）…")
            self.bench.start_all(rebuild=self.config.rebuild)
            self._diag_log("✅ English-only text")

            while True:
                try:
                    task_id = self.bench.active_task_id()
                except BenchError:
                    break

                current = self.bench.current_session()
                if (current.get("tasks") or {}).get(task_id, {}).get("status") == "done":
                    break

                self._diag_log(f"=== Task {task_id} ===")
                result = self._run_one_task(task_id, interface=interface)
                self.recorder.record_task(result)
                self._diag_log(
                    f"{result.verify.line} "
                    f"(stop={result.stop_reason}, operations={len(result.turns)})"
                )

                if not self.bench.next():
                    self._log("Suite complete.")
                    break

            return self.recorder.finalize()
        finally:
            self._diag_log("Stopping bench session...")
            self.bench.stop()

    def run_task(
        self,
        task_id: str,
        *,
        interface: str = "terminal",
        checkout: PoolCheckout | None = None,
        artifact_task_id: str | None = None,
    ) -> TaskRunResult:
        self._install_interrupt_handler()
        try:
            return self._run_task_body(
                task_id,
                interface=interface,
                checkout=checkout,
                artifact_task_id=artifact_task_id,
            )
        finally:
            self._active = None
            self._restore_interrupt_handler()

    def _run_task_body(
        self,
        task_id: str,
        *,
        interface: str = "terminal",
        checkout: PoolCheckout | None = None,
        artifact_task_id: str | None = None,
    ) -> TaskRunResult:
        pooled = checkout is not None
        bench = checkout.bench if checkout else self.bench
        human_cfg = self.human
        if checkout is not None:
            human_cfg = replace(self.human, web_port=checkout.web_port)

        self._diag_log(f"Run logs: {self.recorder.run_dir}")
        startup_status: StartupStatus | None = None
        startup_server = None
        if interface == "web":
            startup_status = StartupStatus()
            startup_server = run_startup_web_server(
                status=startup_status,
                host=human_cfg.web_host,
                port=human_cfg.web_port,
                open_browser=human_cfg.open_browser,
                log=self._log,
            )
        try:
            if not pooled:
                if bench.has_active_session():
                    msg = "⏳ English-only text benchmark English-only text…"
                    self._diag_log(msg)
                    if startup_status is not None:
                        startup_status.set(PHASE_STOPPING, msg, task_id=task_id)
                    bench.stop()
                msg = f"⏳ English-only text {task_id}（English-only text 1–2 English-only text）…"
                self._diag_log(msg)
                if startup_status is not None:
                    startup_status.set(PHASE_STARTING, msg, task_id=task_id)
                bench.start(task_id, rebuild=self.config.rebuild)
                self._diag_log("✅ English-only text")
            else:
                if not bench.has_active_session():
                    self._diag_log("⚠ English-only text slot English-only text bench session，English-only text…")
                    bench.start(task_id, no_build=True)
                elif not bench.is_container_alive(task_id):
                    self._diag_log("⚠ English-only text，English-only text…")
                    bench.stop()
                    bench.start(task_id, no_build=True)
                self._diag_log(f"✅ English-only text slot-{checkout.slot.slot_id}（English-only text）")
                if startup_status is not None:
                    startup_status.set(
                        PHASE_READY,
                        ready_message(),
                        task_id=task_id,
                    )
            self._diag_log(f"=== Task {task_id} ===")
            result = self._run_one_task(
                task_id,
                interface=interface,
                startup_status=startup_status,
                startup_server=startup_server,
                bench=bench,
                human=human_cfg,
                skip_tui_wait=pooled,
                artifact_task_id=artifact_task_id,
            )
            self.recorder.record_task(result, artifact_task_id=artifact_task_id)
            summary = self.recorder.finalize()
            self._diag_log(
                f"{result.verify.line} "
                f"(stop={result.stop_reason}, operations={len(result.turns)})"
            )
            self._diag_log(f"Run artifacts: {summary['run_dir']}")
            return result
        finally:
            if not pooled:
                self._diag_log("Stopping bench session...")
                bench.stop()

    def _run_one_task(
        self,
        task_id: str,
        *,
        interface: str = "terminal",
        startup_status: StartupStatus | None = None,
        startup_server=None,
        bench: BenchClient | None = None,
        human: HumanRunConfig | None = None,
        skip_tui_wait: bool = False,
        artifact_task_id: str | None = None,
    ) -> TaskRunResult:
        bench = bench or self.bench
        human = human or self.human
        self._task_bench = bench
        started_at = _utc_now()
        task = bench.load_task_info(task_id)
        record_key = artifact_task_id or task_id
        prep = self.recorder.prepare_task_dir(record_key)
        if prep.get("archived_previous"):
            self._diag_log(
                f"Archived prior attempt → {prep['archived_previous']} "
                f"(now attempt {prep['attempt']})"
            )
        task_dir = self.recorder.task_dir(record_key)
        turns_dir = task_dir / "turns"
        driver = KeyboardDriver(
            task.session_id,
            turns_dir,
            observation_mode=self.config.observation_mode,
        )

        self._diag_log(f"Task: {task.description}")
        self._diag_log(f"Session: {task.session_id}")
        self._diag_log(f"Run logs: {self.recorder.run_dir}")

        if startup_status is not None:
            startup_status.set(
                PHASE_WAITING_TUI,
                "⏳ English-only text TUI English-only text…",
                task_id=task.task_id,
                description=task.description,
            )
        elif not skip_tui_wait:
            self._diag_log("⏳ English-only text TUI English-only text…")

        startup_cfg = load_startup_wait_config(
            self.config.project_dir,
            self.config.startup_wait,
        )
        fast_startup = replace(startup_cfg, stable_sec=0.05, poll_interval_sec=0.05)
        alive = lambda: bench.is_container_alive(task_id)
        quiet_ctx = (
            self.harness.attach_quiet()
            if self.harness is not None and interface == "terminal"
            else nullcontext()
        )
        with quiet_ctx:
            if skip_tui_wait:
                observation = driver.observe(0, run_dir=self.recorder.run_dir)
            else:
                try:
                    observation = wait_for_tui_ready(
                        driver,
                        run_dir=self.recorder.run_dir,
                        config=fast_startup,
                        container_alive=alive,
                        log=self._diag_log,
                    )
                except StartupNotReadyError:
                    self._diag_log("  TUI not ready yet — falling back to full startup wait…")
                    try:
                        observation = wait_for_tui_ready(
                            driver,
                            run_dir=self.recorder.run_dir,
                            config=startup_cfg,
                            container_alive=alive,
                            log=self._diag_log,
                        )
                    except StartupNotReadyError as exc:
                        raise BenchError(str(exc)) from exc

            if startup_status is not None:
                startup_status.set(
                    PHASE_READY,
                    ready_message(),
                    task_id=task.task_id,
                    description=task.description,
                )
            else:
                self._diag_log(ready_message())

            self.recorder.record_initial_screen(record_key, observation=observation)

            agent_id = f"{human.operator_id}:{record_key}"
            self.recorder.begin_task(
                record_key,
                description=task.description,
                agent_id=agent_id,
            )

            turns: list[TurnRecord] = []
            stop_reason: StopReason = "session_lost"
            session_end_reason: str | None = None
            self._active = _ActiveTaskState(
                task_id=task_id,
                record_key=record_key,
                started_at=started_at,
                turns=turns,
            )

            def on_keystroke(record: KeystrokeRecord) -> None:
                self.recorder.append_keystroke_jsonl(record_key, record.to_dict())

            def on_keystroke_snapshot(
                record: KeystrokeRecord,
                obs,
            ) -> None:
                if obs is None:
                    return
                obs_fields = _turn_record_from_observation(obs)
                turn_record = TurnRecord(
                    turn=record.seq,
                    started_at=record.pressed_at,
                    ended_at=record.pressed_at,
                    duration_ms=0.0,
                    llm_latency_ms=None,
                    token_usage=None,
                    action=_keystroke_to_action(record),
                    action_ok=True,
                    action_error=None,
                    has_changed=True,
                    raw_agent_response="",
                    container_alive=bench.is_container_alive(task_id),
                    reasoning_content=None,
                    **obs_fields,
                )
                turns.append(turn_record)
                self.recorder.append_turn_jsonl(record_key, turn_record)
                self.recorder.update_live_task(
                    record_key,
                    turn_count=len(turns),
                    last_turn=record.seq,
                )

            turn_recorder: HumanTurnRecorder
            if interface == "web":
                turn_recorder = run_web_session(
                    session_id=task.session_id,
                    driver=driver,
                    run_dir=self.recorder.run_dir,
                    task_id=task.task_id,
                    description=task.description,
                    type_idle_ms=human.type_idle_ms,
                    submit_key=human.submit_key,
                    host=human.web_host,
                    port=human.web_port,
                    open_browser=False if startup_server else human.open_browser,
                    on_keystroke=on_keystroke,
                    on_keystroke_snapshot=on_keystroke_snapshot,
                    log=self._log,
                    startup_server=startup_server,
                )
                session_end_reason = "submit" if turn_recorder.submitted else "attach_lost"
            else:
                session = HumanSession(
                    task.session_id,
                    driver,
                    run_dir=self.recorder.run_dir,
                    task_id=task.task_id,
                    description=task.description,
                    type_idle_ms=human.type_idle_ms,
                    submit_key=human.submit_key,
                    task_pane_percent=human.task_pane_percent,
                    on_keystroke=on_keystroke,
                    on_keystroke_snapshot=on_keystroke_snapshot,
                    container_alive=alive,
                    log=self._diag_log,
                )
                session.run()
                turn_recorder = session.recorder
                session_end_reason = session.end_reason

        if self._active is not None:
            self._active.turn_recorder = turn_recorder
        turn_recorder.join_snapshots()

        if turn_recorder.submitted or session_end_reason == "submit":
            submit_turn = turn_recorder.keystroke_count + 1
            try:
                submit_obs = driver.observe(submit_turn, run_dir=self.recorder.run_dir)
                obs_fields = _turn_record_from_observation(submit_obs)
            except Exception:
                obs_fields = _turn_record_from_observation(observation)
            submit_record = TurnRecord(
                turn=submit_turn,
                started_at=_utc_now(),
                ended_at=_utc_now(),
                duration_ms=0.0,
                llm_latency_ms=None,
                token_usage=None,
                action={"action": "submit"},
                action_ok=True,
                action_error=None,
                has_changed=None,
                raw_agent_response="",
                container_alive=bench.is_container_alive(task_id),
                **obs_fields,
            )
            turns.append(submit_record)
            self.recorder.append_turn_jsonl(record_key, submit_record)
            stop_reason = "agent_submit"
        elif session_end_reason in {"attach_lost", "tui_exit"} or (
            not bench.is_container_alive(task_id) and turn_recorder.keystroke_count == 0
        ):
            # Attach/server died or TUI crashed — do NOT pretend the human submitted.
            if not bench.is_container_alive(task_id) and session_end_reason != "attach_lost":
                stop_reason = "container_exit"
            else:
                stop_reason = "session_lost"
        elif not bench.is_container_alive(task_id):
            stop_reason = "container_exit"
        elif turn_recorder.keystroke_count >= self.config.max_turns:
            stop_reason = "max_turns"
        else:
            stop_reason = "session_lost"

        self.recorder.update_live_task(
            record_key,
            turn_count=len(turns),
            status="completed",
            stop_reason=stop_reason,
        )
        self._active = None

        if stop_reason == "session_lost":
            from ..models import VerifyResult

            verify = VerifyResult(
                passed=False,
                line=f"[ABORT] {task.task_id} (session lost — not a submit)",
                details=[
                    "tmux attach ended unexpectedly (e.g. server exited); "
                    "no intentional submit was recorded",
                    f"keystrokes={turn_recorder.keystroke_count}",
                    f"session_end_reason={session_end_reason or 'unknown'}",
                ],
                recorded_at=_utc_now(),
            )
            self._diag_log(
                "⚠️ English-only text（English-only text Ctrl+G English-only text）。English-only text ABORT，English-only text retry-failed English-only text。"
            )
        else:
            verify = bench.verify()
        ended_at = _utc_now()

        return TaskRunResult(
            task_id=task.task_id,
            description=task.description,
            observation=task.observation,
            passed=verify.passed,
            verify=verify,
            turns=turns,
            stop_reason=stop_reason,
            agent_id=agent_id,
            started_at=started_at,
            ended_at=ended_at,
            total_token_usage=TokenUsage(),
        )

    def _operation_to_turn_record(
        self,
        op: HumanOperation,
        *,
        task_id: str,
        observation_fallback,
        driver: KeyboardDriver,
    ) -> TurnRecord:
        obs = op.observation
        if obs is None:
            try:
                obs = driver.observe(op.turn, run_dir=self.recorder.run_dir)
            except Exception:
                obs = observation_fallback
        obs_fields = _turn_record_from_observation(obs)
        return TurnRecord(
            turn=op.turn,
            started_at=op.started_at,
            ended_at=op.ended_at,
            duration_ms=op.duration_ms,
            llm_latency_ms=None,
            token_usage=None,
            action=op.to_action_dict(),
            action_ok=True,
            action_error=None,
            has_changed=True,
            raw_agent_response="",
            container_alive=self._task_bench.is_container_alive(task_id),
            reasoning_content=None,
            **obs_fields,
        )
