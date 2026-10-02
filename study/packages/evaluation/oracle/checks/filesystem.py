from __future__ import annotations

import fnmatch
import os
import re
from pathlib import Path
from typing import Iterable

from oracle.docker_exec import docker_container, run_shell
from oracle.result import CheckResult

# Directories we never descend into during haystack search
SKIP_DIR_NAMES = {
    ".git",
    "proc",
    "sys",
    "dev",
    "run",
    "target",
    "node_modules",
    "__pycache__",
    ".cargo",
}


def default_search_roots() -> list[Path]:
    env = os.environ.get("ORACLE_SEARCH_ROOTS", "")
    if env.strip():
        parts = re.split(r"[:,]", env)
        return [Path(p.strip()) for p in parts if p.strip()]
    return [
        Path("/bench"),
        Path("/data"),
        Path("/root"),
        Path("/tmp"),
        Path("/home"),
        Path.cwd(),
    ]


def _iter_files(roots: Iterable[Path], max_depth: int = 12) -> Iterable[Path]:
    for root in roots:
        if not root.exists():
            continue
        root = root.resolve()
        if root.is_file():
            yield root
            continue
        for dirpath, dirnames, filenames in os.walk(root, topdown=True):
            rel = Path(dirpath).relative_to(root)
            depth = len(rel.parts) if str(rel) != "." else 0
            if depth >= max_depth:
                dirnames.clear()
                continue
            dirnames[:] = [
                d for d in dirnames if d not in SKIP_DIR_NAMES and not d.startswith(".")
            ]
            for name in filenames:
                if name.startswith("."):
                    continue
                yield Path(dirpath) / name


def find_by_fingerprint(
    *,
    name_token: str | None = None,
    content_token: str | None = None,
    roots: list[Path] | None = None,
    max_depth: int = 12,
    max_file_bytes: int = 2_000_000,
    glob_pattern: str | None = None,
) -> list[dict]:
    """Needle-in-haystack: locate files whose name and/or body contain tokens."""
    roots = roots or default_search_roots()
    hits: list[dict] = []
    for path in _iter_files(roots, max_depth=max_depth):
        try:
            if path.stat().st_size > max_file_bytes:
                continue
        except OSError:
            continue
        name_ok = True
        content_ok = True
        if name_token:
            name_ok = name_token in path.name or name_token in str(path)
        if glob_pattern and not fnmatch.fnmatch(path.name, glob_pattern):
            continue
        body = ""
        if content_token:
            try:
                body = path.read_text(encoding="utf-8", errors="replace")
            except (OSError, UnicodeDecodeError):
                content_ok = False
            else:
                content_ok = content_token in body
        if name_ok and content_ok:
            hits.append(
                {
                    "path": str(path),
                    "size": path.stat().st_size,
                    "name_match": bool(name_token and name_token in path.name),
                    "content_match": bool(content_token and content_token in body),
                }
            )
    return hits


def find_by_fingerprint_in_container(
    container: str,
    *,
    name_token: str | None = None,
    content_token: str | None = None,
    roots: list[Path] | None = None,
    max_depth: int = 12,
    max_file_bytes: int = 2_000_000,
) -> list[dict]:
    """Haystack search via docker exec (host-side oracle against a running container)."""
    roots = roots or default_search_roots()
    root_list = " ".join(str(r) for r in roots)
    name_lit = (name_token or "").replace("'", "'\"'\"'")
    content_lit = (content_token or "").replace("'", "'\"'\"'")
    script = f"""
set -euo pipefail
name_token='{name_lit}'
content_token='{content_lit}'
max_depth={max_depth}
max_bytes={max_file_bytes}
for root in {root_list}; do
  [ -e "$root" ] || continue
  find "$root" -maxdepth "$max_depth" -type f -size -{max_file_bytes}c 2>/dev/null | while read -r f; do
    bn=$(basename "$f")
    name_ok=1
    content_ok=1
    if [ -n "$name_token" ]; then
      case "$f" in *"$name_token"*) ;; *) name_ok=0 ;; esac
    fi
    if [ -n "$content_token" ]; then
      awk -v t="$content_token" 'BEGIN {{
        while ((getline line < ARGV[1]) > 0) {{
          buf = buf (buf=="" ? "" : "\\n") line
        }}
        if (index(buf, t) > 0) exit 0
        exit 1
      }}' "$f" 2>/dev/null || content_ok=0
    fi
    if [ "$name_ok" = 1 ] && [ "$content_ok" = 1 ]; then
      echo "HIT:$f"
    fi
  done
done
"""
    proc = run_shell(script.strip())
    hits: list[dict] = []
    for line in (proc.stdout or "").splitlines():
        if not line.startswith("HIT:"):
            continue
        path = line[4:]
        hits.append(
            {
                "path": path,
                "name_match": bool(name_token and name_token in path),
                "content_match": bool(content_token),
            }
        )
    return hits


def run_filesystem_check(
    check_id: str,
    *,
    name_token: str | None = None,
    content_token: str | None = None,
    roots: list[str] | None = None,
    require_both: bool = True,
    min_hits: int = 1,
    max_depth: int = 12,
) -> CheckResult:
    root_paths = [Path(r) for r in roots] if roots else None
    container = docker_container()
    if require_both and not (name_token or content_token):
        return CheckResult(
            check_id=check_id,
            check_type="filesystem",
            passed=False,
            message="filesystem check needs at least one fingerprint token",
        )
    if container:
        hits = find_by_fingerprint_in_container(
            container,
            name_token=name_token,
            content_token=content_token if require_both or content_token else None,
            roots=root_paths,
            max_depth=max_depth,
        )
        if content_token and not require_both:
            hits = find_by_fingerprint_in_container(
                container,
                content_token=content_token,
                roots=root_paths,
                max_depth=max_depth,
            )
    else:
        hits = find_by_fingerprint(
            name_token=name_token,
            content_token=content_token if require_both or content_token else None,
            roots=root_paths,
            max_depth=max_depth,
        )
        if content_token and not require_both:
            # name optional: re-scan all hits for content only
            hits = find_by_fingerprint(
                content_token=content_token,
                roots=root_paths,
                max_depth=max_depth,
            )
    passed = len(hits) >= min_hits
    if passed:
        msg = f"found {len(hits)} file(s) matching fingerprint"
    else:
        msg = "no file matching fingerprint in search roots"
    return CheckResult(
        check_id=check_id,
        check_type="filesystem",
        passed=passed,
        message=msg,
        details={"hits": hits, "roots": [str(p) for p in (root_paths or default_search_roots())]},
    )
