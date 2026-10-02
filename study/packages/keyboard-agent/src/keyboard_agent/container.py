from __future__ import annotations

import shutil
import subprocess


def docker_available() -> bool:
    return shutil.which("docker") is not None


def container_running(container_id: str) -> bool:
    if not container_id or not docker_available():
        return False
    proc = subprocess.run(
        ["docker", "inspect", "-f", "{{.State.Running}}", container_id],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        return False
    return proc.stdout.strip().lower() == "true"
