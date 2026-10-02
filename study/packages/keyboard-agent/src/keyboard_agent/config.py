from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml

from .models import ObservationMode
from .startup_wait import StartupWaitConfig


@dataclass
class LLMConfig:
    provider: str = "openai"
    model: str = "gpt-4o-mini"
    api_key: str | None = None
    base_url: str | None = None
    temperature: float = 0.2
    max_tokens: int = 512
    request_timeout: float = 90.0
    estimate_reasoning_tokens: bool = False
    # DeepSeek V4 / reasoning models: auto | enabled | disabled
    thinking: str = "auto"
    # DeepSeek V4: high | max (only sent when thinking is enabled)
    reasoning_effort: str | None = None


@dataclass
class RunConfig:
    project_dir: Path
    bench_script: Path | None = None
    output_dir: Path = field(default_factory=lambda: Path("runs"))
    max_turns: int = 200
    rebuild: bool = False
    observation_mode: ObservationMode = ObservationMode.SEMANTIC
    llm: LLMConfig = field(default_factory=LLMConfig)
    agent: str = "openai"
    startup_wait: StartupWaitConfig | None = None
    # Seconds to wait after an action before post-action observe (Agent only).
    snapshot_settle_sec: float = 1.0
    task_timeout_sec: float | None = None
    max_consecutive_agent_errors: int = 3
    max_consecutive_action_errors: int = 3

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> RunConfig:
        llm_raw = data.get("llm") or {}
        llm = LLMConfig(
            provider=llm_raw.get("provider", "openai"),
            model=llm_raw.get("model", "gpt-4o-mini"),
            api_key=(
                llm_raw.get("api_key")
                or os.environ.get("DEEPSEEK_API_KEY")
                or os.environ.get("OPENAI_API_KEY")
            ),
            base_url=llm_raw.get("base_url") or os.environ.get("OPENAI_BASE_URL"),
            temperature=float(llm_raw.get("temperature", 0.2)),
            max_tokens=int(llm_raw.get("max_tokens", 512)),
            request_timeout=float(llm_raw.get("request_timeout", 90.0)),
            estimate_reasoning_tokens=bool(llm_raw.get("estimate_reasoning_tokens", False)),
            thinking=str(llm_raw.get("thinking", "auto")),
            reasoning_effort=llm_raw.get("reasoning_effort"),
        )
        project = data.get("project_dir") or os.environ.get("BENCH_PROJECT")
        if not project:
            raise ValueError("project_dir or BENCH_PROJECT is required")
        bench = data.get("bench_script") or os.environ.get("BENCH")
        max_turns = int(data.get("max_turns", data.get("max_steps", 200)))
        obs_raw = data.get(
            "observation_mode",
            data.get("snapshot_format", ObservationMode.SEMANTIC.value),
        )
        startup_raw = data.get("startup") or data.get("startup_wait")
        startup_wait = None
        if isinstance(startup_raw, dict):
            startup_wait = StartupWaitConfig(
                timeout_sec=float(startup_raw.get("timeout_sec", 180.0)),
                stable_sec=float(startup_raw.get("stable_sec", 2.0)),
                poll_interval_sec=float(startup_raw.get("poll_interval_sec", 0.5)),
                min_text_len=int(startup_raw.get("min_text_len", 40)),
                ready_pattern=startup_raw.get("ready_pattern"),
            )
        task_timeout_raw = data.get("task_timeout_sec", data.get("task_timeout_seconds"))
        return cls(
            project_dir=Path(project).expanduser().resolve(),
            bench_script=Path(bench).expanduser().resolve() if bench else None,
            output_dir=Path(data.get("output_dir", "runs")).expanduser(),
            max_turns=max_turns,
            rebuild=bool(data.get("rebuild", False)),
            observation_mode=ObservationMode.parse(str(obs_raw)),
            llm=llm,
            agent=data.get("agent", "openai"),
            startup_wait=startup_wait,
            snapshot_settle_sec=float(
                data.get(
                    "snapshot_settle_sec",
                    data.get("snapshot_min_interval_sec", 1.0),
                )
            ),
            task_timeout_sec=(
                float(task_timeout_raw) if task_timeout_raw is not None else None
            ),
            max_consecutive_agent_errors=int(
                data.get("max_consecutive_agent_errors", 3)
            ),
            max_consecutive_action_errors=int(
                data.get("max_consecutive_action_errors", 3)
            ),
        )

    @classmethod
    def load(cls, path: str | Path) -> RunConfig:
        raw = yaml.safe_load(Path(path).read_text())
        if not isinstance(raw, dict):
            raise ValueError(f"Config must be a YAML mapping: {path}")
        return cls.from_dict(raw)
