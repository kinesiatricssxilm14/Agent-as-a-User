from __future__ import annotations

import json
import shutil
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, TextIO


class DockerBuildError(RuntimeError):
    pass


def _docker_bin() -> str:
    path = shutil.which("docker")
    if not path:
        raise DockerBuildError("docker not found in PATH")
    return path


def docker_image_exists(image: str) -> bool:
    proc = subprocess.run(
        [_docker_bin(), "image", "inspect", image],
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.returncode == 0


@dataclass
class BuildResult:
    image: str
    project_dir: Path
    action: str  # skipped | built
    log_file: Path | None = None
    duration_sec: float | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "image": self.image,
            "project_dir": str(self.project_dir),
            "action": self.action,
            "log_file": str(self.log_file) if self.log_file else None,
            "duration_sec": self.duration_sec,
        }


def _default_log_path(project_dir: Path) -> Path:
    log_dir = project_dir / ".oracle"
    log_dir.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return log_dir / f"docker_build_{stamp}.log"


def build_docker_image(
    project_dir: Path,
    image: str,
    *,
    rebuild: bool = False,
    log_file: Path | None = None,
    stream: TextIO | None = None,
) -> BuildResult:
    """Run docker build -t <image> . in project_dir."""
    project_dir = Path(project_dir).resolve()
    dockerfile = project_dir / "Dockerfile"
    if not dockerfile.is_file():
        raise DockerBuildError(f"Dockerfile not found: {dockerfile}")

    if not rebuild and docker_image_exists(image):
        return BuildResult(image=image, project_dir=project_dir, action="skipped")

    log_path = log_file or _default_log_path(project_dir)
    log_path.parent.mkdir(parents=True, exist_ok=True)

    cmd = [_docker_bin(), "build", "-t", image, str(project_dir)]
    started = datetime.now(timezone.utc)

    with log_path.open("w", encoding="utf-8") as log_fp:
        log_fp.write(f"# {' '.join(cmd)}\n")
        log_fp.write(f"# started {started.isoformat()}\n\n")
        log_fp.flush()

        proc = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        assert proc.stdout is not None
        for line in proc.stdout:
            log_fp.write(line)
            if stream is not None:
                stream.write(line)
                stream.flush()
        proc.wait()

        ended = datetime.now(timezone.utc)
        duration = (ended - started).total_seconds()
        log_fp.write(f"\n# exit={proc.returncode} duration={duration:.1f}s\n")

    if proc.returncode != 0:
        tail = log_path.read_text(encoding="utf-8", errors="replace")[-2000:]
        raise DockerBuildError(
            f"docker build failed (exit {proc.returncode}) for {image}\n"
            f"Log: {log_path}\n\n--- tail ---\n{tail}"
        )

    return BuildResult(
        image=image,
        project_dir=project_dir,
        action="built",
        log_file=log_path,
        duration_sec=duration,
    )


def ensure_docker_image(
    project_dir: Path,
    image: str,
    *,
    auto_build: bool = True,
    rebuild: bool = False,
    stream: TextIO | None = None,
) -> BuildResult:
    """Build image when missing (or when rebuild=True)."""
    if rebuild or not docker_image_exists(image):
        if not auto_build:
            raise DockerBuildError(
                f"Docker image not found: {image}\n"
                f"Run: bench-oracle --project-dir {project_dir} build"
            )
        return build_docker_image(
            project_dir,
            image,
            rebuild=rebuild,
            stream=stream or sys.stderr,
        )
    return BuildResult(image=image, project_dir=Path(project_dir).resolve(), action="skipped")


def smoke_test_image(image: str, *, timeout_sec: float = 120.0) -> None:
    """
    Validate image without launching the interactive TUI:
    1) CMD must be a proper exec-form argv (no single combined string)
    2) entrypoint + seed/init must succeed (run `true` as CMD)
    """
    proc = subprocess.run(
        [_docker_bin(), "inspect", "-f", "{{json .Config.Cmd}}", image],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise DockerBuildError(f"Cannot inspect image {image}: {proc.stderr}")
    try:
        cmd = json.loads(proc.stdout.strip() or "[]")
    except json.JSONDecodeError as exc:
        raise DockerBuildError(f"Invalid CMD metadata for {image}") from exc
    if isinstance(cmd, list) and len(cmd) == 1 and " " in str(cmd[0]):
        raise DockerBuildError(
            f"Malformed Dockerfile CMD for {image}: {cmd!r}\n"
            'Use JSON array with one element per arg, e.g. CMD ["sudo", "aptui"].'
        )

    run = subprocess.run(
        [
            _docker_bin(),
            "run",
            "--rm",
            "-i",
            "--entrypoint",
            "/entrypoint.sh",
            image,
            "true",
        ],
        capture_output=True,
        text=True,
        timeout=timeout_sec,
        check=False,
    )
    if run.returncode == 0:
        return
    output = (run.stdout or "") + (run.stderr or "")
    raise DockerBuildError(
        f"Container seed/init smoke test failed for {image} (exit {run.returncode}).\n\n"
        f"--- output tail ---\n{output[-2000:]}"
    )
