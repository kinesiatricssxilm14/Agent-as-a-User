#!/usr/bin/env python3
from __future__ import annotations

import re
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = (
    "README.md",
    "artifact.sh",
    "study/benchmark",
    "study/packages/agent-tui",
    "study/packages/evaluation",
    "study/packages/keyboard-agent",
    "generated-tuis/claude-code",
    "generated-tuis/codex",
    "generated-tuis/deepseek-harness",
)
TEXT_SUFFIXES = {
    "", ".c", ".cc", ".cfg", ".conf", ".cpp", ".css", ".csv", ".go",
    ".h", ".html", ".ini", ".js", ".json", ".jsx", ".md", ".py", ".rs",
    ".sh", ".sql", ".toml", ".ts", ".tsx", ".txt", ".template", ".yaml", ".yml",
}
HAN = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff]")
PRIVATE = re.compile(
    r"(?:/Users/|/data00/|/data1/|"
    r"(?i:/home/(?:freed|binchang)\b|"
    r"\b(?:freed|binchang|huruida|rdhu[0-9]*|wangyuanhao)\b|"
    r"\b(?:tcodex|tclaude)\b))"
)
SECRET = re.compile(
    r"(?i)(?:api[_-]?key|access[_-]?token|secret[_-]?key)\s*[:=]\s*['\"]"
    r"(?!\s*['\"]|\$|\.\.\.)[^'\"]{8,}"
)

errors = []
for relative in REQUIRED:
    if not (ROOT / relative).exists():
        errors.append(f"missing required path: {relative}")

for path in ROOT.rglob("*"):
    if not path.is_file() or path.name == "check_artifact.py":
        continue
    relative = path.relative_to(ROOT)
    if ".git" in relative.parts:
        continue
    if any(part in {"runs", "__pycache__"} for part in relative.parts):
        errors.append(f"generated/private path is present: {relative}")
        continue
    if path.suffix.lower() not in TEXT_SUFFIXES and path.name not in {
        "Dockerfile", "LICENSE", "Makefile"
    }:
        continue
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        continue
    if HAN.search(text):
        errors.append(f"non-English Han text: {relative}")
    if PRIVATE.search(text):
        errors.append(f"private identifier or host path: {relative}")
    if SECRET.search(text):
        errors.append(f"possible embedded secret: {relative}")

if errors:
    print("Artifact validation failed:", file=sys.stderr)
    for error in errors[:100]:
        print(f"  - {error}", file=sys.stderr)
    raise SystemExit(1)

checksum_file = ROOT / "SHA256SUMS"
if checksum_file.is_file():
    for line in checksum_file.read_text(encoding="utf-8").splitlines():
        expected, relative = line.split("  ", 1)
        target = ROOT / relative
        if not target.is_file():
            raise SystemExit(f"checksummed file is missing: {relative}")
        actual = hashlib.sha256(target.read_bytes()).hexdigest()
        if actual != expected:
            raise SystemExit(f"checksum mismatch: {relative}")

projects = sorted((ROOT / "study" / "benchmark").glob("P*/bench.spec.json"))
if len(projects) != 15:
    raise SystemExit(f"expected 15 benchmark projects, found {len(projects)}")
task_count = sum(
    len(json.loads(path.read_text(encoding="utf-8"))["tasks"]) for path in projects
)
if task_count != 84:
    raise SystemExit(f"expected 84 benchmark tasks, found {task_count}")

print("Artifact validation passed: English-only, anonymous, 15 projects, 84 tasks.")
