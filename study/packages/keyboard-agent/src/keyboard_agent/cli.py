from __future__ import annotations

import json
import os
import sys
from pathlib import Path

import click

from .config import RunConfig
from .human.batch import DEFAULT_CN_BENCH_ROOT, default_bench_script, discover_bench_projects
from .human.runner import HumanRunConfig, HumanSuiteRunner
from .human.suite_runner import InterleavedSuiteRunner
from .human.suite_scheduler import (
    default_suite_root,
    init_suite_state,
    load_suite_state,
    save_suite_state,
    simulate_presentation_order,
    suite_state_path,
)
from .human.warm_pool import (
    WarmPool,
    WarmPoolConfig,
    build_project_image,
    claim_slot_from_served_pool,
    create_warm_pool_group,
    load_project_task_ids,
    resolve_warm_pool_config,
)
from .models import ObservationMode
from .runner import SuiteRunner


def _load_config(
    config: str | None,
    *,
    project: str | None,
    bench: str | None,
    output: str,
    max_turns: int | None,
    observation_mode: str | None,
    rebuild: bool,
    agent: str,
    model: str | None,
    snapshot_settle_sec: float | None = None,
) -> RunConfig:
    # Merge CLI/env into YAML *before* from_dict: suite configs omit project_dir
    # and pass -p per project (same pattern as run-agent.sh).
    data: dict = {}
    if config:
        import yaml

        raw = yaml.safe_load(Path(config).read_text(encoding="utf-8"))
        if not isinstance(raw, dict):
            raise click.ClickException(f"Config must be a YAML mapping: {config}")
        data = dict(raw)
    data.setdefault("output_dir", output)
    data.setdefault("rebuild", rebuild)
    data.setdefault("agent", agent)
    if project:
        data["project_dir"] = project
    elif os.environ.get("BENCH_PROJECT"):
        data["project_dir"] = os.environ["BENCH_PROJECT"]
    if bench:
        data["bench_script"] = bench
    elif os.environ.get("BENCH"):
        data["bench_script"] = os.environ["BENCH"]
    if max_turns is not None:
        data["max_turns"] = max_turns
    if observation_mode is not None:
        data["observation_mode"] = observation_mode
    if snapshot_settle_sec is not None:
        data["snapshot_settle_sec"] = snapshot_settle_sec
    cfg = RunConfig.from_dict(data)
    if model:
        cfg.llm.model = model
    cfg.agent = agent
    if rebuild:
        cfg.rebuild = True
    return cfg


def _resolve_bench_script(cfg: RunConfig) -> Path:
    """bench.sh path: config/env > monorepo discovery."""
    if cfg.bench_script is not None:
        path = cfg.bench_script.expanduser().resolve()
        if path.is_file():
            return path
    return default_bench_script()


def _load_human_config(
    config: str | None,
    *,
    idle_ms: float,
    submit_key: str,
    operator: str | None,
    host: str,
    port: int,
    no_browser: bool,
    task_pane_percent: int = -1,
) -> HumanRunConfig:
    """CLI --operator wins over config YAML (supports multiple operators per machine)."""
    effective_operator = operator or "human"
    human_cfg = HumanRunConfig(
        type_idle_ms=idle_ms,
        submit_key=submit_key,
        operator_id=effective_operator,
        web_host=host,
        web_port=port,
        open_browser=not no_browser,
        task_pane_percent=task_pane_percent,
    )
    if config:
        import yaml

        raw = yaml.safe_load(Path(config).read_text(encoding="utf-8"))
        if isinstance(raw, dict) and isinstance(raw.get("human"), dict):
            h = raw["human"]
            yaml_operator = h.get("operator_id")
            if operator is not None:
                effective_operator = operator
            elif yaml_operator is not None:
                effective_operator = str(yaml_operator)
            human_cfg = HumanRunConfig(
                type_idle_ms=float(h.get("type_idle_ms", h.get("idle_threshold_ms", idle_ms))),
                submit_key=str(h.get("submit_key", submit_key)),
                operator_id=effective_operator,
                web_host=str(h.get("web_host", host)),
                web_port=int(h.get("web_port", port)),
                open_browser=bool(h.get("open_browser", not no_browser)) and not no_browser,
                task_pane_percent=int(h.get("task_pane_percent", task_pane_percent)),
            )
    return human_cfg


