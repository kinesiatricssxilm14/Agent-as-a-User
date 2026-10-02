from __future__ import annotations

import json
import shutil
import subprocess
from typing import Any


class AgentTuiError(RuntimeError):
    pass


def _agent_tui_bin() -> str:
    path = shutil.which("agent-tui")
    if not path:
        raise AgentTuiError(
            "agent-tui not found in PATH; install from agent-tui/ (pip install -e .)"
        )
    return path


def run_agent_tui(*args: str, timeout: float = 120.0) -> dict[str, Any]:
    """Run agent-tui subcommand; parse JSON stdout."""
    cmd = [_agent_tui_bin(), *args]
    proc = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    stdout = (proc.stdout or "").strip()
    stderr = (proc.stderr or "").strip()
    if not stdout:
        raise AgentTuiError(
            f"agent-tui produced no JSON output (exit={proc.returncode}): {stderr}"
        )
    try:
        payload = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise AgentTuiError(f"invalid JSON from agent-tui: {stdout[:500]}") from exc
    if proc.returncode != 0 and not payload.get("success", False):
        err = payload.get("error") or stderr or f"exit code {proc.returncode}"
        raise AgentTuiError(err)
    return payload


def snapshot(
    *,
    session: str | None = None,
    fmt: str = "semantic",
    output: str | None = None,
) -> str:
    if session:
        run_agent_tui("use", session)
    args = ["snapshot", "--format", fmt]
    if output:
        args.extend(["--output", output])
    payload = run_agent_tui(*args)
    data = payload.get("data") or {}
    snap = data.get("snapshot")
    if snap is None:
        raise AgentTuiError(f"snapshot missing in response: {payload}")
    if fmt in {"plain", "json", "semantic"} and isinstance(snap, str):
        if snap.startswith("/") or snap.startswith("./"):
            from pathlib import Path

            return Path(snap).read_text(encoding="utf-8", errors="replace")
        return snap
    return str(snap)


def find_on_screen(pattern: str, *, session: str | None = None) -> dict[str, Any]:
    if session:
        run_agent_tui("use", session)
    payload = run_agent_tui("find", pattern)
    return payload


def wait_for_text(
    pattern: str,
    *,
    session: str | None = None,
    timeout_ms: int = 5000,
    fmt: str = "semantic",
) -> dict[str, Any]:
    if session:
        run_agent_tui("use", session)
    return run_agent_tui(
        "wait",
        str(timeout_ms),
        "--text",
        pattern,
        "--format",
        fmt,
        timeout=timeout_ms / 1000 + 30,
    )
