from __future__ import annotations

import json
import os
import stat
import subprocess
import tempfile
from pathlib import Path

from oracle.checks.filesystem import run_filesystem_check
from oracle.checks.answer import run_answer_check
from oracle.checks.screen import (
    match_patterns,
    run_screen_check,
    run_screen_color_check,
    run_screen_highlight_check,
    semantic_plain_text,
    text_is_highlighted,
)
from oracle.fingerprint import generate_fingerprints
from oracle.runner import OracleRunner


def test_filesystem_haystack(tmp_path: Path):
    fp = generate_fingerprints(project_id="P99", task_id="T01", run_id="testrun")
    target = tmp_path / "nested" / f"report_{fp['file_name']}.csv"
    target.parent.mkdir(parents=True)
    target.write_text(f"header\nvalue,{fp['file_content']}\n")
    result = run_filesystem_check(
        "fs",
        name_token=fp["file_name"],
        content_token=fp["file_content"],
        roots=[str(tmp_path)],
    )
    assert result.passed


def test_filesystem_name_only(tmp_path: Path):
    fp = generate_fingerprints(project_id="P99", task_id="T01", run_id="testrun")
    target = tmp_path / f"only_{fp['file_name']}.txt"
    target.write_text("no fingerprint here")
    assert run_filesystem_check(
        "fs",
        name_token=fp["file_name"],
        content_token=None,
        roots=[str(tmp_path)],
    ).passed
    assert not run_filesystem_check(
        "fs",
        name_token=fp["file_name"],
        content_token="missing-content",
        roots=[str(tmp_path)],
    ).passed


def test_filesystem_content_only(tmp_path: Path):
    fp = generate_fingerprints(project_id="P99", task_id="T01", run_id="testrun")
    target = tmp_path / "plain.txt"
    target.write_text(fp["file_content"])
    assert run_filesystem_check(
        "fs",
        name_token=None,
        content_token=fp["file_content"],
        roots=[str(tmp_path)],
    ).passed


def test_filesystem_content_only_in_container(monkeypatch):
    from oracle.checks import filesystem as fs_mod

    captured: dict[str, str] = {}

    def fake_run_shell(cmd: str, *, timeout=None):
        captured["cmd"] = cmd
        return type("P", (), {"stdout": "HIT:/bench/data/board/cols/todo/CARD-1.md\n", "stderr": ""})()

    monkeypatch.setenv("ORACLE_DOCKER_CONTAINER", "bench-c-test")
    monkeypatch.setattr(fs_mod, "run_shell", fake_run_shell)
    result = run_filesystem_check(
        "fs",
        name_token=None,
        content_token="fc_token",
        roots=["/bench/data/board/cols/todo"],
    )
    assert result.passed
    assert "name_token=''" in captured["cmd"]
    assert "content_token='fc_token'" in captured["cmd"]
    assert "None" not in captured["cmd"]


def test_screen_highlight_semantic(tmp_path: Path):
    semantic = "Packages <fg:yellow>wget</fg:yellow> <bg:blue>curl</bg:blue> installed"
    snap = tmp_path / "screen.sem"
    snap.write_text(semantic)

    assert run_screen_highlight_check(
        "hl",
        text="wget",
        snapshot_path=str(snap),
        fg="yellow",
    ).passed
    assert not run_screen_highlight_check(
        "hl",
        text="wget",
        snapshot_path=str(snap),
        fg="red",
    ).passed
    assert run_screen_highlight_check(
        "hl",
        text="wget",
        snapshot_path=str(snap),
        colored=True,
    ).passed
    assert run_screen_highlight_check(
        "hl",
        text="curl",
        snapshot_path=str(snap),
        highlighted=True,
    ).passed
    assert not run_screen_highlight_check(
        "hl",
        text="curl",
        snapshot_path=str(snap),
        colored=True,
    ).passed
    assert run_screen_highlight_check(
        "hl",
        text="wget",
        snapshot_path=str(snap),
        colored=True,
        highlighted=True,
    ).passed
    assert run_screen_highlight_check(
        "hl",
        text="curl",
        snapshot_path=str(snap),
        bg="blue",
    ).passed
    assert not run_screen_highlight_check(
        "hl",
        text="missing",
        snapshot_path=str(snap),
        colored=True,
    ).passed
    assert run_screen_highlight_check(
        "hl",
        text="Packages",
        snapshot_path=str(snap),
    ).passed


