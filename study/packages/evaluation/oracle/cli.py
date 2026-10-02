from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

from oracle.checks import (
    run_answer_check,
    run_filesystem_check,
    run_screen_absent_check,
    run_screen_check,
    run_screen_color_check,
    run_screen_highlight_check,
)
from oracle.checks.shell import run_shell_check
from oracle.fingerprint import load_fingerprints
from oracle.docker_build import DockerBuildError, build_docker_image
from oracle.runner import OracleRunner
from oracle.session import (
    SessionError,
    load_current_session,
    load_manifest,
    next_task,
    resolve_session_ref,
    start_all_tasks,
    start_session,
    stop_session,
    verify_session,
)
from oracle.spec import ProjectSpec


def _resolve_eval_root() -> Path:
    return Path(__file__).resolve().parent.parent


def _ensure_bench_oracle_lib() -> None:
    if not os.environ.get("BENCH_ORACLE_LIB"):
        os.environ["BENCH_ORACLE_LIB"] = str(_resolve_eval_root() / "scripts" / "oracle_lib.sh")


def cmd_gen_fps(args: argparse.Namespace) -> int:
    runner = OracleRunner(args.project_dir)
    fps = runner.ensure_fingerprints(
        args.task,
        run_id=args.run_id,
        persist=not args.stdout,
    )
    if args.stdout:
        print(json.dumps(fps, indent=2, ensure_ascii=False))
    else:
        path = runner.fingerprint_path(args.task, args.run_id)
        print(f"Wrote fingerprints → {path}", file=sys.stderr)
        print(json.dumps(fps, indent=2, ensure_ascii=False))
    return 0


def cmd_run(args: argparse.Namespace) -> int:
    _ensure_bench_oracle_lib()
    runner = OracleRunner(args.project_dir)
    extra: dict[str, str] = {}
    if args.agent_answer:
        extra["ORACLE_AGENT_ANSWER"] = args.agent_answer
    if args.session:
        extra["AGENT_TUI_SESSION"] = args.session
    if args.docker_container:
        extra["ORACLE_DOCKER_CONTAINER"] = args.docker_container
    if args.run_id:
        manifest = load_manifest(args.project_dir, args.run_id)
        extra.setdefault("ORACLE_DOCKER_CONTAINER", manifest["container_id"])
        extra.setdefault("AGENT_TUI_SESSION", manifest["session_id"])
        extra.setdefault("ORACLE_FP_FILE", manifest["fingerprints_file"])
    result = runner.run(
        args.task,
        run_id=args.run_id,
        extra_env=extra or None,
    )
    _print_run_result(result, runner, args.task, args.human)
    return 0 if result.passed else 1


def cmd_inject_task(args: argparse.Namespace) -> int:
    runner = OracleRunner(args.project_dir)
    task = runner.get_task(args.task)
    fps = runner.ensure_fingerprints(args.task, run_id=args.run_id)
    desc = runner.inject_description(args.task, fps)
    payload = {
        "task_id": args.task,
        "observation": task.get("observation"),
        "description": desc,
        "fingerprints": fps,
    }
    if args.run_id:
        payload["run_id"] = args.run_id
    print(json.dumps(payload, indent=2, ensure_ascii=False))
    return 0


def cmd_list_tasks(args: argparse.Namespace) -> int:
    spec = ProjectSpec(args.project_dir)
    tasks = []
    for task_id in spec.list_tasks():
        task = spec.get_task(task_id)
        tasks.append(
            {
                "id": task_id,
                "observation": task.get("observation"),
                "description": task.get("description", ""),
            }
        )
    print(
        json.dumps(
            {
                "project_id": spec.project_id,
                "spec_file": str(spec.spec_path),
                "docker_image": spec.docker_image,
                "tasks": tasks,
            },
            indent=2,
            ensure_ascii=False,
        )
    )
    return 0


