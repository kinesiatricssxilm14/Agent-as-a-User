#!/usr/bin/env python3
"""Run generation and Docker builds concurrently with resumable status."""

from __future__ import annotations

import argparse
import datetime as dt
import fnmatch
import json
import os
import re
import subprocess
import sys
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path

from build_docker_images import slug


DEFAULT_BENCH_ROOT = Path("/path/to/anonymous-artifact")
DEFAULT_OUTPUT_ROOT = Path("/path/to/anonymous-artifact")
DEFAULT_MODEL = "claude-sonnet-4-6"
DEFAULT_AGENTS = ["claude-code", "openhands"]
STATUS_LABELS = {
    "pending": "English-only text",
    "generating": "English-only text",
    "generated": "English-only text",
    "building": "English-only text",
    "built": "English-only text",
    "failed": "English-only text",
}


@dataclass(frozen=True)
class Target:
    agent: str
    model: str
    project: str

    @property
    def combo(self) -> str:
        return f"{self.agent}__{self.model}"

    @property
    def key(self) -> str:
        return f"{self.combo}/{self.project}"

    @property
    def image_name(self) -> str:
        return f"agent-{slug(self.combo)}-{slug(self.project)}"


class StatusStore:
    def __init__(self, path: Path, markdown_path: Path, total: int) -> None:
        self.path = path
        self.markdown_path = markdown_path
        self.total = total
        self.lock = threading.Lock()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.markdown_path.parent.mkdir(parents=True, exist_ok=True)
        if self.path.exists():
            self.data = json.loads(self.path.read_text(encoding="utf-8"))
        else:
            self.data = {"runs": {}}

    def init_target(self, target: Target) -> None:
        with self.lock:
            self.data["runs"].setdefault(
                target.key,
                {
                    "agent": target.agent,
                    "model": target.model,
                    "project": target.project,
                    "combo": target.combo,
                    "image_name": target.image_name,
                    "status": "pending",
                    "attempts": 0,
                },
            )
            self._write_locked()

    def get_status(self, target: Target) -> str:
        with self.lock:
            return str(self.data["runs"].get(target.key, {}).get("status", "pending"))

    def update(self, target: Target, status: str, **fields: object) -> None:
        with self.lock:
            run = self.data["runs"].setdefault(
                target.key,
                {
                    "agent": target.agent,
                    "model": target.model,
                    "project": target.project,
                    "combo": target.combo,
                    "image_name": target.image_name,
                    "status": "pending",
                    "attempts": 0,
                },
            )
            if status == "generating":
                run["attempts"] = int(run.get("attempts", 0)) + 1
            run.update(fields)
            if status != "failed" and "error" not in fields:
                run.pop("error", None)
            run["status"] = status
            run["updated_at"] = now()
            self._write_locked()

    def _write_locked(self) -> None:
        self.path.write_text(
            json.dumps(self.data, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        self._write_markdown_locked()

    def _write_markdown_locked(self) -> None:
        runs = self.data.get("runs", {})
        counts: dict[str, int] = {}
        for run in runs.values():
            counts[str(run.get("status", "pending"))] = counts.get(str(run.get("status", "pending")), 0) + 1

        lines = [
            "# Agent Generation Batch Status",
            "",
            f"Updated: `{now()}`",
            f"Total targets: `{self.total}`",
            "",
            "## Summary",
            "",
        ]
        for status in ["pending", "generating", "generated", "building", "built", "failed"]:
            lines.append(f"- {STATUS_LABELS[status]}: `{counts.get(status, 0)}`")
        lines.extend(
            [
                "",
                "## Targets",
                "",
                "| Agent | Model | Project | Status | Image | Output | Workspace | Log | Updated | Error |",
                "|---|---|---|---|---|---|---|---|---|---|",
            ]
        )
        for key in sorted(runs):
            run = runs[key]
            status = str(run.get("status", "pending"))
            error = str(run.get("error", "")).replace("|", "\\|").replace("\n", " ")[:180]
            output_dir = str(run.get("output_dir", ""))
            workspace_dir = str(run.get("workspace_dir", ""))
            log_prefix = str(run.get("log_prefix", ""))
            lines.append(
                "| "
                f"`{run.get('agent', '')}` | "
                f"`{run.get('model', '')}` | "
                f"`{run.get('project', '')}` | "
                f"{STATUS_LABELS.get(status, status)} | "
                f"`{run.get('image_name', '')}` | "
                f"`{output_dir}` | "
                f"`{workspace_dir}` | "
                f"`{log_prefix}` | "
                f"`{run.get('updated_at', '')}` | "
                f"{error} |"
            )
        self.markdown_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def now() -> str:
    return dt.datetime.now(dt.UTC).isoformat(timespec="seconds")


def discover_projects(root: Path) -> list[str]:
    return sorted(path.parent.name for path in root.glob("P*/P*-prompt.md"))


def has_artifact(output_root: Path, target: Target, artifact_glob: str) -> bool:
    project_dir = output_root / target.combo / target.project
    if not project_dir.exists():
        return False
    return any(
        path.is_dir() and fnmatch.fnmatch(path.name.lower(), artifact_glob.lower())
        for path in project_dir.iterdir()
    )


def read_generation_meta(output_root: Path, target: Target) -> dict[str, object]:
    meta_path = output_root / target.combo / target.project / "meta.json"
    if not meta_path.exists():
        return {}
    try:
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    return {
        "run_dir": meta.get("run_dir", ""),
        "workspace_dir": meta.get("workspace_dir", ""),
        "artifacts": meta.get("artifacts", []),
    }


def docker_image_exists(image_name: str) -> bool:
    return subprocess.run(
        ["docker", "image", "inspect", image_name],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    ).returncode == 0


def run_logged(command: list[str], log_prefix: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    log_prefix.parent.mkdir(parents=True, exist_ok=True)
    stdout_path = log_prefix.with_suffix(".stdout.log")
    stderr_path = log_prefix.with_suffix(".stderr.log")
    with stdout_path.open("w", encoding="utf-8") as stdout_file, stderr_path.open("w", encoding="utf-8") as stderr_file:
        stdout_file.write("$ " + " ".join(command) + "\n\n")
        stdout_file.flush()
        return subprocess.run(
            command,
            env=env,
            text=True,
            stdout=stdout_file,
            stderr=stderr_file,
            check=False,
        )


def run_target(
    target: Target,
    *,
    bench_root: Path,
    output_root: Path,
    config: Path,
    logs_root: Path,
    artifact_glob: str,
    overwrite_generation: bool,
    overwrite_context: bool,
    skip_existing: bool,
    dry_run: bool,
    status: StatusStore,
) -> bool:
    env = os.environ.copy()
    gen_log = logs_root / target.combo / target.project / "generate"
    build_log = logs_root / target.combo / target.project / "build"
    runs_root = output_root / "_runs"
    output_dir = output_root / target.combo / target.project

    if skip_existing and has_artifact(output_root, target, artifact_glob):
        status.update(
            target,
            "generated",
            note="artifact already exists",
            output_dir=str(output_dir),
            **read_generation_meta(output_root, target),
        )
    else:
        status.update(
            target,
            "generating",
            log_prefix=str(gen_log),
            output_dir=str(output_dir),
            runs_root=str(runs_root),
        )
        gen_command = [
            sys.executable,
            "agent-generation/run_generation.py",
            "--config",
            str(config),
            "--input-root",
            str(bench_root),
            "--output-root",
            str(output_root),
            "--runs-root",
            str(runs_root),
            "--only",
            target.project,
            "--agent",
            target.agent,
            "--model",
            target.model,
            "--stop-on-error",
        ]
        if overwrite_generation:
            gen_command.append("--overwrite")
        elif skip_existing:
            gen_command.append("--skip-existing")
        if dry_run:
            gen_command.append("--dry-run")
            print("[dry-run]", " ".join(gen_command))
            status.update(target, "generated", note="dry-run generation")
        else:
            gen_result = run_logged(gen_command, gen_log, env)
            if gen_result.returncode != 0:
                status.update(
                    target,
                    "failed",
                    error=f"generation exited {gen_result.returncode}",
                    log_prefix=str(gen_log),
                    output_dir=str(output_dir),
                    **read_generation_meta(output_root, target),
                )
                return False
            if not has_artifact(output_root, target, artifact_glob):
                status.update(
                    target,
                    "failed",
                    error="generation completed but no tool* artifact found",
                    log_prefix=str(gen_log),
                    output_dir=str(output_dir),
                    **read_generation_meta(output_root, target),
                )
                return False
            status.update(
                target,
                "generated",
                log_prefix=str(gen_log),
                output_dir=str(output_dir),
                **read_generation_meta(output_root, target),
            )

    if skip_existing and docker_image_exists(target.image_name):
        status.update(
            target,
            "built",
            note="image already exists",
            output_dir=str(output_dir),
            **read_generation_meta(output_root, target),
        )
        return True

    status.update(
        target,
        "building",
        log_prefix=str(build_log),
        output_dir=str(output_dir),
        **read_generation_meta(output_root, target),
    )
    build_command = [
        sys.executable,
        "agent-generation/build_docker_images.py",
        "--output-root",
        str(output_root),
        "--benchmark-root",
        str(bench_root),
        "--combo",
        target.combo,
        "--project",
        target.project,
        "--artifact-glob",
        artifact_glob,
        "--stop-on-error",
    ]
    if overwrite_context:
        build_command.append("--overwrite-context")
    if dry_run:
        build_command.append("--dry-run")
        print("[dry-run]", " ".join(build_command))
        status.update(target, "built", note="dry-run build")
        return True

    build_result = run_logged(build_command, build_log, env)
    if build_result.returncode != 0:
        status.update(
            target,
            "failed",
            error=f"docker build exited {build_result.returncode}",
            log_prefix=str(build_log),
            output_dir=str(output_dir),
            **read_generation_meta(output_root, target),
        )
        return False
    status.update(
        target,
        "built",
        log_prefix=str(build_log),
        output_dir=str(output_dir),
        **read_generation_meta(output_root, target),
    )
    return True


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bench-root", type=Path, default=DEFAULT_BENCH_ROOT)
    parser.add_argument("--output-root", type=Path, default=DEFAULT_OUTPUT_ROOT)
    parser.add_argument("--config", type=Path, default=Path("agent-generation/configs/agents.yaml"))
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument("--agent", action="append", choices=["claude-code", "openhands", "claude-code-code"])
    parser.add_argument("--project", action="append", help="Project id, e.g. P01-aptui")
    parser.add_argument("--max-workers", type=int, default=5)
    parser.add_argument("--artifact-glob", default="tool*")
    parser.add_argument("--status-json", type=Path)
    parser.add_argument("--status-md", type=Path)
    parser.add_argument("--logs-root", type=Path)
    parser.add_argument("--overwrite-generation", action="store_true")
    parser.add_argument("--overwrite-context", action="store_true", default=True)
    parser.add_argument("--no-skip-existing", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--stop-on-error", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    bench_root = args.bench_root.expanduser()
    output_root = args.output_root.expanduser()
    agents = args.agent or DEFAULT_AGENTS
    projects = args.project or discover_projects(bench_root)
    targets = [Target(agent=agent, model=args.model, project=project) for agent in agents for project in projects]
    status_dir = output_root / "_batch_status"
    safe_bench = re.sub(r"[^a-zA-Z0-9_.-]+", "-", bench_root.name)
    dry_suffix = ".dry-run" if args.dry_run else ""
    status_json = args.status_json or status_dir / f"{safe_bench}-{args.model}{dry_suffix}.json"
    status_md = args.status_md or status_dir / f"{safe_bench}-{args.model}{dry_suffix}.md"
    logs_root = args.logs_root or status_dir / f"{safe_bench}-{args.model}{dry_suffix}-logs"
    store = StatusStore(status_json, status_md, total=len(targets))

    for target in targets:
        store.init_target(target)

    print(f"Projects: {len(projects)}")
    print(f"Targets:  {len(targets)}")
    print(f"Workers:  {args.max_workers}")
    print(f"Status:   {status_md}")

    failed = False
    with ThreadPoolExecutor(max_workers=args.max_workers) as executor:
        futures = {
            executor.submit(
                run_target,
                target,
                bench_root=bench_root,
                output_root=output_root,
                config=args.config,
                logs_root=logs_root,
                artifact_glob=args.artifact_glob,
                overwrite_generation=args.overwrite_generation,
                overwrite_context=args.overwrite_context,
                skip_existing=not args.no_skip_existing,
                dry_run=args.dry_run,
                status=store,
            ): target
            for target in targets
        }
        for future in as_completed(futures):
            target = futures[future]
            try:
                ok = future.result()
            except Exception as exc:
                ok = False
                store.update(target, "failed", error=repr(exc))
            if not ok:
                failed = True
                if args.stop_on_error:
                    for pending in futures:
                        pending.cancel()
                    break

    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
