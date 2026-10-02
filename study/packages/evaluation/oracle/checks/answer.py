from __future__ import annotations

import re

from oracle.result import CheckResult


def normalize_answer(text: str, mode: str = "strip_lower") -> str:
    text = text.strip()
    if mode == "strip_lower":
        return re.sub(r"\s+", " ", text.lower())
    if mode == "strip":
        return re.sub(r"\s+", " ", text)
    if mode == "exact":
        return text
    raise ValueError(f"unknown normalize mode: {mode}")


def run_answer_check(
    check_id: str,
    *,
    agent_answer: str | None,
    expected: str,
    normalize: str = "strip_lower",
) -> CheckResult:
    if agent_answer is None or not str(agent_answer).strip():
        return CheckResult(
            check_id=check_id,
            check_type="answer",
            passed=False,
            message="no agent answer provided (use --agent-answer or ORACLE_AGENT_ANSWER)",
        )
    got = normalize_answer(str(agent_answer), normalize)
    want = normalize_answer(expected, normalize)
    passed = got == want
    return CheckResult(
        check_id=check_id,
        check_type="answer",
        passed=passed,
        message="answer matches fingerprint" if passed else f"expected '{want}', got '{got}'",
        details={"expected": want, "actual": got},
    )
