from __future__ import annotations

import re
from pathlib import Path

_PROJECT_DIR_RE = re.compile(r"^P\d+-")


def resolve_fingerprint_scope(project_dir: Path) -> str:
    """Stable scope for fingerprint tokens (usually the bench project folder name).

    Warm-pool slot clones live under ``slot-N`` directories; they carry a marker
    file or symlink back to the real project so tokens match the source bench.
    """
    project_dir = project_dir.resolve()
    name = project_dir.name
    if _PROJECT_DIR_RE.match(name):
        return name

    marker = project_dir / ".oracle" / "fingerprint_scope"
    if marker.is_file():
        text = marker.read_text(encoding="utf-8").strip()
        if text:
            return text

    spec_path = project_dir / "bench.spec.json"
    if spec_path.is_symlink():
        try:
            target_name = spec_path.resolve().parent.name
            if _PROJECT_DIR_RE.match(target_name):
                return target_name
        except OSError:
            pass

    return name