def cmd_build(args: argparse.Namespace) -> int:
    spec = ProjectSpec(args.project_dir)
    image = args.docker_image or spec.docker_image
    try:
        result = build_docker_image(
            args.project_dir,
            image,
            rebuild=args.rebuild,
            stream=sys.stderr if args.human else None,
        )
    except DockerBuildError as exc:
        print(json.dumps({"success": False, "error": str(exc)}, indent=2), file=sys.stderr)
        return 1
    payload = {"success": True, **result.to_dict()}
    print(json.dumps(payload, indent=2, ensure_ascii=False))
    if args.human:
        if result.action == "skipped":
            print(f"\n[SKIP] image already exists: {image}", file=sys.stderr)
        else:
            print(
                f"\n[BUILT] {image} in {result.duration_sec:.1f}s → log {result.log_file}",
                file=sys.stderr,
            )
    return 0


def cmd_session_start(args: argparse.Namespace) -> int:
    try:
        manifest = start_session(
            args.project_dir,
            args.task,
            docker_image=args.docker_image,
            run_id=args.run_id,
            skip_docker=args.skip_docker,
            auto_build=not args.no_build,
            rebuild=args.rebuild,
        )
    except (SessionError, FileNotFoundError, KeyError, ValueError) as exc:
        print(json.dumps({"success": False, "error": str(exc)}, indent=2), file=sys.stderr)
        return 1
    if args.json or not args.human:
        print(json.dumps(manifest, indent=2, ensure_ascii=False))
    if args.human:
        build = manifest.get("docker_build") or {}
        if build.get("action") == "built":
            print(
                f"[BUILT] {build.get('image')} ({build.get('duration_sec', '?')}s)",
                file=sys.stderr,
            )
        obs = manifest.get("observation", "")
        print(
            f"[READY] {manifest.get('project_id')} {manifest.get('task_id')} ({obs})",
            file=sys.stderr,
        )
    return 0


def cmd_session_start_all(args: argparse.Namespace) -> int:
    try:
        manifest = start_all_tasks(
            args.project_dir,
            docker_image=args.docker_image,
            skip_docker=args.skip_docker,
            auto_build=not args.no_build,
            rebuild=args.rebuild,
        )
    except (SessionError, FileNotFoundError, KeyError, ValueError) as exc:
        print(json.dumps({"success": False, "error": str(exc)}, indent=2), file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(manifest, indent=2, ensure_ascii=False))
    if args.human:
        current = load_current_session(args.project_dir)
        order = current.get("task_order") or []
        build = manifest.get("docker_build") or {}
        if build.get("action") == "built":
            print(
                f"[BUILT] {build.get('image')} ({build.get('duration_sec', '?')}s)",
                file=sys.stderr,
            )
        obs = manifest.get("observation", "")
        print(
            f"[READY] {manifest.get('project_id')} {manifest.get('task_id')} ({obs}) "
            f"— 1/{len(order)}",
            file=sys.stderr,
        )
    return 0


def cmd_session_next(args: argparse.Namespace) -> int:
    try:
        manifest = next_task(
            args.project_dir,
            docker_image=args.docker_image,
            skip_docker=args.skip_docker,
            auto_build=not args.no_build,
            rebuild=args.rebuild,
        )
    except SessionError as exc:
        print(str(exc), file=sys.stderr)
        return 1 if "All tasks completed" not in str(exc) else 0
    except (FileNotFoundError, KeyError, ValueError) as exc:
        print(json.dumps({"success": False, "error": str(exc)}, indent=2), file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(manifest, indent=2, ensure_ascii=False))
    if args.human:
        current = load_current_session(args.project_dir)
        order = current.get("task_order") or []
        active = current.get("active_task_id")
        idx = order.index(active) + 1 if active in order else 0
        obs = manifest.get("observation", "")
        print(
            f"[READY] {manifest.get('project_id')} {manifest.get('task_id')} ({obs}) "
            f"— {idx}/{len(order)}",
            file=sys.stderr,
        )
    return 0


