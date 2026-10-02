from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

# Stable short tokens: fc_a1b2c3d4 (11 chars). Same for every run of a given task.
TOKEN_HEX_LEN = 8

_KIND_PREFIX = {
    "file_name": "fn",
    "file_content": "fc",
    "screen": "sc",
    "answer": "ans",
}


def stable_run_id(*, project_id: str, task_id: str, scope: str | None = None) -> str:
    del scope
    return f"{project_id.lower()}_{task_id.lower()}"


def stable_token(*, scope: str, task_id: str, kind: str) -> str:
    prefix = _KIND_PREFIX[kind]
    key = f"{scope.upper()}:{task_id.upper()}:{kind}"
    digest = hashlib.sha256(key.encode("utf-8")).hexdigest()
    return f"{prefix}_{digest[:TOKEN_HEX_LEN]}"


def generate_fingerprints(
    *,
    project_id: str,
    task_id: str,
    run_id: str | None = None,
    overrides: dict[str, str] | None = None,
    scope: str | None = None,
) -> dict[str, str]:
    """Deterministic anti-cheat tokens for a task (stable across Agent/Human runs)."""
    token_scope = scope or project_id
    rid = run_id or stable_run_id(project_id=project_id, task_id=task_id, scope=token_scope)
    fps = {
        "run_id": rid,
        "file_name": stable_token(scope=token_scope, task_id=task_id, kind="file_name"),
        "file_content": stable_token(scope=token_scope, task_id=task_id, kind="file_content"),
        "screen": stable_token(scope=token_scope, task_id=task_id, kind="screen"),
        "answer": stable_token(scope=token_scope, task_id=task_id, kind="answer"),
    }
    if overrides:
        fps.update({k: v for k, v in overrides.items() if v})
    return fps


def save_fingerprints(path: Path, data: dict[str, str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")


def load_fingerprints(path: Path) -> dict[str, str]:
    if not path.is_file():
        raise FileNotFoundError(f"fingerprint file not found: {path}")
    return json.loads(path.read_text())


def resolve_fp(spec: str | dict[str, Any], fingerprints: dict[str, str]) -> str:
    """Resolve 'auto', '$file_name', or literal string to concrete token."""
    if isinstance(spec, dict):
        key = spec.get("ref") or spec.get("key")
        if not key:
            raise ValueError(f"invalid fingerprint spec dict: {spec}")
        spec = key
    if spec == "auto":
        raise ValueError("bare 'auto' is ambiguous; use a named fingerprint key")
    if spec.startswith("$"):
        key = spec[1:]
        if key not in fingerprints:
            raise KeyError(f"unknown fingerprint key: {key}")
        return fingerprints[key]
    if spec in fingerprints:
        return fingerprints[spec]
    return spec