def test_screen_highlight_ignores_fake_default_colors(tmp_path: Path):
    semantic = (
        "Packages <fg:white>wget</fg:white> normal "
        "<fg:yellow>selected</fg:yellow> "
        "<bg:black>plainbg</bg:black>"
    )
    snap = tmp_path / "screen.sem"
    snap.write_text(semantic)

    assert not run_screen_highlight_check(
        "hl",
        text="wget",
        snapshot_path=str(snap),
        colored=True,
    ).passed
    assert run_screen_highlight_check(
        "hl",
        text="selected",
        snapshot_path=str(snap),
        colored=True,
    ).passed
    assert not run_screen_highlight_check(
        "hl",
        text="plainbg",
        snapshot_path=str(snap),
        highlighted=True,
    ).passed
    # Explicit --fg white still matches literally when requested.
    assert run_screen_highlight_check(
        "hl",
        text="wget",
        snapshot_path=str(snap),
        fg="white",
    ).passed


def test_screen_highlight_modal_baseline(tmp_path: Path):
    semantic = (
        "<fg:white>item1</fg:white><fg:white>item2</fg:white>"
        "<fg:yellow>item3</fg:yellow>"
    )
    snap = tmp_path / "screen.sem"
    snap.write_text(semantic)

    assert not run_screen_highlight_check(
        "hl",
        text="item1",
        snapshot_path=str(snap),
        colored=True,
    ).passed
    assert run_screen_highlight_check(
        "hl",
        text="item3",
        snapshot_path=str(snap),
        colored=True,
    ).passed


def test_screen_checks_case_insensitive(tmp_path: Path):
    snap = tmp_path / "screen.txt"
    snap.write_text("Package WGET selected\n")

    assert run_screen_check(
        "screen",
        patterns=["wget"],
        snapshot_path=str(snap),
        fmt="plain",
    ).passed
    assert run_screen_check(
        "screen",
        patterns=["SELECTED"],
        snapshot_path=str(snap),
        fmt="plain",
    ).passed

    semantic = "Line <fg:Red>Alert</fg:Red> end"
    sem_snap = tmp_path / "screen.sem"
    sem_snap.write_text(semantic)
    assert run_screen_highlight_check(
        "hl",
        text="alert",
        snapshot_path=str(sem_snap),
        fg="RED",
    ).passed
    assert match_patterns("abc DEF ghi", ["def"])


def test_screen_color_semantic(tmp_path: Path):
    semantic = "Line1 <fg:red>Selected</fg:red> Line2"
    snap = tmp_path / "screen.sem"
    snap.write_text(semantic)

    assert run_screen_color_check("color", fg="red", snapshot_path=str(snap)).passed
    assert not run_screen_color_check("color", fg="blue", snapshot_path=str(snap)).passed
    assert semantic_plain_text(semantic) == "Line1 Selected Line2"
    assert text_is_highlighted(semantic, "Selected", fg="red")[0]


def test_answer_check():
    fp = generate_fingerprints(project_id="P99", task_id="T02", run_id="testrun")
    assert run_answer_check("ans", agent_answer=fp["answer"], expected=fp["answer"]).passed
    assert not run_answer_check("ans", agent_answer="wrong", expected=fp["answer"]).passed


def test_runner_executes_oracle_script(tmp_path: Path):
    project = tmp_path / "P99-demo"
    (project / "oracle").mkdir(parents=True)
    script = project / "oracle" / "T01.sh"
    script.write_text("#!/usr/bin/env bash\nexit 0\n")
    script.chmod(script.stat().st_mode | stat.S_IEXEC)
    spec = {
        "project_id": "P99",
        "tasks": [
            {
                "id": "T01",
                "description": "dummy",
                "observation": "filesystem",
            }
        ],
    }
    (project / "bench.spec.json").write_text(json.dumps(spec))
    runner = OracleRunner(project)
    assert runner.run("T01").passed

    script.write_text("#!/usr/bin/env bash\nexit 1\n")
    assert not runner.run("T01").passed