def cmd_session_verify(args: argparse.Namespace) -> int:
    _ensure_bench_oracle_lib()
    try:
        result = verify_session(
            args.project_dir,
            args.run_id,
            agent_answer=args.agent_answer,
        )
    except (SessionError, FileNotFoundError, KeyError, ValueError) as exc:
        print(json.dumps({"success": False, "error": str(exc)}, indent=2), file=sys.stderr)
        return 1
    _print_run_result(
        result,
        OracleRunner(args.project_dir),
        result.task_id,
        human=args.human,
        quiet=args.quiet,
        json_output=args.json,
    )
    return 0 if result.passed else 1


def cmd_session_stop(args: argparse.Namespace) -> int:
    try:
        payload = stop_session(args.project_dir, args.run_id)
    except (SessionError, FileNotFoundError) as exc:
        print(json.dumps({"success": False, "error": str(exc)}, indent=2), file=sys.stderr)
        return 1
    if args.json or not args.human:
        print(json.dumps(payload, indent=2, ensure_ascii=False))
    if args.human:
        print("[STOPPED]", file=sys.stderr)
    return 0


def cmd_session_status(args: argparse.Namespace) -> int:
    try:
        if args.run_id:
            manifest = load_manifest(args.project_dir, args.run_id)
        else:
            _, manifest = resolve_session_ref(args.project_dir, None)
    except FileNotFoundError as exc:
        print(json.dumps({"success": False, "error": str(exc)}, indent=2), file=sys.stderr)
        return 1
    print(json.dumps(manifest, indent=2, ensure_ascii=False))
    return 0


def cmd_session_current(args: argparse.Namespace) -> int:
    try:
        current = load_current_session(args.project_dir)
    except FileNotFoundError as exc:
        print(json.dumps({"success": False, "error": str(exc)}, indent=2), file=sys.stderr)
        return 1
    if args.json or not args.human:
        print(json.dumps(current, indent=2, ensure_ascii=False))
    if args.human:
        active = current.get("active_task_id", "?")
        order = current.get("task_order") or []
        pos = order.index(active) + 1 if active in order else "?"
        print(
            f"[ACTIVE] {current.get('project_id')} {active} — {pos}/{len(order)}",
            file=sys.stderr,
        )
    return 0


def _format_failed_check_line(check: CheckResult) -> str | None:
    """Turn oracle script output into: check_screen "wget" — reason"""
    tail = check.details.get("output_tail") or ""
    for line in tail.splitlines():
        line = line.strip()
        if line.startswith("[oracle] ✗"):
            return line.removeprefix("[oracle]").strip()
    text = tail.strip()
    if text.startswith("{"):
        try:
            payload = json.loads(text)
        except json.JSONDecodeError:
            payload = None
        if isinstance(payload, dict) and not payload.get("passed", True):
            ctype = payload.get("check_type", "check")
            msg = payload.get("message", "")
            details = payload.get("details") or {}
            if ctype == "screen":
                missing = details.get("missing_patterns") or []
                if missing:
                    pat = missing[0]
                    return f'check_screen "{pat}" — {msg}'
                patterns = details.get("matched_patterns") or []
                if patterns:
                    return f'check_screen — {msg}'
            if ctype == "screen_absent":
                found = details.get("found_patterns") or []
                if found:
                    pat = found[0]
                    return f'check_screen_absent "{pat}" — {msg}'
                forbidden = details.get("forbidden_patterns") or []
                if forbidden:
                    return f'check_screen_absent "{forbidden[0]}" — {msg}'
            if ctype == "shell":
                cmd = details.get("cmd", "")
                return f'check_shell "{cmd}" — {msg}'
            if ctype == "filesystem":
                return f"check_filesystem — {msg}"
            if ctype == "screen_highlight":
                text_arg = details.get("text") or "?"
                return f'check_screen_highlight --text "{text_arg}" — {msg}'
            if ctype == "answer":
                return f"check_answer — {msg}"
            return f"{ctype} — {msg}"
    if not check.passed:
        return check.message
    return None


