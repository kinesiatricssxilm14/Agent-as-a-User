from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

SPEC_FILENAME = "bench.spec.json"

OBSERVATION_TYPES = frozenset({"filesystem", "screen", "answer", "shell"})

FINGERPRINT_KEYS = ("file_name", "file_content", "screen", "answer", "run_id")

FP_KEY_ALIASES = {
    "FP_FILE_NAME": "file_name",
    "FP_FILE_CONTENT": "file_content",
    "FP_SCREEN": "screen",
    "FP_ANSWER": "answer",
    "FP_RUN_ID": "run_id",
}

_PLACEHOLDER_NAMES = tuple(
    sorted(set(FINGERPRINT_KEYS) | set(FP_KEY_ALIASES.keys()), key=len, reverse=True)
)
_PLACEHOLDER_ALT = "|".join(re.escape(name) for name in _PLACEHOLDER_NAMES)
PLACEHOLDER_PATTERN = re.compile(
    rf"\{{{{({_PLACEHOLDER_ALT})\}}\}}|\$({_PLACEHOLDER_ALT})(?!\w)"
)

CHECK_METHOD_TO_OBSERVATION = {
    "check_shell": "shell",
    "check_screen": "screen",
    "check_screen_absent": "screen",
    "check_filesystem": "filesystem",
    "check_answer": "answer",
}


class ProjectSpec:
    """Machine-readable benchmark spec (bench.spec.json)."""

    def __init__(self, project_dir: Path):
        self.project_dir = Path(project_dir).resolve()
        self.spec_path = self.project_dir / SPEC_FILENAME
        if not self.spec_path.is_file():
            raise FileNotFoundError(
                f"{SPEC_FILENAME} not found: {self.spec_path}\n"
                f"Create it from task_note.md; tasks.json is human draft only."
            )
        self.spec = json.loads(self.spec_path.read_text())
        self.project_id = self.spec.get(
            "project_id", self.project_dir.name.split("-")[0]
        )

    @property
    def docker_image(self) -> str:
        return self.spec.get("docker_image") or f"bench-{self.spec.get('slug', 'tui')}"

    def list_tasks(self) -> list[str]:
        return [t["id"] for t in self.spec.get("tasks", [])]

    def get_task(self, task_id: str) -> dict[str, Any]:
        for task in self.spec.get("tasks", []):
            if task["id"] == task_id:
                return task
        raise KeyError(f"task {task_id} not found in {self.spec_path}")

    def get_observation(self, task_id: str) -> str:
        obs = self.get_task(task_id).get("observation", "")
        if obs not in OBSERVATION_TYPES:
            raise ValueError(
                f"task {task_id} missing or invalid observation "
                f"(must be one of: {', '.join(sorted(OBSERVATION_TYPES))})"
            )
        return obs

    def inject_description(
        self, task_id: str, fingerprints: dict[str, str]
    ) -> str:
        return expand_placeholders(
            self.get_task(task_id).get("description", ""), fingerprints
        )


def contains_fingerprint_placeholders(text: str) -> bool:
    return PLACEHOLDER_PATTERN.search(text) is not None


def expand_placeholders(text: str, fingerprints: dict[str, str]) -> str:
    out = text
    for key, val in fingerprints.items():
        out = out.replace(f"{{{{{key}}}}}", val)
        out = out.replace(f"${key}", val)
    for alias, key in FP_KEY_ALIASES.items():
        if key not in fingerprints:
            continue
        val = fingerprints[key]
        out = out.replace(f"{{{{{alias}}}}}", val)
        out = out.replace(f"${alias}", val)
    return out
