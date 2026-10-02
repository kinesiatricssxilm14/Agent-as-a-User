from __future__ import annotations

import os
import subprocess
from typing import Sequence


def docker_container() -> str | None:
    name = os.environ.get("ORACLE_DOCKER_CONTAINER", "").strip()
    return name or None


def run_shell(cmd: str, *, timeout: float | None = None) -> subprocess.CompletedProcess[str]:
    """Run shell command on host or inside ORACLE_DOCKER_CONTAINER."""
    container = docker_container()
    if container:
        argv = ["docker", "exec", container, "bash", "-lc", cmd]
        return subprocess.run(
            argv,
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
    return subprocess.run(
        cmd,
        shell=True,
        capture_output=True,
        text=True,
        check=False,
        timeout=timeout,
    )


def run_argv(argv: Sequence[str], *, timeout: float | None = None) -> subprocess.CompletedProcess[str]:
    container = docker_container()
    if container:
        wrapped = ["docker", "exec", container, *argv]
        return subprocess.run(
            wrapped,
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
    return subprocess.run(
        list(argv),
        capture_output=True,
        text=True,
        check=False,
        timeout=timeout,
    )
