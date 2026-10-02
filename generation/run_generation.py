#!/usr/bin/env python3
"""Batch-run code generation agents over benchmark prompts."""

from __future__ import annotations

import argparse
import datetime as dt
import fnmatch
import json
import os
import shutil
import subprocess
import sys
import uuid
from dataclasses import dataclass
from pathlib import Path
from string import Formatter
from typing import Any


DEFAULT_CONFIG = Path(__file__).resolve().parent / "configs" / "agents.yaml"


@dataclass(frozen=True)
class PromptTask:
    project_id: str
    prompt_path: Path


@dataclass(frozen=True)
class RunSpec:
    agent: str
    model: str
    model_cli_name: str
    project_id: str
    prompt_path: Path
    output_dir: Path
    run_dir: Path
    workspace_dir: Path

    @property
    def combo_id(self) -> str:
        return f"{self.agent}__{self.model}"


class ConfigError(RuntimeError):
    pass


def load_config(path: Path) -> dict[str, Any]:
    text = path.read_text(encoding="utf-8")
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        try:
            import yaml  # type: ignore[import-not-found]
        except ImportError as exc:
            raise ConfigError(
                f"{path} is not JSON-compatible YAML and PyYAML is not installed. "
                "Keep the config in JSON-compatible YAML or install PyYAML."
            ) from exc
        loaded = yaml.safe_load(text)
        if not isinstance(loaded, dict):
            raise ConfigError(f"{path} must contain a mapping at the top level")
        return loaded


def scan_prompts(input_root: Path) -> list[PromptTask]:
    tasks: list[PromptTask] = []
    for prompt_path in sorted(input_root.glob("P*/P*-prompt.md")):
        project_id = prompt_path.parent.name
        tasks.append(PromptTask(project_id=project_id, prompt_path=prompt_path))
    return tasks


def selected(values: list[str] | None, available: list[str], label: str) -> list[str]:
    if not values:
        return available
    missing = sorted(set(values) - set(available))
    if missing:
        raise ConfigError(f"Unknown {label}: {', '.join(missing)}")
    return values


def resolve_model_cli_name(model_config: dict[str, Any], agent: str) -> str:
    agent_names = model_config.get("agent_model_cli_name", {})
    if agent_names and not isinstance(agent_names, dict):
        raise ConfigError("agent_model_cli_name must be a mapping when provided")
    if isinstance(agent_names, dict) and agent in agent_names:
        return str(agent_names[agent])
    return str(model_config["model_cli_name"])


def build_wrapped_prompt(original_prompt: str, project_id: str) -> str:
    return f"""You are generating a complete software project for benchmark project {project_id}.

Follow the user's prompt exactly. Work only inside the current working directory.
Create the final project folder requested by the prompt, for example toolA/toolB/etc.
Do not write files outside the working directory. When finished, leave the complete
source tree in that final project folder so it can be archived as the benchmark artifact.

Experiment integrity rules:
- Do not search the web, browse GitHub/GitLab/Bitbucket, or look up existing
  implementations of this benchmark task.
- Do not clone, download, copy, vendor, or adapt an existing project as the
  answer.
- You may only use network access for language/package manager dependency
  resolution when the prompt's allowed stack requires it.
- The final artifact must be your own implementation created for this prompt.

--- BEGIN USER PROMPT ---
{original_prompt.rstrip()}
--- END USER PROMPT ---
"""


def make_run_id(agent: str, model: str, project_id: str) -> str:
    timestamp = dt.datetime.now(dt.UTC).strftime("%Y%m%dT%H%M%SZ")
    suffix = uuid.uuid4().hex[:8]
    safe_model = model.replace("/", "_")
    return f"{timestamp}-{agent}-{safe_model}-{project_id}-{suffix}"


def format_template(value: str, context: dict[str, str]) -> str:
    keys = [field for _, field, _, _ in Formatter().parse(value) if field]
    missing = [key for key in keys if key not in context]
    if missing:
        raise ConfigError(f"Unknown template fields in {value!r}: {', '.join(missing)}")
    return value.format(**context)


def expand_env_value(value: str, context: dict[str, str], *, dry_run: bool) -> str:
    if value.startswith("${") and value.endswith("}") and len(value) > 3:
        env_name = value[2:-1]
        env_value = os.environ.get(env_name)
        if env_value is None and not dry_run:
            raise ConfigError(f"Required environment variable is not set: {env_name}")
        return env_value or value
    return os.path.expandvars(format_template(value, context))


def apply_config_env(
    env: dict[str, str],
    configured_env: dict[str, Any],
    context: dict[str, str],
    *,
    dry_run: bool,
) -> None:
    for key, value in configured_env.items():
        env[key] = expand_env_value(str(value), context, dry_run=dry_run)


def render_command(command: list[Any], context: dict[str, str]) -> list[str]:
    rendered: list[str] = []
    for part in command:
        if not isinstance(part, str):
            raise ConfigError(f"Command parts must be strings, got {part!r}")
        rendered.append(format_template(part, context))
    return rendered


