from __future__ import annotations

import re

from oracle.docker_exec import run_shell
from oracle.result import CheckResult


def run_shell_check(check_id: str, *, cmd: str, expect: str = "") -> CheckResult:
    proc = run_shell(cmd)
    out = (proc.stdout or "") + (proc.stderr or "")
    expect = expect.strip()
    if proc.returncode != 0 and not expect:
        return CheckResult(
            check_id,
            "shell",
            False,
            f"command failed (exit {proc.returncode}): {out[:200]}",
            details={"cmd": cmd, "output": out[:500]},
        )
    out_stripped = out.strip()
    if expect.startswith(">="):
        try:
            passed = float(out_stripped.split()[0]) >= float(expect[2:].strip())
        except (ValueError, IndexError):
            passed = expect[2:].strip() in out
    elif expect.startswith("regex:"):
        passed = bool(re.search(expect[6:], out, re.MULTILINE))
    elif re.fullmatch(r"\d+", expect):
        passed = out_stripped == expect
    elif expect:
        passed = expect in out
    else:
        passed = proc.returncode == 0
    return CheckResult(
        check_id,
        "shell",
        passed,
        "shell check passed" if passed else f"expected '{expect}' in output, got: {out[:200]}",
        details={"cmd": cmd, "output": out[:500]},
    )
