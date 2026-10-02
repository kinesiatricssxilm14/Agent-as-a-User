from __future__ import annotations

import re
from pathlib import Path

_INIT_CP_RE = re.compile(
    r"cp\s+-r(?:\s+-[^\s]+)*\s+(/bench/\S+?)\s+(/bench/\S+)",
)


def seed_dir(project_dir: Path) -> Path:
    path = project_dir / "seed"
    if not path.is_dir():
        raise FileNotFoundError(f"seed/ directory not found under {project_dir}")
    return path


def parse_init_copy_rules(project_dir: Path) -> list[dict[str, str]]:
    """Parse ``cp -r /bench/SRC /bench/DEST`` lines from seed/init.sh."""
    init = project_dir / "seed" / "init.sh"
    if not init.is_file():
        return []
    rules: list[dict[str, str]] = []
    for match in _INIT_CP_RE.finditer(init.read_text()):
        src = match.group(1).rstrip("/")
        dst = match.group(2).rstrip("/")
        if not src.startswith("/bench/") or not dst.startswith("/bench/"):
            continue
        src_rel = src.removeprefix("/bench/").lstrip("/")
        rules.append(
            {
                "from": src_rel,
                "mount_at": src,
                "to": dst,
            }
        )
    return rules


def _longest_matching_rule(rel: str, rules: list[dict[str, str]]) -> dict[str, str] | None:
    rel = rel.lstrip("/")
    best: dict[str, str] | None = None
    best_len = -1
    for rule in rules:
        prefix = rule["from"]
        if rel == prefix or rel.startswith(prefix + "/"):
            if len(prefix) > best_len:
                best = rule
                best_len = len(prefix)
    return best


def seed_rel_to_container_path(rel: str, rules: list[dict[str, str]]) -> str:
    rel = rel.lstrip("/")
    rule = _longest_matching_rule(rel, rules)
    if rule:
        suffix = rel[len(rule["from"]) :].lstrip("/")
        base = rule["to"]
        return f"{base}/{suffix}" if suffix else base
    return f"/bench/{rel}"


def plan_seed_mounts(
    seed_rel_paths: list[str],
    rules: list[dict[str, str]],
) -> tuple[dict[str, dict[str, str]], list[str]]:
    """Return tree mounts (prefix → rule) and per-file seed rel paths."""
    tree_mounts: dict[str, dict[str, str]] = {}
    file_mounts: list[str] = []

    for rel in seed_rel_paths:
        rule = _longest_matching_rule(rel, rules)
        if rule:
            tree_mounts[rule["from"]] = rule
        else:
            file_mounts.append(rel)

    covered: list[str] = []
    for rel in file_mounts:
        if any(rel == p or rel.startswith(p + "/") for p in tree_mounts):
            continue
        covered.append(rel)
    return tree_mounts, covered