def test_runner_requires_oracle_script(tmp_path: Path):
    project = tmp_path / "P99-demo"
    project.mkdir()
    (project / "bench.spec.json").write_text(
        json.dumps(
            {
                "project_id": "P99",
                "slug": "demo",
                "docker_image": "bench-demo",
                "tasks": [{"id": "T01", "description": "x", "observation": "screen"}],
            }
        )
    )
    runner = OracleRunner(project)
    try:
        runner.run("T01")
        assert False, "expected FileNotFoundError"
    except FileNotFoundError as e:
        assert "oracle/T01.sh" in str(e)


def test_session_start_skip_docker(tmp_path: Path):
    from oracle.session import (
        clear_current_session,
        current_session_path,
        fingerprint_path,
        load_current_session,
        load_task_session,
        resolve_session_ref,
        start_session,
        stop_session,
        task_session_path,
        verify_session,
    )

    project = tmp_path / "P99-demo"
    (project / "oracle").mkdir(parents=True)
    script = project / "oracle" / "T01.sh"
    script.write_text("#!/usr/bin/env bash\nexit 0\n")
    script.chmod(script.stat().st_mode | stat.S_IEXEC)
    spec = {
        "project_id": "P99",
        "slug": "demo",
        "docker_image": "bench-demo",
        "tasks": [
            {
                "id": "T01",
                "description": "do {{file_content}}",
                "observation": "filesystem",
            }
        ],
    }
    (project / "bench.spec.json").write_text(json.dumps(spec))

    manifest = start_session(project, "T01", skip_docker=True)
    assert manifest["container_id"].startswith("bench-c-")
    assert manifest["session_id"].startswith("bench-s-")
    assert manifest["run_id"]
    assert task_session_path(project, "T01").is_file()
    assert fingerprint_path(project, "T01").is_file()
    loaded = load_task_session(project, "T01")
    assert "fc_" in loaded["description"]

    current = load_current_session(project)
    assert current["active_task_id"] == "T01"
    assert current_session_path(project).is_file()

    _, resolved = resolve_session_ref(project, None)
    assert resolved["task_id"] == "T01"

    verify_session(project)
    stop_session(project)
    assert not current_session_path(project).is_file()


def test_resolve_manifest_ref_by_session_id(tmp_path: Path):
    from oracle.session import save_task_session, resolve_session_ref

    project = tmp_path / "P99-demo"
    manifest = {
        "run_id": "000012-88c622bb",
        "session_id": "bench-s-00001288c6",
        "container_id": "bench-c-00001288c6",
        "task_id": "T01",
    }
    save_task_session(project, "T01", manifest)
    rid, loaded = resolve_session_ref(project, "bench-s-00001288c6")
    assert rid == "000012-88c622bb"
    assert loaded["task_id"] == "T01"


def test_session_start_missing_docker_image(tmp_path: Path):
    from oracle.session import SessionError, start_session

    project = tmp_path / "P99-demo"
    (project / "oracle").mkdir(parents=True)
    script = project / "oracle" / "T01.sh"
    script.write_text("#!/usr/bin/env bash\nexit 0\n")
    script.chmod(script.stat().st_mode | stat.S_IEXEC)
    (project / "bench.spec.json").write_text(
        json.dumps(
            {
                "project_id": "P99",
                "slug": "demo",
                "docker_image": "bench-image-that-does-not-exist-xyz",
                "tasks": [
                    {
                        "id": "T01",
                        "description": "x",
                        "observation": "shell",
                    }
                ],
            }
        )
    )
    try:
        start_session(project, "T01", skip_docker=False, auto_build=False)
        assert False, "expected SessionError"
    except SessionError as exc:
        assert "Docker image not found" in str(exc) or "bench-oracle" in str(exc)


def test_build_skips_existing_image(tmp_path: Path, monkeypatch):
    from oracle import docker_build

    monkeypatch.setattr(docker_build, "docker_image_exists", lambda _img: True)
    (tmp_path / "Dockerfile").write_text("FROM scratch\n")
    (tmp_path / "bench.spec.json").write_text(
        json.dumps(
            {
                "project_id": "P99",
                "slug": "demo",
                "docker_image": "bench-tui-p99-demo",
                "tasks": [],
            }
        )
    )
    result = docker_build.build_docker_image(tmp_path, "bench-tui-p99-demo")
    assert result.action == "skipped"


