from __future__ import annotations

from dataclasses import asdict, dataclass, field
from enum import Enum
from typing import Any, Literal


ActionKind = Literal["press", "type", "wait", "submit"]
StopReason = Literal[
    "agent_submit",
    "max_turns",
    "container_exit",
    "session_lost",
    "task_timeout",
    "agent_error_limit",
    "action_error_limit",
]


class ObservationMode(str, Enum):
    """Screen content format sent to the LLM each turn."""

    PLAIN = "plain"
    SEMANTIC = "semantic"
    PNG = "png"
    SVG = "svg"

    @classmethod
    def parse(cls, value: str) -> ObservationMode:
        try:
            return cls(value.lower())
        except ValueError as exc:
            allowed = ", ".join(m.value for m in cls)
            raise ValueError(f"observation_mode must be one of: {allowed}") from exc

    @property
    def is_image(self) -> bool:
        return self in (ObservationMode.PNG, ObservationMode.SVG)


@dataclass
class TokenUsage:
    prompt_tokens: int = 0
    completion_tokens: int = 0
    total_tokens: int = 0
    reasoning_tokens: int = 0
    estimated_reasoning_tokens: int = 0
    cached_prompt_tokens: int = 0

    @classmethod
    def from_openai(cls, usage: Any) -> TokenUsage:
        from .llm_helpers import parse_usage

        return parse_usage(usage)

    def to_dict(self) -> dict[str, Any]:
        d = asdict(self)
        d["thinking_tokens"] = self.reasoning_tokens
        d["estimated_thinking_tokens"] = self.estimated_reasoning_tokens
        return d


@dataclass
class TaskInfo:
    task_id: str
    description: str
    observation: str
    container_id: str = ""
    fingerprints: dict[str, str] = field(default_factory=dict)
    project_id: str = ""
    session_id: str = ""

    @classmethod
    def from_bench_task(
        cls,
        payload: dict[str, Any],
        *,
        session_id: str = "",
        project_id: str = "",
        container_id: str = "",
    ) -> TaskInfo:
        return cls(
            task_id=payload["task_id"],
            description=payload["description"],
            observation=payload.get("observation", ""),
            fingerprints=payload.get("fingerprints") or {},
            session_id=session_id,
            project_id=project_id,
            container_id=container_id,
        )

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass
class AgentAction:
    action: ActionKind
    key: str | None = None
    text: str | None = None
    ms: int | None = None
    pattern: str | None = None

    def to_dict(self) -> dict[str, Any]:
        out: dict[str, Any] = {"action": self.action}
        if self.key is not None:
            out["key"] = self.key
        if self.text is not None:
            out["text"] = self.text
        if self.ms is not None:
            out["ms"] = self.ms
        if self.pattern is not None:
            out["pattern"] = self.pattern
        return out


@dataclass
class ScreenObservation:
    """One screen capture: LLM payload + trajectory artifacts."""

    turn: int
    mode: str
    plain_text: str = ""
    text: str | None = None
    image_relative: str | None = None
    svg_file: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass
class AgentTurn:
    raw_response: str
    action: AgentAction | None = None
    parse_error: str | None = None
    token_usage: TokenUsage | None = None
    llm_latency_ms: float | None = None
    llm_error: str | None = None
    reasoning_content: str | None = None


@dataclass
class TurnRecord:
    turn: int
    started_at: str
    ended_at: str
    duration_ms: float
    llm_latency_ms: float | None
    token_usage: TokenUsage | None
    action: dict[str, Any]
    action_ok: bool
    action_error: str | None
    has_changed: bool | None
    screen_text: str
    observation_mode: str
    svg_file: str | None
    llm_image_file: str | None
    raw_agent_response: str
    container_alive: bool
    reasoning_content: str | None = None

    def to_dict(self) -> dict[str, Any]:
        d = asdict(self)
        if self.token_usage is not None:
            d["token_usage"] = self.token_usage.to_dict()
        else:
            d.pop("token_usage", None)
        if self.reasoning_content is None:
            d.pop("reasoning_content", None)
        return d


@dataclass
class VerifyResult:
    passed: bool
    line: str
    details: list[str] = field(default_factory=list)
    recorded_at: str = ""

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass
class TaskRunResult:
    task_id: str
    description: str
    observation: str
    passed: bool
    verify: VerifyResult
    turns: list[TurnRecord] = field(default_factory=list)
    stop_reason: StopReason = "agent_submit"
    agent_id: str = ""
    started_at: str = ""
    ended_at: str = ""
    total_token_usage: TokenUsage = field(default_factory=TokenUsage)

    def to_dict(self) -> dict[str, Any]:
        return {
            "task_id": self.task_id,
            "description": self.description,
            "observation": self.observation,
            "passed": self.passed,
            "stop_reason": self.stop_reason,
            "agent_id": self.agent_id,
            "started_at": self.started_at,
            "ended_at": self.ended_at,
            "turn_count": len(self.turns),
            "total_token_usage": self.total_token_usage.to_dict(),
            "verify": self.verify.to_dict(),
            "turns": [t.to_dict() for t in self.turns],
        }
