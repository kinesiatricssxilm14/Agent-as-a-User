from __future__ import annotations

import shutil
import subprocess
from pathlib import Path
from typing import Any

from oracle.seed_layout import (
    parse_init_copy_rules,
    plan_seed_mounts,
    seed_dir,
    seed_rel_to_container_path,
)
from oracle.spec import (
    PLACEHOLDER_PATTERN,
    contains_fingerprint_placeholders,
    expand_placeholders,
)


class SeedInjectError(RuntimeError):
    pass


def _docker_bin() -> str:
    path = shutil.which("docker")
    if not path:
        raise SeedInjectError("docker not found in PATH")
    return path


def _read_seed_text(path: Path) -> str | None:
    try:
        return path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        return None


def iter_seed_text_files(seed_root: Path) -> list[Path]:
    if not seed_root.is_dir():
        return []
    return sorted(p for p in seed_root.rglob("*") if p.is_file())


def find_seed_files_with_placeholders(seed_root: Path) -> list[tuple[Path, str]]:
    found: list[tuple[Path, str]] = []
    for path in iter_seed_text_files(seed_root):
        text = _read_seed_text(path)
        if text is None:
            continue
        if contains_fingerprint_placeholders(text):
            found.append((path, text))
    return found


def _expand_staged_file(
    path: Path,
    *,
    seed_rel: str,
    fingerprints: dict[str, str],
    rules: list[dict[str, str]],
    applied: list[dict[str, Any]],
) -> None:
    original = _read_seed_text(path)
    if original is None or not contains_fingerprint_placeholders(original):
        return
    expanded = expand_placeholders(original, fingerprints)
    if expanded == original:
        return
    path.write_text(expanded, encoding="utf-8")
    applied.append(
        {
            "seed_path": seed_rel,
            "container_path": seed_rel_to_container_path(seed_rel, rules),
            "placeholders": sorted(
                {
                    match.group(1) or match.group(2)
                    for match in PLACEHOLDER_PATTERN.finditer(original)
                }
            ),
            "host_path": str(path.resolve()),
            "applied": True,
        }
    )


def stage_seed_for_run(
    project_dir: Path,
    task_id: str,
    fingerprints: dict[str, str],
    staging_root: Path,
) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    """Stage fingerprint-expanded seed on the host (repo ``seed/`` stays untouched).

    Returns ``(applied, docker_copies)`` where each copy is
    ``{host, container, from}`` for ``docker cp`` into a writable container path.
    """
    del task_id
    try:
        seed_root = seed_dir(project_dir)
    except FileNotFoundError:
        return [], []

    placeholder_files = find_seed_files_with_placeholders(seed_root)
    if not placeholder_files:
        return [], []

    rules = parse_init_copy_rules(project_dir)
    seed_rels = [src.relative_to(seed_root).as_posix() for src, _ in placeholder_files]
    tree_mounts, file_mounts = plan_seed_mounts(seed_rels, rules)

    if staging_root.exists():
        shutil.rmtree(staging_root)
    staging_root.mkdir(parents=True, exist_ok=True)

    docker_copies: list[dict[str, str]] = []
    applied: list[dict[str, Any]] = []

    for prefix in sorted(tree_mounts):
        rule = tree_mounts[prefix]
        src_dir = seed_root / prefix
        if not src_dir.is_dir():
            raise SeedInjectError(f"seed directory not found: {src_dir}")
        dst_dir = staging_root / prefix
        shutil.copytree(src_dir, dst_dir)
        container_target = rule["to"]
        docker_copies.append(
            {
                "host": str(dst_dir.resolve()),
                "container": container_target,
                "from": prefix,
                "kind": "dir",
            }
        )
        for path in iter_seed_text_files(dst_dir):
            seed_rel = (Path(prefix) / path.relative_to(dst_dir)).as_posix()
            _expand_staged_file(
                path,
                seed_rel=seed_rel,
                fingerprints=fingerprints,
                rules=rules,
                applied=applied,
            )

    for rel in sorted(file_mounts):
        src_file = seed_root / rel
        if not src_file.is_file():
            raise SeedInjectError(f"seed file not found: {src_file}")
        dst_file = staging_root / rel
        dst_file.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src_file, dst_file)
        container_target = f"/bench/{rel}"
        docker_copies.append(
            {
                "host": str(dst_file.resolve()),
                "container": container_target,
                "from": rel,
                "kind": "file",
            }
        )
        _expand_staged_file(
            dst_file,
            seed_rel=rel,
            fingerprints=fingerprints,
            rules=rules,
            applied=applied,
        )

    return applied, docker_copies


def copy_seed_into_container(container_id: str, copies: list[dict[str, str]]) -> None:
    """``docker cp`` staged seed into the container filesystem (writable layer)."""
    if not copies:
        return
    docker = _docker_bin()
    for item in copies:
        host = item["host"]
        target = item["container"]
        kind = item.get("kind", "dir")
        if kind == "file":
            parent = str(Path(target).parent)
            proc = subprocess.run(
                [docker, "exec", container_id, "mkdir", "-p", parent],
                capture_output=True,
                text=True,
                check=False,
            )
            if proc.returncode != 0:
                raise SeedInjectError(
                    f"mkdir {parent} in {container_id}: {proc.stderr.strip()}"
                )
            cp_args = [docker, "cp", host, f"{container_id}:{target}"]
        else:
            proc = subprocess.run(
                [docker, "exec", container_id, "mkdir", "-p", target],
                capture_output=True,
                text=True,
                check=False,
            )
            if proc.returncode != 0:
                raise SeedInjectError(
                    f"mkdir {target} in {container_id}: {proc.stderr.strip()}"
                )
            cp_args = [docker, "cp", f"{host}/.", f"{container_id}:{target}/"]
        proc = subprocess.run(cp_args, capture_output=True, text=True, check=False)
        if proc.returncode != 0:
            raise SeedInjectError(
                f"docker cp to {target} failed: {(proc.stderr or proc.stdout).strip()}"
            )


def materialize_seed_fingerprints(
    project_dir: Path,
    task_id: str,
    fingerprints: dict[str, str],
    *,
    container_id: str | None = None,
    host_root: Path | None = None,
) -> list[dict[str, Any]]:
    del container_id
    root = host_root or (project_dir / ".oracle" / "staged_seed" / task_id)
    applied, copies = stage_seed_for_run(project_dir, task_id, fingerprints, root)
    if container_id and copies:
        copy_seed_into_container(container_id, copies)
    return applied
