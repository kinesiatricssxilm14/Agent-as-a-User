from __future__ import annotations

import json
import os
import re
import secrets
import shutil
import subprocess
import time
from pathlib import Path
from typing import Any

from agent_tui.readiness import ScreenReadyConfig, is_bootstrap_screen, is_screen_ready

from oracle.agent_tui import AgentTuiError, run_agent_tui, snapshot
from oracle.docker_build import DockerBuildError, ensure_docker_image, smoke_test_image
from oracle.runner import OracleRunner
from oracle.seed_inject import (
    SeedInjectError,
    copy_seed_into_container,
    stage_seed_for_run,
)
from oracle.spec import ProjectSpec

CURRENT_SESSION_FILE = "current_session.json"
LEGACY_SUFFIXES = ("_session.json", ".injected.json")


class SessionError(RuntimeError):
    pass


def _oracle_dir(project_dir: Path) -> Path:
    path = project_dir / ".oracle"
    path.mkdir(parents=True, exist_ok=True)
    return path


def next_run_id(project_dir: Path) -> str:
    counter_file = _oracle_dir(project_dir) / "run_counter"
    n = 0
    if counter_file.is_file():
        try:
            n = int(counter_file.read_text().strip() or "0")
        except ValueError:
            n = 0
    n += 1
    counter_file.write_text(str(n))
    return f"{n:06d}-{secrets.token_hex(4)}"


def make_ids(run_id: str) -> tuple[str, str]:
    suffix = run_id.replace("-", "")[:10]
    prefix = (os.environ.get("TUI_BENCH_NAME_PREFIX") or "").strip()
    if prefix:
        safe = re.sub(r"[^a-zA-Z0-9]+", "", prefix)[:12]
        if safe:
            return f"bench-c-{safe}-{suffix}", f"bench-s-{safe}-{suffix}"
    return f"bench-c-{suffix}", f"bench-s-{suffix}"


def current_session_path(project_dir: Path) -> Path:
    return _oracle_dir(project_dir) / CURRENT_SESSION_FILE


def task_session_path(project_dir: Path, task_id: str) -> Path:
    return _oracle_dir(project_dir) / f"session_{task_id}.json"


def fingerprint_path(project_dir: Path, task_id: str) -> Path:
    return _oracle_dir(project_dir) / f"{task_id}_fingerprints.json"


def cleanup_oracle_runtime(project_dir: Path) -> None:
    """Remove per-run session files; keep fingerprints until regenerated."""
    oracle_dir = _oracle_dir(project_dir)
    for path in oracle_dir.glob("session_*.json"):
        path.unlink(missing_ok=True)
    for path in oracle_dir.iterdir():
        if not path.is_file():
            continue
        name = path.name
        if name == CURRENT_SESSION_FILE or name.endswith("_fingerprints.json"):
            continue
        if name == "run_counter":
            continue
        if any(name.endswith(suffix) for suffix in LEGACY_SUFFIXES):
            path.unlink(missing_ok=True)
        if "_" in name and name.endswith(".json") and not name.startswith("session_"):
            parts = name.split("_", 1)
            if parts[0].isdigit() or parts[0][:6].isdigit():
                path.unlink(missing_ok=True)


def load_current_session(project_dir: Path) -> dict[str, Any]:
    path = current_session_path(project_dir)
    if not path.is_file():
        raise FileNotFoundError(
            f"No active session for this project.\n"
            f"Run: bench.sh start T01   or   bench.sh start-all\n"
            f"(creates {path})"
        )
    return json.loads(path.read_text())


def save_current_session(project_dir: Path, payload: dict[str, Any]) -> Path:
    path = current_session_path(project_dir)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n")
    return path


def clear_current_session(project_dir: Path) -> bool:
    path = current_session_path(project_dir)
    if path.is_file():
        path.unlink()
        return True
    return False


def load_task_session(project_dir: Path, task_id: str) -> dict[str, Any]:
    path = task_session_path(project_dir, task_id)
    if not path.is_file():
        raise FileNotFoundError(f"Task session not found: {path}")
    return json.loads(path.read_text())


def save_task_session(project_dir: Path, task_id: str, payload: dict[str, Any]) -> Path:
    path = task_session_path(project_dir, task_id)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n")
    return path


def load_active_task_session(project_dir: Path) -> dict[str, Any]:
    current = load_current_session(project_dir)
    task_id = current.get("active_task_id")
    if not task_id:
        raise FileNotFoundError("current_session.json missing active_task_id")
    return load_task_session(project_dir, task_id)


