#!/usr/bin/env python3
"""Build Docker images for generated agent artifacts."""

from __future__ import annotations

import argparse
import fnmatch
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

from create_docker_context import (
    DEFAULT_BENCH_ROOT,
    DEFAULT_OUTPUT_ROOT,
    create_context,
    infer_command_name,
)


@dataclass(frozen=True)
class BuildTarget:
    combo: str
    project: str
    artifact: str
    output_dir: Path
    context_dir: Path
    image_name: str


def slug(value: str) -> str:
    normalized = re.sub(r"[^a-zA-Z0-9_.-]+", "-", value).strip("-")
    return normalized.lower().replace("_", "-")


def artifact_matches(path: Path, pattern: str) -> bool:
    return path.is_dir() and fnmatch.fnmatch(path.name.lower(), pattern.lower())


def discover_targets(
    *,
    output_root: Path,
    context_root: Path,
    combos: set[str] | None,
    projects: set[str] | None,
    artifact_glob: str,
) -> list[BuildTarget]:
    targets: list[BuildTarget] = []
    for combo_dir in sorted(output_root.iterdir()):
        if not combo_dir.is_dir() or combo_dir.name.startswith("_"):
            continue
        combo = combo_dir.name
        if combos and combo not in combos:
            continue
        for project_dir in sorted(combo_dir.iterdir()):
            if not project_dir.is_dir():
                continue
            project = project_dir.name
            if projects and project not in projects:
                continue
            for artifact_dir in sorted(project_dir.iterdir()):
                if not artifact_matches(artifact_dir, artifact_glob):
                    continue
                artifact = artifact_dir.name
                context_dir = context_root / combo / project
                image_name = f"agent-{slug(combo)}-{slug(project)}"
                targets.append(
                    BuildTarget(
                        combo=combo,
                        project=project,
                        artifact=artifact,
                        output_dir=project_dir,
                        context_dir=context_dir,
                        image_name=image_name,
                    )
                )
    return targets


def build_target(
    target: BuildTarget,
    *,
    benchmark_root: Path,
    overwrite_context: bool,
    no_cache: bool,
    dry_run: bool,
) -> bool:
    benchmark_dir = benchmark_root / target.project
    print(f"==> {target.combo} {target.project} {target.artifact}")
    print(f"    context: {target.context_dir}")
    print(f"    image:   {target.image_name}")

    if dry_run:
        print(
            "    create:  "
            f"{target.output_dir / target.artifact} -> {target.context_dir}"
        )
        print(f"    build:   docker build -t {target.image_name} {target.context_dir}")
        return True

    create_context(
        benchmark_dir=benchmark_dir,
        output_dir=target.output_dir,
        artifact_name=target.artifact,
        context_dir=target.context_dir,
        image_name=target.image_name,
        binary_name=infer_command_name(target.artifact),
        cmd_name=infer_command_name(target.artifact),
        overwrite=overwrite_context,
    )

    command = ["docker", "build", "-t", target.image_name]
    if no_cache:
        command.append("--no-cache")
    command.append(str(target.context_dir))
    result = subprocess.run(command, check=False)
    if result.returncode != 0:
        print(f"    failed: docker build exited {result.returncode}", file=sys.stderr)
        return False
    print(f"    ok: docker run -it --rm {target.image_name}")
    return True


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-root", type=Path, default=DEFAULT_OUTPUT_ROOT)
    parser.add_argument("--benchmark-root", type=Path, default=DEFAULT_BENCH_ROOT)
    parser.add_argument("--context-root", type=Path)
    parser.add_argument("--combo", action="append", help="Combo id, e.g. openhands__claude-sonnet-4-6")
    parser.add_argument("--project", action="append", help="Project id, e.g. P01-aptui")
    parser.add_argument("--artifact-glob", default="tool*")
    parser.add_argument("--overwrite-context", "--overwrite", action="store_true")
    parser.add_argument("--no-cache", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--stop-on-error", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_root = args.output_root.expanduser()
    context_root = (args.context_root or output_root / "_docker_contexts").expanduser()
    targets = discover_targets(
        output_root=output_root,
        context_root=context_root,
        combos=set(args.combo) if args.combo else None,
        projects=set(args.project) if args.project else None,
        artifact_glob=args.artifact_glob,
    )
    if not targets:
        print("No generated artifacts found.", file=sys.stderr)
        return 1

    print(f"Targets: {len(targets)}")
    failed: list[BuildTarget] = []
    for target in targets:
        ok = build_target(
            target,
            benchmark_root=args.benchmark_root,
            overwrite_context=args.overwrite_context,
            no_cache=args.no_cache,
            dry_run=args.dry_run,
        )
        if not ok:
            failed.append(target)
            if args.stop_on_error:
                break

    if failed:
        print("\nFailed builds:", file=sys.stderr)
        for target in failed:
            print(f"  {target.combo} {target.project} {target.artifact}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
