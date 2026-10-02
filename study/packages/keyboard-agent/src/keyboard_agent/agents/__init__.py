from __future__ import annotations

import json
import time
from pathlib import Path
from typing import TYPE_CHECKING

from ..actions import parse_agent_json
from ..llm_helpers import (
    assistant_message_dict,
    build_chat_kwargs,
    estimate_reasoning_tokens,
    parse_chat_response,
)
from ..models import AgentAction, AgentTurn, ObservationMode, ScreenObservation, TaskInfo, TokenUsage
from .base import KeyboardAgent
from .observation import build_llm_observation_content
from .prompts import build_system_prompt, format_action_result_message

if TYPE_CHECKING:
    from ..config import RunConfig


class ScriptAgent(KeyboardAgent):
    """Deterministic JSONL script — for tests without an LLM."""

    def __init__(self, actions: list[dict], *, agent_id: str = "script") -> None:
        self._actions = [parse_agent_json(json.dumps(a)) for a in actions]
        self._index = 0
        self._agent_id = agent_id

    @property
    def agent_id(self) -> str:
        return self._agent_id

    def begin_task(self, task: TaskInfo) -> None:
        self._index = 0

    def decide(self, observation: ScreenObservation) -> AgentTurn:
        if self._index >= len(self._actions):
            action = AgentAction(action="submit")
        else:
            action = self._actions[self._index]
            self._index += 1
        raw = json.dumps(action.to_dict())
        return AgentTurn(action=action, raw_response=raw, token_usage=None, llm_latency_ms=0.0)


class OpenAIAgent(KeyboardAgent):
    """OpenAI-compatible chat completions (Observe-Act, JSON action output)."""

    def __init__(
        self,
        llm,
        *,
        run_dir: Path,
        observation_mode: ObservationMode,
        agent_id: str | None = None,
    ) -> None:
        from ..config import LLMConfig

        self._llm: LLMConfig = llm
        self._run_dir = run_dir
        self._observation_mode = observation_mode
        self._agent_id = agent_id or f"openai:{llm.model}"
        self._messages: list[dict] = []

    @property
    def agent_id(self) -> str:
        return self._agent_id

    def begin_task(self, task: TaskInfo) -> None:
        self._messages = [
            {
                "role": "system",
                "content": build_system_prompt(
                    task.description,
                    observation_mode=self._observation_mode,
                ),
            }
        ]

    def decide(self, observation: ScreenObservation) -> AgentTurn:
        note = "initial" if observation.turn == 0 else ""
        content = build_llm_observation_content(observation, run_dir=self._run_dir, note=note)
        self._messages.append({"role": "user", "content": content})
        t0 = time.perf_counter()
        try:
            raw, reasoning_content, usage = self._chat()
        except Exception as exc:
            latency_ms = (time.perf_counter() - t0) * 1000.0
            return AgentTurn(
                raw_response="",
                llm_error=str(exc),
                token_usage=None,
                llm_latency_ms=latency_ms,
            )
        latency_ms = (time.perf_counter() - t0) * 1000.0
        if raw.strip():
            self._messages.append(assistant_message_dict(raw, reasoning_content))
        try:
            action = parse_agent_json(raw)
        except ValueError as exc:
            return AgentTurn(
                raw_response=raw,
                parse_error=str(exc),
                token_usage=usage,
                llm_latency_ms=latency_ms,
                reasoning_content=reasoning_content,
            )
        return AgentTurn(
            action=action,
            raw_response=raw,
            token_usage=usage,
            llm_latency_ms=latency_ms,
            reasoning_content=reasoning_content,
        )

    def record_action_feedback(
        self,
        *,
        ok: bool,
        error: str | None,
        has_changed: bool | None,
        raw_response: str,
    ) -> None:
        self._messages.append(
            {
                "role": "user",
                "content": format_action_result_message(
                    raw_response,
                    ok=ok,
                    error=error,
                    has_changed=has_changed,
                ),
            }
        )

    def _client(self):
        try:
            from openai import OpenAI
        except ImportError as exc:
            raise RuntimeError("Install LLM support: pip install 'keyboard-agent[llm]'") from exc
        kwargs: dict = {}
        if self._llm.api_key:
            kwargs["api_key"] = self._llm.api_key
        if self._llm.base_url:
            kwargs["base_url"] = self._llm.base_url
        kwargs["timeout"] = self._llm.request_timeout
        return OpenAI(**kwargs)

    def _chat(self) -> tuple[str, str | None, TokenUsage]:
        client = self._client()
        resp = client.chat.completions.create(
            **build_chat_kwargs(self._llm, self._messages),
        )
        raw, reasoning_content, usage = parse_chat_response(resp)
        if self._llm.estimate_reasoning_tokens:
            usage.estimated_reasoning_tokens = estimate_reasoning_tokens(
                self._llm,
                reasoning_content,
            )
        return raw, reasoning_content, usage


def create_agent(
    config: RunConfig,
    task_id: str,
    *,
    run_dir: Path,
    script_path: str | None = None,
) -> KeyboardAgent:
    if config.agent == "script":
        if not script_path:
            raise ValueError("script agent requires script_path")
        lines = []
        with open(script_path, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if line and not line.startswith("#"):
                    lines.append(json.loads(line))
        return ScriptAgent(lines, agent_id=f"script:{task_id}")
    if config.agent == "openai":
        return OpenAIAgent(
            config.llm,
            run_dir=run_dir,
            observation_mode=config.observation_mode,
            agent_id=f"openai:{config.llm.model}:{task_id}",
        )
    raise ValueError(f"Unknown agent type: {config.agent}")