def test_check_screen_explicit_pattern_ignores_fingerprint(tmp_path: Path):
    """check_screen "wget" must not also require FP_SCREEN."""
    eval_root = Path(__file__).resolve().parent.parent
    env = os.environ.copy()
    env["PYTHONPATH"] = str(eval_root)
    env["ORACLE_FP_FILE"] = str(
        tmp_path / "T03_fingerprints.json"
    )
    (tmp_path / "T03_fingerprints.json").write_text(
        json.dumps(
            {
                "screen": "sc_p01_t03_should_not_be_required",
                "file_name": "fn_x",
                "file_content": "fc_x",
            }
        )
    )
    snap = tmp_path / "screen.sem"
    snap.write_text("Package <fg:yellow>wget</fg:yellow> detail")
    proc = subprocess.run(
        [
            "python3",
            "-m",
            "oracle.cli",
            "check",
            "screen",
            "--pattern",
            "wget",
            "--snapshot",
            str(snap),
        ],
        cwd=str(eval_root),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 0, proc.stdout + proc.stderr
    payload = json.loads(proc.stdout)
    assert payload["passed"] is True
    assert "sc_p01_t03_should_not_be_required" not in payload.get("message", "")


def test_check_screen_absent_passes_when_pattern_missing(tmp_path: Path):
    eval_root = Path(__file__).resolve().parent.parent
    env = os.environ.copy()
    env["PYTHONPATH"] = str(eval_root)
    snap = tmp_path / "screen.sem"
    snap.write_text("Package <fg:yellow>curl</fg:yellow> detail")
    proc = subprocess.run(
        [
            "python3",
            "-m",
            "oracle.cli",
            "check",
            "screen-absent",
            "--pattern",
            "wget",
            "--snapshot",
            str(snap),
        ],
        cwd=str(eval_root),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 0, proc.stdout + proc.stderr
    payload = json.loads(proc.stdout)
    assert payload["passed"] is True
    assert payload["details"]["found_patterns"] == []


def test_check_screen_absent_fails_when_pattern_present(tmp_path: Path):
    eval_root = Path(__file__).resolve().parent.parent
    env = os.environ.copy()
    env["PYTHONPATH"] = str(eval_root)
    snap = tmp_path / "screen.sem"
    snap.write_text("Package <fg:yellow>wget</fg:yellow> detail")
    proc = subprocess.run(
        [
            "python3",
            "-m",
            "oracle.cli",
            "check",
            "screen-absent",
            "--pattern",
            "wget",
            "--snapshot",
            str(snap),
        ],
        cwd=str(eval_root),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 1, proc.stdout + proc.stderr
    payload = json.loads(proc.stdout)
    assert payload["passed"] is False
    assert payload["details"]["found_patterns"] == ["wget"]


def test_format_failed_check_line_screen_absent():
    from oracle.cli import _format_failed_check_line
    from oracle.result import CheckResult

    payload = {
        "check_type": "screen_absent",
        "passed": False,
        "message": "screen still shows forbidden pattern(s): ['error']",
        "details": {"found_patterns": ["error"], "forbidden_patterns": ["error"]},
    }
    check = CheckResult(
        "T02.sh",
        "oracle_sh",
        False,
        "oracle script failed",
        details={"output_tail": json.dumps(payload)},
    )
    line = _format_failed_check_line(check)
    assert 'check_screen_absent "error"' in line


def test_format_failed_check_line_from_oracle_tag():
    from oracle.cli import _format_failed_check_line
    from oracle.result import CheckResult

    check = CheckResult(
        "T03.sh",
        "oracle_sh",
        False,
        "oracle script failed (exit 1)",
        details={
            "output_tail": '[oracle] ✗ check_screen "libuuid1"\n{"passed": false}\n',
        },
    )
    assert _format_failed_check_line(check) == '✗ check_screen "libuuid1"'


def test_format_failed_check_line_from_json():
    from oracle.cli import _format_failed_check_line
    from oracle.result import CheckResult

    payload = {
        "check_type": "screen",
        "passed": False,
        "message": "screen missing fingerprint(s): ['libuuid1']",
        "details": {"missing_patterns": ["libuuid1"]},
    }
    check = CheckResult(
        "T03.sh",
        "oracle_sh",
        False,
        "oracle script failed",
        details={"output_tail": json.dumps(payload)},
    )
    line = _format_failed_check_line(check)
    assert 'check_screen "libuuid1"' in line
    assert "missing" in line


def test_oracle_cli_module_runs_check(tmp_path: Path):
    """python3 -m oracle.cli must invoke main(); otherwise check_* helpers no-op."""
    eval_root = Path(__file__).resolve().parent.parent
    env = os.environ.copy()
    env["PYTHONPATH"] = str(eval_root)
    proc = subprocess.run(
        [
            "python3",
            "-m",
            "oracle.cli",
            "check",
            "shell",
            "--cmd",
            "echo 0",
            "--expect",
            "1",
        ],
        cwd=str(eval_root),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 1, proc.stdout + proc.stderr
    payload = json.loads(proc.stdout)
    assert payload["passed"] is False


def test_parse_init_copy_rules(tmp_path: Path):
    from oracle.seed_layout import parse_init_copy_rules, seed_rel_to_container_path

    project = tmp_path / "P03-flow"
    (project / "seed").mkdir(parents=True)
    (project / "seed" / "init.sh").write_text(
        "#!/bin/sh\ncp -r /bench/boards/demo /bench/data/board\n"
    )
    rules = parse_init_copy_rules(project)
    assert len(rules) == 1
    assert rules[0]["from"] == "boards/demo"
    assert rules[0]["mount_at"] == "/bench/boards/demo"
    assert rules[0]["to"] == "/bench/data/board"
    assert (
        seed_rel_to_container_path("boards/demo/cols/in_progress/FLOW-4.md", rules)
        == "/bench/data/board/cols/in_progress/FLOW-4.md"
    )


def test_stage_seed_before_container(tmp_path: Path):
    from oracle.fingerprint import generate_fingerprints
    from oracle.seed_inject import stage_seed_for_run

    project = tmp_path / "P99-demo"
    seed_dir = project / "seed" / "boards" / "demo" / "cols" / "in_progress"
    seed_dir.mkdir(parents=True)
    seed_file = seed_dir / "FLOW-4.md"
    seed_file.write_text("# Title\n\nBody line.\n{{FP_SCREEN}}\n")
    (seed_dir / "order.txt").write_text("FLOW-4\n")
    (project / "seed" / "init.sh").write_text(
        "#!/bin/sh\ncp -r /bench/boards/demo /bench/data/board\n"
    )
    (project / "bench.spec.json").write_text(
        json.dumps(
            {
                "project_id": "P99",
                "slug": "demo",
                "docker_image": "bench-demo",
                "tasks": [
                    {
                        "id": "T01",
                        "description": "see {{screen}}",
                        "observation": "screen",
                    }
                ],
            }
        )
    )
    fps = generate_fingerprints(project_id="P99", task_id="T01", run_id="testrun")
    staging = project / ".oracle" / "staged_seed" / "T01"
    applied, seed_copies = stage_seed_for_run(
        project, "T01", fps, staging
    )
    assert len(applied) == 1
    assert applied[0]["container_path"] == "/bench/data/board/cols/in_progress/FLOW-4.md"
    staged = staging / "boards/demo/cols/in_progress/FLOW-4.md"
    assert fps["screen"] in staged.read_text()
    assert "{{FP_SCREEN}}" not in staged.read_text()
    assert "{{FP_SCREEN}}" in seed_file.read_text()
    assert (staging / "boards/demo/cols/in_progress/order.txt").read_text() == "FLOW-4\n"
    assert len(seed_copies) == 1
    assert seed_copies[0]["container"] == "/bench/data/board"
    assert seed_copies[0]["kind"] == "dir"


def test_expand_placeholders_fp_aliases():
    from oracle.spec import expand_placeholders

    fps = {
        "file_name": "fn_x",
        "file_content": "fc_x",
        "screen": "sc_x",
        "answer": "ans_x",
        "run_id": "rid_x",
    }
    text = "{{FP_FILE_NAME}} {{FP_FILE_CONTENT}} {{FP_SCREEN}} {{FP_ANSWER}} {{FP_RUN_ID}}"
    out = expand_placeholders(text, fps)
    assert out == "fn_x fc_x sc_x ans_x rid_x"


def test_session_start_seed_materialize_skip_docker(tmp_path: Path):
    from oracle.session import load_task_session, start_session

    project = tmp_path / "P99-demo"
    (project / "oracle").mkdir(parents=True)
    script = project / "oracle" / "T01.sh"
    script.write_text("#!/usr/bin/env bash\nexit 0\n")
    script.chmod(script.stat().st_mode | stat.S_IEXEC)
    seed_file = project / "seed" / "boards" / "demo" / "cols" / "in_progress" / "FLOW-4.md"
    seed_file.parent.mkdir(parents=True)
    seed_file.write_text("# card\n{{screen}}\n")
    (project / "seed" / "init.sh").write_text(
        "#!/bin/sh\ncp -r /bench/boards/demo /bench/data/board\n"
    )
    (project / "bench.spec.json").write_text(
        json.dumps(
            {
                "project_id": "P99",
                "slug": "demo",
                "docker_image": "bench-demo",
                "tasks": [
                    {
                        "id": "T01",
                        "description": "see {{screen}}",
                        "observation": "screen",
                    }
                ],
            }
        )
    )

    start_session(project, "T01", skip_docker=True)
    manifest = load_task_session(project, "T01")
    assert manifest["seed_injections"]
    assert manifest["seed_staging"]
    staged = Path(manifest["seed_injections"][0]["host_path"])
    assert staged.is_file()
    assert manifest["fingerprints"]["screen"] in staged.read_text()
    assert "{{screen}}" in seed_file.read_text()


def test_docker_run_cmd_uses_detached_container_and_exec(monkeypatch):
    from oracle.session import _docker_run_cmd_for_session

    calls: list[list[str]] = []

    def fake_run(argv, **kwargs):
        calls.append(list(argv))
        class Proc:
            returncode = 0
            stdout = '["demo-tui"]' if "inspect" in argv else ""
            stderr = ""

        return Proc()

    monkeypatch.setattr("oracle.session.subprocess.run", fake_run)
    monkeypatch.setattr(
        "oracle.session.copy_seed_into_container",
        lambda container_id, copies: calls.append(["copy", container_id, str(len(copies))]),
    )

    cmd = _docker_run_cmd_for_session(
        container_id="bench-c-deadbeef",
        image="bench-tui-demo",
        seed_copies=[{"host": "/tmp/x", "container": "/bench/y"}],
    )

    assert cmd == "docker exec -it bench-c-deadbeef /entrypoint.sh demo-tui"
    run_argv = next(a for a in calls if len(a) >= 2 and a[0].endswith("docker") and a[1] == "run")
    assert run_argv[2:6] == ["-d", "--name", "bench-c-deadbeef", "--entrypoint"]
    assert run_argv[-2:] == ["-c", "sleep infinity"]
    assert ["copy", "bench-c-deadbeef", "1"] in calls


def test_docker_run_cmd_without_seed_still_detached(monkeypatch):
    from oracle.session import _docker_run_cmd_for_session

    calls: list[list[str]] = []

    def fake_run(argv, **kwargs):
        calls.append(list(argv))
        class Proc:
            returncode = 0
            stdout = '["rura", "--file", "/bench/server.log"]' if "inspect" in argv else ""
            stderr = ""

        return Proc()

    monkeypatch.setattr("oracle.session.subprocess.run", fake_run)

    cmd = _docker_run_cmd_for_session(
        container_id="bench-c-cafebabe",
        image="bench-tui-p10-rura",
        seed_copies=[],
    )

    assert cmd == (
        "docker exec -it bench-c-cafebabe /entrypoint.sh rura --file /bench/server.log"
    )
    run_calls = [a for a in calls if len(a) >= 2 and a[0].endswith("docker") and a[1] == "run"]
    assert len(run_calls) == 1
    assert run_calls[0][-2:] == ["-c", "sleep infinity"]