def render_cwd(cwd_template: str | None, context: dict[str, str]) -> Path:
    if not cwd_template:
        return Path(context["workspace_dir"])
    return Path(format_template(cwd_template, context))


def preview_command(command: list[str], context: dict[str, str]) -> str:
    preview_parts = [
        "<prompt_text>" if part == context["prompt_text"] else part for part in command
    ]
    return " ".join(preview_parts)


def render_stdin(stdin_template: str | None, context: dict[str, str]) -> str | None:
    if stdin_template is None:
        return None
    return format_template(stdin_template, context)


def resolve_artifacts(workspace_dir: Path, artifact_glob: str) -> list[Path]:
    artifact_pattern = artifact_glob.lower()
    matches = [
        path
        for path in sorted(workspace_dir.iterdir())
        if path.is_dir() and fnmatch.fnmatch(path.name.lower(), artifact_pattern)
    ]
    if matches:
        return matches

    nested: list[Path] = []
    for path in sorted(workspace_dir.rglob("*")):
        if path.is_dir() and fnmatch.fnmatch(path.name.lower(), artifact_pattern):
            nested.append(path)
    return nested


def copy_artifacts(artifacts: list[Path], output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    for artifact in artifacts:
        destination = output_dir / artifact.name
        if destination.exists():
            raise RuntimeError(f"Artifact destination already exists: {destination}")
        shutil.copytree(artifact, destination, symlinks=True)


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def prepare_output_dir(output_dir: Path, *, skip_existing: bool, overwrite: bool) -> bool:
    if not output_dir.exists():
        output_dir.mkdir(parents=True)
        return True
    if skip_existing:
        print(f"[SKIP] Existing output: {output_dir}")
        return False
    if overwrite:
        shutil.rmtree(output_dir)
        output_dir.mkdir(parents=True)
        return True
    raise RuntimeError(
        f"Output already exists: {output_dir}. Use --skip-existing or --overwrite."
    )


def build_specs(
    config: dict[str, Any],
    tasks: list[PromptTask],
    agents: list[str],
    models: list[str],
) -> list[RunSpec]:
    output_root = Path(config["output_root"]).expanduser()
    runs_root = Path(config.get("runs_root", output_root / "_runs")).expanduser()

    specs: list[RunSpec] = []
    for task in tasks:
        for agent in agents:
            for model in models:
                model_config = config["models"][model]
                run_id = make_run_id(agent, model, task.project_id)
                output_dir = output_root / f"{agent}__{model}" / task.project_id
                run_dir = runs_root / run_id
                specs.append(
                    RunSpec(
                        agent=agent,
                        model=model,
                        model_cli_name=resolve_model_cli_name(model_config, agent),
                        project_id=task.project_id,
                        prompt_path=task.prompt_path,
                        output_dir=output_dir,
                        run_dir=run_dir,
                        workspace_dir=run_dir / "workspace",
                    )
                )
    return specs


def run_one(
    spec: RunSpec,
    config: dict[str, Any],
    *,
    dry_run: bool,
    skip_existing: bool,
    overwrite: bool,
) -> bool:
    agent_config = config["agents"][spec.agent]
    model_config = config["models"][spec.model]
    artifact_glob = str(config.get("artifact_glob", "tool*"))
    timeout = int(config.get("timeout_seconds", 7200))

    original_prompt = spec.prompt_path.read_text(encoding="utf-8")
    wrapped_prompt = build_wrapped_prompt(original_prompt, spec.project_id)

    prompt_file = spec.run_dir / "prompt.md"
    original_prompt_file = spec.run_dir / "original_prompt.md"

    context = {
        "agent": spec.agent,
        "model": spec.model,
        "model_cli_name": spec.model_cli_name,
        "project_id": spec.project_id,
        "prompt_file": str(prompt_file),
        "original_prompt_file": str(original_prompt_file),
        "prompt_text": wrapped_prompt,
        "workspace_dir": str(spec.workspace_dir),
        "run_dir": str(spec.run_dir),
        "output_dir": str(spec.output_dir),
    }

    env = os.environ.copy()
    env.update(
        {
            "PROMPT_FILE": str(prompt_file),
            "WORKSPACE_DIR": str(spec.workspace_dir),
            "MODEL": spec.model_cli_name,
            "OUTPUT_DIR": str(spec.output_dir),
            "AGENT": spec.agent,
            "PROJECT_ID": spec.project_id,
        }
    )
    apply_config_env(env, agent_config.get("env", {}), context, dry_run=dry_run)
    if agent_config.get("use_model_env", True):
        apply_config_env(env, model_config.get("env", {}), context, dry_run=dry_run)

    if dry_run:
        output_ready = not spec.output_dir.exists() or overwrite or skip_existing
    else:
        output_ready = prepare_output_dir(
            spec.output_dir, skip_existing=skip_existing, overwrite=overwrite
        )
    if not output_ready:
        return True

    command = render_command(agent_config["command"], context)
    cwd = render_cwd(agent_config.get("cwd"), context)
    stdin_text = render_stdin(agent_config.get("stdin"), context)

    print(f"[RUN] {spec.combo_id} {spec.project_id}")
    print(f"      cwd: {cwd}")
    print(f"      out: {spec.output_dir}")
    print(f"      cmd: {preview_command(command, context)}")
    if stdin_text is not None:
        print("      stdin: <prompt_text>")
    if dry_run:
        return True

    spec.workspace_dir.mkdir(parents=True, exist_ok=True)
    prompt_file.write_text(wrapped_prompt, encoding="utf-8")
    original_prompt_file.write_text(original_prompt, encoding="utf-8")

    started_at = dt.datetime.now(dt.UTC)
    result = subprocess.run(
        command,
        cwd=str(cwd),
        env=env,
        input=stdin_text,
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    finished_at = dt.datetime.now(dt.UTC)

    (spec.output_dir / "stdout.log").write_text(result.stdout, encoding="utf-8")
    (spec.output_dir / "stderr.log").write_text(result.stderr, encoding="utf-8")
    (spec.output_dir / "prompt.md").write_text(wrapped_prompt, encoding="utf-8")

    artifacts = resolve_artifacts(spec.workspace_dir, artifact_glob)
    if result.returncode == 0 and artifacts:
        copy_artifacts(artifacts, spec.output_dir)

    meta = {
        "agent": spec.agent,
        "model": spec.model,
        "model_cli_name": spec.model_cli_name,
        "project_id": spec.project_id,
        "source_prompt": str(spec.prompt_path),
        "run_dir": str(spec.run_dir),
        "workspace_dir": str(spec.workspace_dir),
        "output_dir": str(spec.output_dir),
        "artifact_glob": artifact_glob,
        "artifacts": [artifact.name for artifact in artifacts],
        "command": command,
        "returncode": result.returncode,
        "started_at": started_at.isoformat(),
        "finished_at": finished_at.isoformat(),
        "duration_seconds": (finished_at - started_at).total_seconds(),
    }
    write_json(spec.output_dir / "meta.json", meta)

    if result.returncode != 0:
        print(f"[FAIL] {spec.combo_id} {spec.project_id}: exit {result.returncode}")
        return False
    if not artifacts:
        print(f"[FAIL] {spec.combo_id} {spec.project_id}: no artifacts matched {artifact_glob!r}")
        return False
    print(f"[OK] {spec.combo_id} {spec.project_id}: {', '.join(meta['artifacts'])}")
    return True


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--input-root", type=Path)
    parser.add_argument("--output-root", type=Path)
    parser.add_argument("--runs-root", type=Path)
    parser.add_argument("--only", action="append", help="Project id to run, e.g. P01-aptui")
    parser.add_argument("--agent", action="append", help="Agent id from config")
    parser.add_argument("--model", action="append", help="Model id from config")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--skip-existing", "--resume", action="store_true", dest="skip_existing")
    parser.add_argument("--overwrite", action="store_true")
    parser.add_argument("--stop-on-error", action="store_true")
    parser.add_argument("--list", action="store_true", help="List selected runs without executing")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    config = load_config(args.config)

    if args.input_root:
        config["input_root"] = str(args.input_root)
    if args.output_root:
        config["output_root"] = str(args.output_root)
        if not args.runs_root:
            config["runs_root"] = str(args.output_root.expanduser() / "_runs")
    if args.runs_root:
        config["runs_root"] = str(args.runs_root)

    input_root = Path(config["input_root"]).expanduser()
    tasks = scan_prompts(input_root)
    if not tasks:
        raise ConfigError(f"No prompt files found under {input_root}")

    if args.only:
        wanted = set(args.only)
        tasks = [task for task in tasks if task.project_id in wanted]
        missing = sorted(wanted - {task.project_id for task in tasks})
        if missing:
            raise ConfigError(f"Unknown project id: {', '.join(missing)}")

    agent_ids = selected(args.agent, sorted(config["agents"].keys()), "agent")
    model_ids = selected(args.model, sorted(config["models"].keys()), "model")
    specs = build_specs(config, tasks, agent_ids, model_ids)

    if args.list or args.dry_run:
        print(f"Selected runs: {len(specs)}")

    failed: list[RunSpec] = []
    for spec in specs:
        ok = run_one(
            spec,
            config,
            dry_run=args.dry_run or args.list,
            skip_existing=args.skip_existing,
            overwrite=args.overwrite,
        )
        if not ok:
            failed.append(spec)
            if args.stop_on_error:
                break

    if failed:
        print("\nFailed runs:", file=sys.stderr)
        for spec in failed:
            print(f"  {spec.combo_id} {spec.project_id}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except (ConfigError, RuntimeError, subprocess.TimeoutExpired) as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(2)
