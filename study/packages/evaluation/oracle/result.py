from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Any


@dataclass
class CheckResult:
    check_id: str
    check_type: str
    passed: bool
    message: str
    details: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass
class OracleResult:
    project_id: str
    task_id: str
    passed: bool
    checks: list[CheckResult] = field(default_factory=list)
    fingerprints: dict[str, str] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "project_id": self.project_id,
            "task_id": self.task_id,
            "passed": self.passed,
            "fingerprints": self.fingerprints,
            "checks": [c.to_dict() for c in self.checks],
        }