def _print_run_result(
    result,
    runner: OracleRunner,
    task_id: str,
    *,
    human: bool,
    quiet: bool = False,
    json_output: bool = True,
) -> None:
    if quiet and human:
        status = "PASS" if result.passed else "FAIL"
        obs = runner.get_observation(task_id)
        print(f"[{status}] {result.project_id} {result.task_id} ({obs})", file=sys.stderr)
        if not result.passed:
            for c in result.checks:
                if c.passed:
                    continue
                line = _format_failed_check_line(c)
                if line:
                    print(f"  ✗ {line}", file=sys.stderr)
        return
    if json_output:
        print(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
    if human:
        status = "PASS" if result.passed else "FAIL"
        obs = runner.get_observation(task_id)
        print(f"[{status}] {result.project_id} {result.task_id} ({obs})", file=sys.stderr)
        if quiet:
            return
        for c in result.checks:
            mark = "✓" if c.passed else "✗"
            print(f"  {mark} {c.message}", file=sys.stderr)
            tail = c.details.get("output_tail", "")
            if tail and not c.passed:
                print(tail, file=sys.stderr)


def _load_fps_from_env(args: argparse.Namespace) -> dict[str, str]:
    if args.fingerprints:
        return load_fingerprints(Path(args.fingerprints))
    fp_file = os.environ.get("ORACLE_FP_FILE")
    if fp_file and Path(fp_file).is_file():
        return load_fingerprints(Path(fp_file))
    return {}


def cmd_check_filesystem(args: argparse.Namespace) -> int:
    fps = _load_fps_from_env(args)
    name = args.name_fp or (fps.get(args.name_key) if args.name_key else None)
    content = args.content_fp or (fps.get(args.content_key) if args.content_key else None)
    if args.name_only:
        content = None
    if args.content_only:
        name = None
    roots = args.roots.split(",") if args.roots else None
    result = run_filesystem_check(
        "filesystem",
        name_token=name,
        content_token=content,
        roots=roots,
        require_both=not args.any_match,
        min_hits=args.min_hits,
    )
    print(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
    return 0 if result.passed else 1


def _resolve_screen_snapshot(args: argparse.Namespace) -> str | None:
    """Prefer --snapshot; else ORACLE_SCREEN_SNAPSHOT from keyboard-agent soft-stop."""
    if getattr(args, "snapshot", None):
        return args.snapshot
    env_snap = (os.environ.get("ORACLE_SCREEN_SNAPSHOT") or "").strip()
    return env_snap or None


def cmd_check_screen(args: argparse.Namespace) -> int:
    fps = _load_fps_from_env(args)
    patterns = list(args.pattern or [])
    # Only fall back to fingerprint when oracle did not pass an explicit pattern.
    if not patterns and args.pattern_key:
        fp_val = fps.get(args.pattern_key)
        if fp_val:
            patterns.append(fp_val)
    if not patterns:
        print(
            json.dumps(
                {
                    "check_id": "screen",
                    "check_type": "screen",
                    "passed": False,
                    "message": "no screen pattern; pass --pattern or call check_screen with no args for $FP_SCREEN",
                },
                indent=2,
            )
        )
        return 1
    result = run_screen_check(
        "screen",
        patterns=patterns,
        session=args.session or os.environ.get("AGENT_TUI_SESSION"),
        fmt=args.format,
        regex=args.regex,
        snapshot_path=_resolve_screen_snapshot(args),
        require_all=not args.any_match,
    )
    print(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
    return 0 if result.passed else 1


def cmd_check_screen_absent(args: argparse.Namespace) -> int:
    fps = _load_fps_from_env(args)
    patterns = list(args.pattern or [])
    if not patterns and args.pattern_key:
        fp_val = fps.get(args.pattern_key)
        if fp_val:
            patterns.append(fp_val)
    if not patterns:
        print(
            json.dumps(
                {
                    "check_id": "screen_absent",
                    "check_type": "screen_absent",
                    "passed": False,
                    "message": "no screen pattern; pass --pattern or call check_screen_absent with an argument",
                },
                indent=2,
            )
        )
        return 1
    result = run_screen_absent_check(
        "screen_absent",
        patterns=patterns,
        session=args.session or os.environ.get("AGENT_TUI_SESSION"),
        fmt=args.format,
        regex=args.regex,
        snapshot_path=_resolve_screen_snapshot(args),
        require_all=not args.any_match,
    )
    print(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
    return 0 if result.passed else 1


def cmd_check_screen_highlight(args: argparse.Namespace) -> int:
    result = run_screen_highlight_check(
        "screen_highlight",
        text=args.text,
        session=args.session or os.environ.get("AGENT_TUI_SESSION"),
        fg=args.fg,
        bg=args.bg,
        colored=args.colored,
        highlighted=args.highlighted,
        snapshot_path=_resolve_screen_snapshot(args),
    )
    print(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
    return 0 if result.passed else 1


def cmd_check_screen_color(args: argparse.Namespace) -> int:
    result = run_screen_color_check(
        "screen_color",
        session=args.session or os.environ.get("AGENT_TUI_SESSION"),
        fg=args.fg,
        bg=args.bg,
        snapshot_path=_resolve_screen_snapshot(args),
    )
    print(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
    return 0 if result.passed else 1


def cmd_check_answer(args: argparse.Namespace) -> int:
    fps = _load_fps_from_env(args)
    expected = args.expected or fps.get(args.expected_key or "answer", "")
    answer = args.agent_answer or os.environ.get("ORACLE_AGENT_ANSWER")
    result = run_answer_check(
        "answer",
        agent_answer=answer,
        expected=expected,
        normalize=args.normalize,
    )
    print(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
    return 0 if result.passed else 1


def cmd_check_shell(args: argparse.Namespace) -> int:
    result = run_shell_check("shell", cmd=args.cmd, expect=args.expect)
    print(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
    return 0 if result.passed else 1


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="bench-oracle",
        description="TUI-Bench oracle: bench.spec.json = machine spec; oracle/*.sh = checks",
    )
    p.add_argument(
        "--project-dir",
        type=Path,
        help="Benchmark project dir (contains bench.spec.json)",
    )

    sub = p.add_subparsers(dest="command", required=True)

    lst = sub.add_parser("list", help="List tasks from bench.spec.json")
    lst.set_defaults(func=cmd_list_tasks, needs_project=True)

    bld = sub.add_parser("build", help="Build Docker image from project Dockerfile")
    bld.add_argument("--docker-image", help="Override bench.spec.json docker_image tag")
    bld.add_argument(
        "--rebuild",
        action="store_true",
        help="Force rebuild even if image exists",
    )
    bld.add_argument("--human", action="store_true", help="Stream build output to stderr")
    bld.set_defaults(func=cmd_build, needs_project=True)

    gen = sub.add_parser("gen-fingerprints", help="Generate per-run fingerprint tokens")
    gen.add_argument("task")
    gen.add_argument("--run-id")
    gen.add_argument("--stdout", action="store_true")
    gen.set_defaults(func=cmd_gen_fps, needs_project=True)

    run = sub.add_parser("run", help="Execute oracle/Txx.sh (low-level)")
    run.add_argument("task")
    run.add_argument("--run-id", help="Use fingerprints/manifest from session start")
    run.add_argument("--agent-answer")
    run.add_argument("--session", help="agent-tui session id")
    run.add_argument("--docker-container", help="Docker container for shell/filesystem checks")
    run.add_argument("--human", action="store_true")
    run.set_defaults(func=cmd_run, needs_project=True)

    inj = sub.add_parser("inject-task", help="Expanded task description + fingerprints (JSON)")
    inj.add_argument("task")
    inj.add_argument("--run-id")
    inj.set_defaults(func=cmd_inject_task, needs_project=True)

    sess = sub.add_parser(
        "session",
        help="Automated benchmark session: start → (agent operates) → verify → stop",
    )
    sess_sub = sess.add_subparsers(dest="session_cmd", required=True)

    s_start = sess_sub.add_parser(
        "start",
        help="Init Docker+tmux, fingerprints, injected task; return container_id & session_id",
    )
    s_start.add_argument("task")
    s_start.add_argument("--docker-image", help="Override bench.spec.json docker_image")
    s_start.add_argument("--run-id", help="Optional fixed run id (default: auto increment+random)")
    s_start.add_argument(
        "--skip-docker",
        action="store_true",
        help="Only write manifest/task files; do not start docker/tmux",
    )
    s_start.add_argument(
        "--no-build",
        action="store_true",
        help="Do not auto-build missing docker image",
    )
    s_start.add_argument(
        "--rebuild",
        action="store_true",
        help="Force docker build before starting session",
    )
    s_start.add_argument("--human", action="store_true")
    s_start.add_argument("--json", action="store_true", help="Also print full session JSON to stdout")
    s_start.set_defaults(func=cmd_session_start, needs_project=True)

    s_start_all = sess_sub.add_parser(
        "start-all",
        help="Fingerprint all tasks and start the first one (suite mode)",
    )
    s_start_all.add_argument("--docker-image", help="Override bench.spec.json docker_image")
    s_start_all.add_argument("--skip-docker", action="store_true")
    s_start_all.add_argument("--no-build", action="store_true")
    s_start_all.add_argument("--rebuild", action="store_true")
    s_start_all.add_argument("--human", action="store_true")
    s_start_all.add_argument("--json", action="store_true")
    s_start_all.set_defaults(func=cmd_session_start_all, needs_project=True)

    s_next = sess_sub.add_parser("next", help="Stop current task and start the next in suite")
    s_next.add_argument("--docker-image")
    s_next.add_argument("--skip-docker", action="store_true")
    s_next.add_argument("--no-build", action="store_true")
    s_next.add_argument("--rebuild", action="store_true")
    s_next.add_argument("--human", action="store_true")
    s_next.add_argument("--json", action="store_true")
    s_next.set_defaults(func=cmd_session_next, needs_project=True)

    s_verify = sess_sub.add_parser("verify", help="Run oracle for active or specified session")
    s_verify.add_argument(
        "--run-id",
        help="Optional; default: .oracle/current_session.json",
    )
    s_verify.add_argument("--agent-answer")
    s_verify.add_argument("--human", action="store_true")
    s_verify.add_argument("--quiet", action="store_true", help="One-line PASS/FAIL only")
    s_verify.add_argument("--json", action="store_true", help="Print full result JSON to stdout")
    s_verify.set_defaults(func=cmd_session_verify, needs_project=True, quiet=False, json=False)

    s_stop = sess_sub.add_parser("stop", help="Remove docker container and kill tmux session")
    s_stop.add_argument(
        "--run-id",
        help="Optional; default: .oracle/current_session.json",
    )
    s_stop.add_argument("--human", action="store_true")
    s_stop.add_argument("--json", action="store_true")
    s_stop.set_defaults(func=cmd_session_stop, needs_project=True, human=False, json=False)

    s_status = sess_sub.add_parser("status", help="Show session manifest JSON")
    s_status.add_argument(
        "--run-id",
        help="Optional; default: .oracle/current_session.json",
    )
    s_status.set_defaults(func=cmd_session_status, needs_project=True)

    s_current = sess_sub.add_parser("current", help="Show .oracle/current_session.json")
    s_current.add_argument("--human", action="store_true")
    s_current.add_argument("--json", action="store_true")
    s_current.set_defaults(func=cmd_session_current, needs_project=True)

    ck = sub.add_parser("check", help="Primitive checks (call from oracle/*.sh)")
    ck_sub = ck.add_subparsers(dest="check_type", required=True)

    fs = ck_sub.add_parser("filesystem", help="Haystack file search by fingerprint")
    fs.add_argument("--name-fp")
    fs.add_argument("--content-fp")
    fs.add_argument("--name-key", default="file_name")
    fs.add_argument("--content-key", default="file_content")
    fs.add_argument("--fingerprints", type=Path)
    fs.add_argument("--roots", help="Comma-separated search roots")
    fs.add_argument("--any-match", action="store_true")
    fs.add_argument("--min-hits", type=int, default=1)
    fs.add_argument("--name-only", action="store_true")
    fs.add_argument("--content-only", action="store_true")
    fs.set_defaults(func=cmd_check_filesystem)

    sc = ck_sub.add_parser("screen", help="agent-tui snapshot text match")
    sc.add_argument("--pattern", action="append")
    sc.add_argument("--pattern-key", default="screen")
    sc.add_argument("--fingerprints", type=Path)
    sc.add_argument("--session")
    sc.add_argument("--format", default="plain")
    sc.add_argument("--snapshot")
    sc.add_argument("--regex", action="store_true")
    sc.add_argument("--any-match", action="store_true")
    sc.set_defaults(func=cmd_check_screen)

    sca = ck_sub.add_parser("screen-absent", help="agent-tui snapshot must NOT contain pattern")
    sca.add_argument("--pattern", action="append")
    sca.add_argument("--pattern-key", default="screen")
    sca.add_argument("--fingerprints", type=Path)
    sca.add_argument("--session")
    sca.add_argument("--format", default="plain")
    sca.add_argument("--snapshot")
    sca.add_argument("--regex", action="store_true")
    sca.add_argument("--any-match", action="store_true")
    sca.set_defaults(func=cmd_check_screen_absent)

    shl = ck_sub.add_parser("screen-highlight", help="Semantic snapshot highlight/color check")
    shl.add_argument("--text", required=True)
    shl.add_argument("--fg")
    shl.add_argument("--bg")
    shl.add_argument("--colored", action="store_true")
    shl.add_argument("--highlighted", action="store_true")
    shl.add_argument("--session")
    shl.add_argument("--snapshot")
    shl.set_defaults(func=cmd_check_screen_highlight)

    scl = ck_sub.add_parser("screen-color", help="Semantic snapshot color check")
    scl.add_argument("--fg")
    scl.add_argument("--bg")
    scl.add_argument("--session")
    scl.add_argument("--snapshot")
    scl.set_defaults(func=cmd_check_screen_color)

    ans = ck_sub.add_parser("answer", help="Compare agent answer to fingerprint")
    ans.add_argument("--expected")
    ans.add_argument("--expected-key", default="answer")
    ans.add_argument("--agent-answer")
    ans.add_argument("--fingerprints", type=Path)
    ans.add_argument("--normalize", default="strip_lower")
    ans.set_defaults(func=cmd_check_answer)

    sh = ck_sub.add_parser("shell", help="Run shell command and match expect")
    sh.add_argument("--cmd", required=True)
    sh.add_argument("--expect", default="")
    sh.set_defaults(func=cmd_check_shell)

    return p


def main(argv: list[str] | None = None) -> None:
    parser = build_parser()
    args = parser.parse_args(argv)
    if getattr(args, "needs_project", False):
        if not args.project_dir:
            parser.error("--project-dir is required for this command")
        args.project_dir = Path(args.project_dir).resolve()
    sys.exit(args.func(args))


if __name__ == "__main__":
    main()