def resolve_session_ref(
    project_dir: Path,
    ref: str | None,
) -> tuple[str, dict[str, Any]]:
    if ref and ref.strip():
        ref = ref.strip()
        if ref.startswith("T") and task_session_path(project_dir, ref).is_file():
            manifest = load_task_session(project_dir, ref)
            return manifest["run_id"], manifest
        for path in sorted(_oracle_dir(project_dir).glob("session_*.json")):
            try:
                manifest = json.loads(path.read_text())
            except (json.JSONDecodeError, OSError):
                continue
            rid = manifest.get("run_id", "")
            if ref in {rid, manifest.get("session_id"), manifest.get("container_id"), manifest.get("task_id")}:
                return rid, manifest
        raise FileNotFoundError(f"session not found for ref '{ref}'")

    manifest = load_active_task_session(project_dir)
    return manifest["run_id"], manifest


def load_manifest(project_dir: Path, ref: str) -> dict[str, Any]:
    _, manifest = resolve_session_ref(project_dir, ref)
    return manifest


def _docker_bin() -> str:
    path = shutil.which("docker")
    if not path:
        raise SessionError("docker not found in PATH")
    return path


def _remove_container(container_id: str) -> None:
    subprocess.run(
        [_docker_bin(), "rm", "-f", container_id],
        capture_output=True,
        text=True,
        check=False,
    )


def docker_image_exists(image: str) -> bool:
    from oracle.docker_build import docker_image_exists as _exists

    return _exists(image)


def _tmux_has_session(session_id: str) -> bool:
    if not shutil.which("tmux"):
        raise SessionError("tmux not found in PATH")
    # Keep in sync with agent-tui's dedicated bench socket (TUI_BENCH_TMUX_SOCKET).
    try:
        from agent_tui.tmux_backend import tmux_cli_prefix
        cmd = tmux_cli_prefix() + ["has-session", "-t", session_id]
    except ImportError:
        sock = (os.environ.get("TUI_BENCH_TMUX_SOCKET") or "tui-bench").strip() or "tui-bench"
        cmd = ["tmux", "-L", sock, "has-session", "-t", session_id]
    proc = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.returncode == 0