@click.group()
@click.version_option(package_name="keyboard-agent")
def main() -> None:
    """Keyboard Agent — LLM Observe-Act harness for TUI-Bench."""


@main.command("run")
@click.option("--config", "-c", type=click.Path(exists=True), help="YAML config file")
@click.option("--project", "-p", type=click.Path(exists=True), help="BENCH_PROJECT directory")
@click.option("--bench", type=click.Path(exists=True), help="Path to bench.sh")
@click.option("--output", "-o", type=click.Path(), default="runs", help="Run logs directory")
@click.option(
    "--max-turns",
    type=int,
    default=None,
    help="Max LLM turns per task (default: config file, else 200)",
)
@click.option(
    "--observation-mode",
    type=click.Choice(["plain", "semantic", "png", "svg"]),
    default=None,
    help="Screen format sent to LLM each turn",
)
@click.option("--rebuild", is_flag=True, help="Rebuild Docker image before start")
@click.option("--task", "-t", default=None, help="Single task e.g. T01 (auto start → agent → stop)")
@click.option("--agent", type=click.Choice(["openai", "script"]), default="openai")
@click.option("--script", type=click.Path(exists=True), help="JSONL action script (script agent)")
@click.option("--model", default=None, help="LLM model (overrides config)")
@click.option(
    "--current-only",
    is_flag=True,
    help="Use existing bench session only (manual: bench start T01 first)",
)
@click.option(
    "--snapshot-settle",
    type=float,
    default=None,
    help="Seconds to wait after each action before snapshot (default 1.0; 0 to disable)",
)
@click.option(
    "--run-dir",
    type=click.Path(),
    default=None,
    help="Reuse/create this suite directory (nests tasks as Pxx-slug/Txx)",
)
def run_cmd(
    config: str | None,
    project: str | None,
    bench: str | None,
    output: str,
    max_turns: int | None,
    observation_mode: str | None,
    rebuild: bool,
    task: str | None,
    agent: str,
    script: str | None,
    model: str | None,
    current_only: bool,
    snapshot_settle: float | None,
    run_dir: str | None,
) -> None:
    """Run keyboard agent on TUI-Bench tasks."""
    cfg = _load_config(
        config,
        project=project,
        bench=bench,
        output=output,
        max_turns=max_turns,
        observation_mode=observation_mode,
        rebuild=rebuild,
        agent=agent,
        model=model,
        snapshot_settle_sec=snapshot_settle,
    )

    runner = SuiteRunner(cfg, script_path=script, run_dir=run_dir)
    try:
        if task:
            result = runner.run_task(task.upper())
            click.echo(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
        elif current_only:
            result = runner.run_current_task()
            click.echo(json.dumps(result.to_dict(), indent=2, ensure_ascii=False))
        else:
            summary = runner.run_suite()
            click.echo(json.dumps(summary, indent=2, ensure_ascii=False))
            click.echo(f"Run artifacts: {summary['run_dir']}", err=True)
    except Exception as exc:
        click.echo(f"Error: {exc}", err=True)
        sys.exit(1)


@main.group("human")
def human_group() -> None:
    """Human operator recording for TUI-Bench (keystroke + operation logging)."""


@human_group.command("run-cn")
@click.option(
    "--root",
    type=click.Path(exists=True, file_okay=False),
    default=str(DEFAULT_CN_BENCH_ROOT),
    show_default=True,
    help="Root containing Pxx-* benchmark projects",
)
@click.option("--config", "-c", type=click.Path(exists=True), help="YAML config file")
@click.option("--bench", type=click.Path(exists=True), default=None, help="Path to bench.sh")
@click.option("--output", "-o", type=click.Path(), default="runs", help="Run logs directory")
@click.option(
    "--observation-mode",
    type=click.Choice(["plain", "semantic", "png", "svg"]),
    default="semantic",
    show_default=True,
    help="Screen format for trajectory SVG/text",
)
@click.option("--rebuild", is_flag=True, help="Rebuild Docker image before each project")
@click.option("--from-project", default=None, help="Start at project name, e.g. P08-rura")
@click.option("--continue-on-error", is_flag=True, help="Continue with next project if a project fails")
@click.option("--dry-run", is_flag=True, help="Print project order without running")
@click.option(
    "--idle-ms",
    type=float,
    default=1000.0,
    show_default=True,
    help="Type idle gap (ms) that closes one type turn",
)
@click.option(
    "--submit-key",
    default="ctrl+g ctrl+g ctrl+g",
    show_default=True,
    help="Space/comma separated key sequence that submits the task (not sent to TUI)",
)
@click.option("--operator", default=None, show_default="human", help="Operator id (separate state per operator)")
@click.option("--web", is_flag=True, help="Open browser UI instead of terminal attach")
@click.option("--host", default="127.0.0.1", show_default=True, help="Web UI bind address")
@click.option("--port", type=int, default=8765, show_default=True, help="Web UI port")
@click.option("--no-browser", is_flag=True, help="With --web, do not auto-open browser")
def human_run_cn_cmd(
    root: str,
    config: str | None,
    bench: str | None,
    output: str,
    observation_mode: str,
    rebuild: bool,
    from_project: str | None,
    continue_on_error: bool,
    dry_run: bool,
    idle_ms: float,
    submit_key: str,
    operator: str | None,
    web: bool,
    host: str,
    port: int,
    no_browser: bool,
) -> None:
    """Run one human operator through all Chinese final benchmark projects."""
    try:
        projects = discover_bench_projects(root)
    except ValueError as exc:
        raise click.ClickException(str(exc)) from exc

    if from_project:
        names = [project.name for project in projects]
        if from_project not in names:
            raise click.ClickException(f"--from-project not found: {from_project}")
        projects = projects[names.index(from_project):]

    bench_path = Path(bench).expanduser().resolve() if bench else default_bench_script()
    if not bench_path.is_file():
        raise click.ClickException(f"bench.sh not found: {bench_path}")

    human_cfg = _load_human_config(
        config,
        idle_ms=idle_ms,
        submit_key=submit_key,
        operator=operator,
        host=host,
        port=port,
        no_browser=no_browser,
    )
    interface = "web" if web else "terminal"

    click.echo(f"Human CN batch root: {Path(root).expanduser().resolve()}", err=True)
    click.echo(f"Logs directory: {Path(output).expanduser().resolve()}", err=True)
    click.echo(f"Projects: {len(projects)}", err=True)
    click.echo(f"Operator: {human_cfg.operator_id}", err=True)
    click.echo(f"Submit sequence: {human_cfg.submit_key.upper()}", err=True)

    summaries: list[dict] = []
    for idx, project_dir in enumerate(projects, start=1):
        click.echo(f"\n=== [{idx}/{len(projects)}] {project_dir.name} ===", err=True)
        if dry_run:
            click.echo(
                "keyboard-agent human run "
                f"--project {project_dir} --bench {bench_path} --operator {human_cfg.operator_id}",
                err=True,
            )
            continue

        cfg = _load_config(
            config,
            project=str(project_dir),
            bench=str(bench_path),
            output=output,
            max_turns=200,
            observation_mode=observation_mode,
            rebuild=rebuild,
            agent="openai",
            model=None,
        )
        runner = HumanSuiteRunner(cfg, human=human_cfg)
        try:
            summary = runner.run_suite(interface=interface)
            summaries.append(summary)
            click.echo(
                f"Project done: {project_dir.name} "
                f"passed={summary['passed']}/{summary['task_count']} "
                f"logs={summary['run_dir']}",
                err=True,
            )
        except Exception as exc:
            click.echo(f"Project failed: {project_dir.name}: {exc}", err=True)
            if not continue_on_error:
                raise click.ClickException(str(exc)) from exc

    if not dry_run:
        payload = {
            "root": str(Path(root).expanduser().resolve()),
            "operator_id": human_cfg.operator_id,
            "project_count": len(summaries),
            "task_count": sum(int(s.get("task_count", 0)) for s in summaries),
            "passed": sum(int(s.get("passed", 0)) for s in summaries),
            "projects": summaries,
        }
        click.echo(json.dumps(payload, indent=2, ensure_ascii=False))


@human_group.command("run-suite")
@click.option(
    "--root",
    type=click.Path(exists=True, file_okay=False),
    default=str(default_suite_root()),
    show_default=True,
    help="Suite root containing Pxx-* projects (e.g. curated_tui_suite)",
)
@click.option("--config", "-c", type=click.Path(exists=True), help="YAML config file")
@click.option("--bench", type=click.Path(exists=True), default=None, help="Path to bench.sh")
@click.option("--output", "-o", type=click.Path(), default="runs", help="Run logs directory")
@click.option(
    "--observation-mode",
    type=click.Choice(["plain", "semantic", "png", "svg"]),
    default="semantic",
    show_default=True,
    help="Screen format for trajectory SVG/text",
)
@click.option("--rebuild", is_flag=True, help="Rebuild Docker image before each task")
@click.option("--seed", type=int, default=None, help="Random seed for project interleaving")
@click.option("--resume", is_flag=True, help="Resume from saved suite state for this operator")
@click.option("--continue-on-error", is_flag=True, help="Continue after a task failure")
@click.option("--dry-run", is_flag=True, help="Print simulated presentation order without running")
@click.option(
    "--idle-ms",
    type=float,
    default=1000.0,
    show_default=True,
    help="Type idle gap (ms) that closes one type turn",
)
@click.option(
    "--submit-key",
    default="ctrl+g ctrl+g ctrl+g",
    show_default=True,
    help="Space/comma separated key sequence that submits the task (not sent to TUI)",
)
@click.option("--operator", default=None, show_default="human", help="Operator id (separate state per operator)")
@click.option("--web", is_flag=True, help="Open browser UI instead of terminal attach")
@click.option("--host", default="127.0.0.1", show_default=True, help="Web UI bind address")
@click.option("--port", type=int, default=8765, show_default=True, help="Web UI port")
@click.option("--no-browser", is_flag=True, help="With --web, do not auto-open browser")
@click.option(
    "--lookahead",
    type=int,
    default=3,
    show_default=True,
    help="Pre-warm this many upcoming tasks in background (0=off)",
)
@click.option(
    "--no-confirm",
    is_flag=True,
    help="Do not wait for Enter between tasks",
)
@click.option(
    "--retry-failed",
    is_flag=True,
    help="Re-run only tasks that failed in the saved operator state",
)
@click.option(
    "--task-pane-percent",
    type=int,
    default=-1,
    show_default=True,
    help="Task bar: -1=auto by terminal size, 0=hide, >0=max height cap (percent)",
)
def human_run_suite_cmd(
    root: str,
    config: str | None,
    bench: str | None,
    output: str,
    observation_mode: str,
    rebuild: bool,
    seed: int | None,
    resume: bool,
    continue_on_error: bool,
    dry_run: bool,
    idle_ms: float,
    submit_key: str,
    operator: str | None,
    web: bool,
    host: str,
    port: int,
    no_browser: bool,
    lookahead: int,
    no_confirm: bool,
    retry_failed: bool,
    task_pane_percent: int,
) -> None:
    """Run interleaved human study: per-project task order, random project frontier."""
    from .human.suite_scheduler import failed_task_refs, suite_manifest

    suite_root = Path(root).expanduser().resolve()
    output_dir = Path(output).expanduser().resolve()
    bench_path = Path(bench).expanduser().resolve() if bench else default_bench_script()
    if not bench_path.is_file():
        raise click.ClickException(f"bench.sh not found: {bench_path}")

    human_cfg = _load_human_config(
        config,
        idle_ms=idle_ms,
        submit_key=submit_key,
        operator=operator,
        host=host,
        port=port,
        no_browser=no_browser,
        task_pane_percent=task_pane_percent,
    )
    state_path = suite_state_path(output_dir, operator_id=human_cfg.operator_id, suite_root=suite_root)

    manifest = suite_manifest(suite_root)
    click.echo(
        f"Suite: {manifest['project_count']} projects, {manifest['task_count']} tasks",
        err=True,
    )

    if retry_failed:
        if not state_path.is_file():
            raise click.ClickException(f"No saved state for retry: {state_path}")
        state = load_suite_state(state_path)
        failed = failed_task_refs(state)
        if not failed:
            raise click.ClickException("No failed tasks in saved state.")
        click.echo(f"Retrying {len(failed)} failed task(s)", err=True)
    elif resume:
        if not state_path.is_file():
            raise click.ClickException(f"No saved state to resume: {state_path}")
        state = load_suite_state(state_path)
        if Path(state.suite_root).resolve() != suite_root:
            raise click.ClickException(
                f"Resume state suite_root mismatch:\n  state: {state.suite_root}\n  --root: {suite_root}"
            )
        if state.operator_id != human_cfg.operator_id:
            raise click.ClickException(
                f"Resume state operator mismatch: {state.operator_id} vs {human_cfg.operator_id}"
            )
    else:
        if state_path.is_file() and not dry_run:
            raise click.ClickException(
                f"Suite state already exists for operator {human_cfg.operator_id}: {state_path}\n"
                "Use --resume to continue or delete the state file to start fresh."
            )
        import random as _random

        state = init_suite_state(
            suite_root,
            operator_id=human_cfg.operator_id,
            seed=seed if seed is not None else _random.randint(0, 2**31 - 1),
        )
        if not dry_run:
            save_suite_state(state, state_path)

    click.echo(f"Suite root: {suite_root}", err=True)
    click.echo(f"Operator: {human_cfg.operator_id}", err=True)
    click.echo(f"Seed: {state.seed}", err=True)
    click.echo(
        f"Tasks: {state.completed_count}/{state.total_tasks} done, "
        f"{state.remaining_count} remaining",
        err=True,
    )
    click.echo(f"State file: {state_path}", err=True)

    if dry_run:
        order = simulate_presentation_order(state)
        for idx, (project, task_id) in enumerate(order, start=1):
            click.echo(f"{idx:3d}. {project} {task_id}")
        payload = {
            **manifest,
            "operator_id": human_cfg.operator_id,
            "seed": state.seed,
            "total_tasks": len(order),
            "order": [{"project": p, "task_id": t} for p, t in order],
        }
        click.echo(json.dumps(payload, indent=2, ensure_ascii=False))
        return

    try:
        runner = InterleavedSuiteRunner(
            suite_root=suite_root,
            bench_script=bench_path,
            output_dir=output_dir,
            human=human_cfg,
            observation_mode=observation_mode,
            rebuild=rebuild,
            state=state,
            state_path=state_path,
            lookahead=lookahead,
            confirm_between_tasks=not no_confirm,
            retry_failed_only=retry_failed,
        )
    except ValueError as exc:
        raise click.ClickException(str(exc)) from exc

    interface = "web" if web else "terminal"
    summary = runner.run(interface=interface, continue_on_error=continue_on_error)
    click.echo(json.dumps(summary, indent=2, ensure_ascii=False))


@main.command("init-config")
@click.argument("path", default="config.yaml")
def init_config(path: str) -> None:
    """Write an example config.yaml."""
    example = Path(__file__).resolve().parents[2] / "config.example.yaml"
    dest = Path(path)
    if dest.exists():
        raise click.ClickException(f"Already exists: {dest}")
    dest.write_text(example.read_text(encoding="utf-8"), encoding="utf-8")
    click.echo(f"Wrote {dest}")


@main.command("keys")
def keys_cmd() -> None:
    """Print valid press keys."""
    from .actions import valid_keys_text

    click.echo(valid_keys_text())


@human_group.command("run")
@click.option("--config", "-c", type=click.Path(exists=True), help="YAML config file")
@click.option("--project", "-p", type=click.Path(exists=True), help="BENCH_PROJECT directory")
@click.option("--bench", type=click.Path(exists=True), help="Path to bench.sh")
@click.option("--output", "-o", type=click.Path(), default="runs", help="Run logs directory")
@click.option(
    "--observation-mode",
    type=click.Choice(["plain", "semantic", "png", "svg"]),
    default="semantic",
    show_default=True,
    help="Screen format for trajectory SVG/text",
)
@click.option("--rebuild", is_flag=True, help="Rebuild Docker image before start")
@click.option("--task", "-t", default=None, help="Single task e.g. T01")
@click.option(
    "--idle-ms",
    type=float,
    default=1000.0,
    show_default=True,
    help="Type idle gap (ms) that closes one type turn",
)
@click.option(
    "--submit-key",
    default="ctrl+g ctrl+g ctrl+g",
    show_default=True,
    help="Space/comma separated key sequence that submits the task (not sent to TUI)",
)
@click.option("--operator", default=None, show_default="human", help="Operator id (separate state per operator)")
@click.option("--web", is_flag=True, help="Open browser UI instead of terminal attach")
@click.option("--host", default="127.0.0.1", show_default=True, help="Web UI bind address")
@click.option("--port", type=int, default=8765, show_default=True, help="Web UI port")
@click.option("--no-browser", is_flag=True, help="With --web, do not auto-open browser")
@click.option(
    "--pool-size",
    type=int,
    default=0,
    show_default=True,
    help="Pre-warm N bench slots in parallel before run (0=off)",
)
@click.option(
    "--use-pool",
    is_flag=True,
    help="Use a pool already running via 'human pool serve' (requires --task)",
)
def human_run_cmd(
    config: str | None,
    project: str | None,
    bench: str | None,
    output: str,
    observation_mode: str,
    rebuild: bool,
    task: str | None,
    idle_ms: float,
    submit_key: str,
    operator: str | None,
    web: bool,
    host: str,
    port: int,
    no_browser: bool,
    pool_size: int,
    use_pool: bool,
) -> None:
    """Run human recording session on TUI-Bench tasks."""
    cfg = _load_config(
        config,
        project=project,
        bench=bench,
        output=output,
        max_turns=200,
        observation_mode=observation_mode,
        rebuild=rebuild,
        agent="openai",
        model=None,
    )
    human_cfg = _load_human_config(
        config,
        idle_ms=idle_ms,
        submit_key=submit_key,
        operator=operator,
        host=host,
        port=port,
        no_browser=no_browser,
    )
    runner = HumanSuiteRunner(cfg, human=human_cfg)
    interface = "web" if web else "terminal"

    if pool_size > 0 and use_pool:
        raise click.ClickException("Use either --pool-size or --use-pool, not both.")
    if (pool_size > 0 or use_pool) and not task:
        raise click.ClickException("--task is required with --pool-size or --use-pool.")

    task_id = task.upper() if task else None
    pool: WarmPool | None = None
    owned_pool: WarmPool | None = None
    checkout = None

    def pool_log(msg: str) -> None:
        click.echo(msg, err=True)

    try:
        if use_pool:
            assert task_id
            pool_root = WarmPool.default_pool_root(cfg.output_dir, cfg.project_dir, task_id)
            checkout = claim_slot_from_served_pool(
                pool_root=pool_root,
                task_id=task_id,
                bench_script=_resolve_bench_script(cfg),
                log=pool_log,
            )
        elif pool_size > 0:
            assert task_id
            pool_root = WarmPool.default_pool_root(cfg.output_dir, cfg.project_dir, task_id)
            pool_config, _ = resolve_warm_pool_config(
                source_project=cfg.project_dir,
                output_dir=cfg.output_dir,
                pool_size=pool_size,
                base_port=port,
                rebuild=rebuild,
                no_build=False,
            )
            owned_pool = WarmPool(
                source_project=cfg.project_dir,
                bench_script=_resolve_bench_script(cfg),
                task_id=task_id,
                pool_root=pool_root,
                config=pool_config,
                log=pool_log,
            )
            owned_pool.start_warming()
            pool = owned_pool

        if pool is not None:
            checkout = pool.acquire()

        if task_id:
            result = runner.run_task(task_id, interface=interface, checkout=checkout)
            status = "PASS" if result.passed else "FAIL"
            click.echo(f"  {result.task_id}: {status} ({result.stop_reason}, {len(result.turns)} ops)")
            click.echo(f"  Logs: {runner.recorder.run_dir}")
        else:
            summary = runner.run_suite(interface=interface)
            for t in summary.get("tasks", []):
                s = "PASS" if t.get("passed") else "FAIL"
                click.echo(f"  {t['task_id']}: {s} ({t.get('stop_reason','?')}, {t.get('turn_count','?')} ops)")
            click.echo(f"  {summary['passed']}/{summary['task_count']} passed")
            click.echo(f"  Logs: {summary['run_dir']}")
    except Exception as exc:
        click.echo(f"Error: {exc}", err=True)
        sys.exit(1)
    finally:
        if checkout is not None:
            checkout.release()
        if owned_pool is not None:
            owned_pool.shutdown()


@human_group.group("pool")
def human_pool_group() -> None:
    """Pre-warm bench containers to cut human session startup wait."""


def _resolve_pool_task_ids(
    project_dir: Path,
    *,
    task: str | None,
    all_tasks: bool,
    tasks: str | None,
) -> list[str]:
    selected = sum([bool(task), all_tasks, bool(tasks)])
    if selected != 1:
        raise click.ClickException("Specify exactly one of -t/--task, --all-tasks, or --tasks")
    if all_tasks:
        return load_project_task_ids(project_dir, all_tasks=True)
    if tasks:
        task_ids = [part.strip() for part in tasks.split(",") if part.strip()]
        if not task_ids:
            raise click.ClickException("--tasks must list at least one task ID")
        return load_project_task_ids(project_dir, task_ids=task_ids)
    assert task is not None
    return load_project_task_ids(project_dir, task_ids=[task])


@human_pool_group.command("build")
@click.option("--project", "-p", type=click.Path(exists=True), required=True)
@click.option("--bench", type=click.Path(exists=True), help="Path to bench.sh")
@click.option(
    "--if-missing",
    is_flag=True,
    help="Only build when image is missing (default: always rebuild)",
)
def human_pool_build_cmd(project: str, bench: str | None, if_missing: bool) -> None:
    """Rebuild the Docker image for a benchmark project (once, from source tree)."""
    cfg = _load_config(
        None,
        project=project,
        bench=bench,
        output="runs",
        max_turns=200,
        observation_mode="semantic",
        rebuild=not if_missing,
        agent="openai",
        model=None,
    )

    def pool_log(msg: str) -> None:
        click.echo(msg, err=True)

    build_project_image(
        source_project=cfg.project_dir,
        bench_script=_resolve_bench_script(cfg),
        rebuild=not if_missing,
        log=pool_log,
    )


@human_pool_group.command("serve")
@click.option("--project", "-p", type=click.Path(exists=True), required=True)
@click.option("--bench", type=click.Path(exists=True), help="Path to bench.sh")
@click.option("--output", "-o", type=click.Path(), default="runs")
@click.option("--task", "-t", default=None, help="Single task to pre-warm, e.g. T02")
@click.option(
    "--all-tasks",
    is_flag=True,
    help="Pre-warm every task in bench.spec.json (e.g. P05 T01–T04)",
)
@click.option(
    "--tasks",
    default=None,
    help="Comma-separated tasks to pre-warm, e.g. T01,T03",
)
@click.option("--slots", "-n", type=int, default=3, show_default=True)
@click.option("--port", type=int, default=8765, show_default=True, help="Reserved slot port base (web UI only)")
@click.option("--rebuild", is_flag=True)
def human_pool_serve_cmd(
    project: str,
    bench: str | None,
    output: str,
    task: str | None,
    all_tasks: bool,
    tasks: str | None,
    slots: int,
    port: int,
    rebuild: bool,
) -> None:
    """Keep bench instances warm until Ctrl+C (use ``human run --use-pool`` to connect)."""
    import time

    cfg = _load_config(
        None,
        project=project,
        bench=bench,
        output=output,
        max_turns=200,
        observation_mode="semantic",
        rebuild=rebuild,
        agent="openai",
        model=None,
    )
    task_ids = _resolve_pool_task_ids(
        cfg.project_dir,
        task=task,
        all_tasks=all_tasks,
        tasks=tasks,
    )
    bench_script = _resolve_bench_script(cfg)

    def pool_log(msg: str) -> None:
        click.echo(msg, err=True)

    slot_rebuild = False
    slot_no_build = False
    if rebuild:
        build_project_image(
            source_project=cfg.project_dir,
            bench_script=bench_script,
            rebuild=True,
            log=pool_log,
        )
        slot_no_build = True

    pool_config, pool_policy = resolve_warm_pool_config(
        source_project=cfg.project_dir,
        output_dir=cfg.output_dir,
        pool_size=slots,
        base_port=port,
        rebuild=slot_rebuild,
        no_build=slot_no_build,
    )
    if pool_policy.max_slots is not None and slots > pool_policy.max_slots:
        pool_log(
            f"ℹ️ {cfg.project_dir.name} English-only text {pool_policy.max_slots} English-only text slot"
            f"（-n {slots} → {pool_config.pool_size}）"
        )
    if pool_policy.single_concurrent_warm:
        pool_log(f"ℹ️ {cfg.project_dir.name} English-only text：English-only text 1 English-only text")

    group = create_warm_pool_group(
        source_project=cfg.project_dir,
        bench_script=bench_script,
        output_dir=cfg.output_dir,
        task_ids=task_ids,
        pool_size=pool_config.pool_size,
        base_port=port,
        rebuild=slot_rebuild,
        no_build=slot_no_build,
        log=pool_log,
    )
    group.start_warming()
    click.echo(f"Warm pool for {cfg.project_dir.name}: {', '.join(task_ids)}", err=True)
    click.echo(f"bench.sh: {bench_script}", err=True)
    click.echo(
        f"Connect with: keyboard-agent human run -p {project} -t <Txx> --use-pool",
        err=True,
    )
    try:
        while True:
            group.sync_from_disk()
            for line in group.status_lines():
                click.echo(line, err=True)
            time.sleep(5.0)
    except KeyboardInterrupt:
        click.echo("\nShutting down warm pool…", err=True)
        group.shutdown()


@human_pool_group.command("status")
@click.option("--project", "-p", type=click.Path(exists=True), required=True)
@click.option("--output", "-o", type=click.Path(), default="runs")
@click.option("--task", "-t", default=None, help="Single task pool to inspect")
@click.option("--all-tasks", is_flag=True, help="Show all task pools for this project")
@click.option("--tasks", default=None, help="Comma-separated tasks to inspect")
def human_pool_status_cmd(
    project: str,
    output: str,
    task: str | None,
    all_tasks: bool,
    tasks: str | None,
) -> None:
    """Show warm pool slot states."""
    project_dir = Path(project).expanduser().resolve()
    output_dir = Path(output).expanduser().resolve()
    if task and not all_tasks and not tasks:
        task_ids = [task.upper()]
    else:
        task_ids = _resolve_pool_task_ids(
            project_dir,
            task=task,
            all_tasks=all_tasks,
            tasks=tasks,
        )

    shown = False
    for task_id in task_ids:
        pool_root = WarmPool.default_pool_root(output_dir, project_dir, task_id)
        state = WarmPool.load_state(pool_root)
        if state is None:
            click.echo(f"No pool at {pool_root}")
            continue
        shown = True
        if len(task_ids) > 1:
            click.echo(f"=== {task_id} ===")
        click.echo(json.dumps(state, indent=2, ensure_ascii=False))
    if not shown and len(task_ids) == 1:
        pool_root = WarmPool.default_pool_root(output_dir, project_dir, task_ids[0])
        click.echo(f"No pool at {pool_root}")


if __name__ == "__main__":
    main()