def _docker_container_exit_code(container_id: str) -> int | None:
    proc = subprocess.run(
        [_docker_bin(), "inspect", "-f", "{{.State.ExitCode}}", container_id],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        return None
    try:
        return int(proc.stdout.strip())
    except ValueError:
        return None


def _docker_logs_tail(container_id: str, lines: int = 30) -> str:
    proc = subprocess.run(
        [_docker_bin(), "logs", "--tail", str(lines), container_id],
        capture_output=True,
        text=True,
        check=False,
    )
    return (proc.stdout or "") + (proc.stderr or "")


def _docker_container_running(container_id: str) -> bool:
    proc = subprocess.run(
        [_docker_bin(), "inspect", "-f", "{{.State.Running}}", container_id],
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.returncode == 0 and proc.stdout.strip() == "true"


def _load_startup_spec(project_dir: Path) -> dict[str, Any]:
    spec_path = project_dir / "bench.spec.json"
    if not spec_path.is_file():
        return {}
    try:
        raw = json.loads(spec_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    startup = raw.get("startup") or {}
    return startup if isinstance(startup, dict) else {}


def _wait_for_tui_screen(
    session_id: str,
    project_dir: Path,
    *,
    timeout_sec: float = 180.0,
    stable_sec: float = 2.0,
) -> None:
    """Wait until seed/init output clears and the interactive TUI is on screen."""
    startup = _load_startup_spec(project_dir)
    timeout_sec = float(startup.get("timeout_sec", timeout_sec))
    stable_sec = float(startup.get("stable_sec", stable_sec))
    poll_interval = float(startup.get("poll_interval_sec", 0.5))
    min_text_len = int(startup.get("min_text_len", 40))
    ready_pattern = startup.get("ready_pattern")
    screen_cfg = ScreenReadyConfig(
        min_text_len=min_text_len,
        ready_pattern=ready_pattern,
    )

    _DYNAMIC_RE = re.compile(r'\b\d+[smhd]\S*ago\b|\b\d{2}:\d{2}:\d{2}\b|█')
    def _stable_text(t: str) -> str:
        return _DYNAMIC_RE.sub(' ', t)

    deadline = time.time() + timeout_sec
    last_text = ""
    stable_since: float | None = None
    last_reason = "starting"

    while time.time() < deadline:
        text = snapshot(session=session_id, fmt="plain")
        ready = is_screen_ready(text, config=screen_cfg)

        if not ready:
            if is_bootstrap_screen(text):
                last_reason = "bootstrap/install output on screen"
            elif len(text.strip()) < min_text_len:
                last_reason = "screen too empty"
            elif ready_pattern and not re.search(ready_pattern, text, re.I | re.S):
                last_reason = f"ready_pattern not matched: {ready_pattern!r}"
            else:
                last_reason = "screen not ready"
            stable_since = None
        elif stable_sec <= 0:
            return
        elif _stable_text(text) == _stable_text(last_text):
            if stable_since is None:
                stable_since = time.time()
            elif time.time() - stable_since >= stable_sec:
                return
        else:
            stable_since = time.time()

        last_text = text
        time.sleep(poll_interval)

    snippet = last_text.strip().replace("\n", "\\n")[:240]
    raise SessionError(
        f"Timed out after {timeout_sec}s waiting for TUI ready "
        f"(last reason: {last_reason}).\n"
        f"Screen tail: {snippet!r}\n"
        "Hint: entrypoint may still be running seed/init.sh before the TUI starts. "
        "Move heavy setup to Dockerfile build time, or set bench.spec.json startup.ready_pattern."
    )


def _wait_session_ready(
    container_id: str,
    session_id: str,
    project_dir: Path,
    *,
    timeout_sec: float = 120.0,
    stable_sec: float = 3.0,
) -> None:
    deadline = time.time() + timeout_sec
    stable_since: float | None = None

    while time.time() < deadline:
        if not _tmux_has_session(session_id):
            code = _docker_container_exit_code(container_id)
            logs = _docker_logs_tail(container_id)
            hint = ""
            if logs and "exec:" in logs and "not found" in logs:
                hint = (
                    "\nHint: check Dockerfile CMD — use JSON array per argument, "
                    'e.g. CMD ["sudo", "aptui"].'
                )
            raise SessionError(
                f"tmux session '{session_id}' died during startup.\n"
                f"Container exit code: {code}\n"
                f"--- docker logs (tail) ---\n{logs[-2000:]}{hint}"
            )

        if _docker_container_running(container_id):
            if stable_since is None:
                stable_since = time.time()
            elif time.time() - stable_since >= stable_sec:
                _wait_for_tui_screen(session_id, project_dir)
                return
        else:
            stable_since = None

        time.sleep(0.5)

    raise SessionError(
        f"Timed out waiting for container '{container_id}' to become stable.\n"
        f"--- docker logs (tail) ---\n{_docker_logs_tail(container_id)[-2000:]}"
    )


def _stop_task_container(manifest: dict[str, Any]) -> None:
    container_id = manifest.get("container_id", "")
    session_id = manifest.get("session_id", "")
    if container_id:
        _remove_container(container_id)
    if session_id:
        try:
            run_agent_tui("use", session_id, timeout=10.0)
            run_agent_tui("kill", timeout=10.0)
        except AgentTuiError:
            pass


def _ensure_task_fingerprints(runner: OracleRunner, task_id: str, *, refresh: bool = False) -> dict[str, str]:
    fp_path = fingerprint_path(runner.project_dir, task_id)
    if refresh and fp_path.is_file():
        fp_path.unlink()
    return runner.ensure_fingerprints(task_id)


def _docker_image_cmd(image: str) -> list[str]:
    proc = subprocess.run(
        [_docker_bin(), "inspect", "-f", "{{json .Config.Cmd}}", image],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        return []
    try:
        cmd = json.loads(proc.stdout.strip() or "[]")
    except json.JSONDecodeError:
        return []
    return cmd if isinstance(cmd, list) else []


def _start_detached_benchmark_container(container_id: str, image: str) -> None:
    """Keep the benchmark container alive after the TUI process exits."""
    proc = subprocess.run(
        [
            _docker_bin(),
            "run",
            "-d",
            "--name",
            container_id,
            "--entrypoint",
            "/bin/sh",
            image,
            "-c",
            "sleep infinity",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise SessionError(
            f"docker run (detached) failed: {(proc.stderr or proc.stdout).strip()}"
        )


def _docker_exec_tui_cmd(container_id: str, image: str) -> str:
    cmd = _docker_image_cmd(image)
    if not cmd:
        raise SessionError(f"cannot read CMD from image {image}")
    inner = " ".join(cmd)
    return f"docker exec -it {container_id} /entrypoint.sh {inner}"


def _docker_run_cmd_for_session(
    *,
    container_id: str,
    image: str,
    seed_copies: list[dict[str, str]],
) -> str:
    """Start a long-lived container; run the TUI via docker exec in tmux.

    When the TUI exits on its own (or the user quits), only the exec session
    ends — the container keeps running so filesystem/shell oracle checks still
    work via ``docker exec``.
    """
    _start_detached_benchmark_container(container_id, image)
    if seed_copies:
        copy_seed_into_container(container_id, seed_copies)
    return _docker_exec_tui_cmd(container_id, image)


def _start_task_session(
    project_dir: Path,
    task_id: str,
    *,
    suite_run_id: str | None = None,
    docker_image: str | None = None,
    skip_docker: bool = False,
    auto_build: bool = True,
    rebuild: bool = False,
    refresh_fingerprints: bool = False,
    task_order: list[str] | None = None,
    tasks_state: dict[str, Any] | None = None,
) -> dict[str, Any]:
    spec = ProjectSpec(project_dir)
    runner = OracleRunner(project_dir)
    rid = next_run_id(project_dir)
    container_id, session_id = make_ids(rid)
    image = docker_image or spec.docker_image

    fps = _ensure_task_fingerprints(runner, task_id, refresh=refresh_fingerprints)
    fp_path = fingerprint_path(project_dir, task_id)
    observation = spec.get_observation(task_id)
    description = spec.inject_description(task_id, fps)

    staging_root = _oracle_dir(project_dir) / "staged_seed" / task_id
    try:
        seed_applied, seed_copies = stage_seed_for_run(
            project_dir,
            task_id,
            fps,
            staging_root,
        )
    except SeedInjectError as exc:
        raise SessionError(str(exc)) from exc

    build_info: dict[str, Any] | None = None

    if not skip_docker:
        try:
            build_result = ensure_docker_image(
                project_dir,
                image,
                auto_build=auto_build,
                rebuild=rebuild,
            )
            build_info = build_result.to_dict()
        except DockerBuildError as exc:
            raise SessionError(str(exc)) from exc
        try:
            smoke_test_image(image)
        except DockerBuildError as exc:
            raise SessionError(str(exc)) from exc
        _remove_container(container_id)
        try:
            docker_cmd = _docker_run_cmd_for_session(
                container_id=container_id,
                image=image,
                seed_copies=seed_copies,
            )
        except SeedInjectError as exc:
            _remove_container(container_id)
            raise SessionError(str(exc)) from exc
        try:
            payload = run_agent_tui(
                "start",
                "--label",
                session_id,
                docker_cmd,
                timeout=180.0,
            )
        except AgentTuiError as exc:
            raise SessionError(f"agent-tui start failed: {exc}") from exc
        started_session = (payload.get("data") or {}).get("session_id", session_id)
        session_id = started_session
        try:
            _wait_session_ready(container_id, session_id, project_dir)
        except SessionError:
            _remove_container(container_id)
            raise

    eval_root = Path(__file__).resolve().parent.parent
    oracle_lib = eval_root / "scripts" / "oracle_lib.sh"
    session_file = task_session_path(project_dir, task_id)

    manifest: dict[str, Any] = {
        "run_id": rid,
        "suite_run_id": suite_run_id or rid,
        "container_id": container_id,
        "session_id": session_id,
        "task_id": task_id,
        "project_id": spec.project_id,
        "project_dir": str(project_dir.resolve()),
        "docker_image": image,
        "docker_build": build_info,
        "observation": observation,
        "description": description,
        "fingerprints": fps,
        "fingerprints_file": str(fp_path),
        "seed_staging": str(staging_root.resolve()) if seed_applied or seed_copies else None,
        "seed_docker_copies": seed_copies,
        "seed_injections": seed_applied,
        "session_file": str(session_file),
        "oracle_lib": str(oracle_lib),
    }
    save_task_session(project_dir, task_id, manifest)

    order = task_order or [task_id]
    state = dict(tasks_state or {tid: {"status": "pending"} for tid in order})
    state[task_id] = {"status": "active", "session_file": str(session_file)}

    save_current_session(
        project_dir,
        {
            "suite_run_id": suite_run_id or rid,
            "project_id": spec.project_id,
            "project_dir": str(project_dir.resolve()),
            "docker_image": image,
            "active_task_id": task_id,
            "task_order": order,
            "tasks": state,
        },
    )
    return manifest


def start_session(
    project_dir: Path,
    task_id: str,
    *,
    docker_image: str | None = None,
    run_id: str | None = None,
    skip_docker: bool = False,
    auto_build: bool = True,
    rebuild: bool = False,
) -> dict[str, Any]:
    cleanup_oracle_runtime(project_dir)
    if run_id:
        suite_run_id = run_id
    else:
        suite_run_id = next_run_id(project_dir)
    return _start_task_session(
        project_dir,
        task_id,
        suite_run_id=suite_run_id,
        docker_image=docker_image,
        skip_docker=skip_docker,
        auto_build=auto_build,
        rebuild=rebuild,
        refresh_fingerprints=True,
        task_order=[task_id],
    )


def start_all_tasks(
    project_dir: Path,
    *,
    docker_image: str | None = None,
    skip_docker: bool = False,
    auto_build: bool = True,
    rebuild: bool = False,
) -> dict[str, Any]:
    spec = ProjectSpec(project_dir)
    task_order = spec.list_tasks()
    if not task_order:
        raise SessionError("bench.spec.json has no tasks")

    cleanup_oracle_runtime(project_dir)
    clear_current_session(project_dir)

    runner = OracleRunner(project_dir)
    for tid in task_order:
        _ensure_task_fingerprints(runner, tid, refresh=True)

    suite_run_id = next_run_id(project_dir)
    first = task_order[0]
    return _start_task_session(
        project_dir,
        first,
        suite_run_id=suite_run_id,
        docker_image=docker_image,
        skip_docker=skip_docker,
        auto_build=auto_build,
        rebuild=rebuild,
        refresh_fingerprints=False,
        task_order=task_order,
    )


def next_task(
    project_dir: Path,
    *,
    docker_image: str | None = None,
    skip_docker: bool = False,
    auto_build: bool = True,
    rebuild: bool = False,
) -> dict[str, Any]:
    current = load_current_session(project_dir)
    task_order: list[str] = current.get("task_order") or []
    active = current.get("active_task_id")
    if not active or active not in task_order:
        raise SessionError("current_session.json has no valid active_task_id")

    active_manifest = load_task_session(project_dir, active)
    _stop_task_container(active_manifest)

    idx = task_order.index(active)
    if idx + 1 >= len(task_order):
        for path in _oracle_dir(project_dir).glob("session_*.json"):
            path.unlink(missing_ok=True)
        clear_current_session(project_dir)
        raise SessionError("All tasks completed. Run start-all to begin again.")

    next_id = task_order[idx + 1]
    tasks_state = dict(current.get("tasks") or {})
    tasks_state[active] = {
        "status": "done",
        "session_file": str(task_session_path(project_dir, active)),
    }

    return _start_task_session(
        project_dir,
        next_id,
        suite_run_id=current.get("suite_run_id"),
        docker_image=docker_image or current.get("docker_image"),
        skip_docker=skip_docker,
        auto_build=auto_build,
        rebuild=rebuild,
        refresh_fingerprints=False,
        task_order=task_order,
        tasks_state=tasks_state,
    )


def verify_session(
    project_dir: Path,
    run_id: str | None = None,
    *,
    agent_answer: str | None = None,
) -> Any:
    run_id, manifest = resolve_session_ref(project_dir, run_id)
    runner = OracleRunner(project_dir)
    task_id = manifest["task_id"]
    extra: dict[str, str] = {
        "ORACLE_DOCKER_CONTAINER": manifest["container_id"],
        "AGENT_TUI_SESSION": manifest["session_id"],
    }
    if agent_answer:
        extra["ORACLE_AGENT_ANSWER"] = agent_answer
    fp_file = manifest.get("fingerprints_file") or str(fingerprint_path(project_dir, task_id))
    extra["ORACLE_FP_FILE"] = fp_file
    return runner.run(
        task_id,
        extra_env=extra,
        fingerprint_file=Path(fp_file),
    )


def stop_session(project_dir: Path, run_id: str | None = None) -> dict[str, Any]:
    _, manifest = resolve_session_ref(project_dir, run_id)
    task_id = manifest["task_id"]
    _stop_task_container(manifest)

    for path in _oracle_dir(project_dir).glob("session_*.json"):
        path.unlink(missing_ok=True)
    clear_current_session(project_dir)

    return {
        "task_id": task_id,
        "run_id": manifest.get("run_id"),
        "container_id": manifest.get("container_id"),
        "session_id": manifest.get("session_id"),
        "stopped": True,
    }
