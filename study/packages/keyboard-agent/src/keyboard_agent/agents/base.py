from __future__ import annotations

from abc import ABC, abstractmethod

from ..models import AgentTurn, ScreenObservation, TaskInfo


class KeyboardAgent(ABC):
    """Fresh instance per task — Observe-Act loop, no cross-task memory."""

    @property
    @abstractmethod
    def agent_id(self) -> str:
        ...

    @abstractmethod
    def begin_task(self, task: TaskInfo) -> None:
        ...

    @abstractmethod
    def decide(self, observation: ScreenObservation) -> AgentTurn:
        """One LLM call → one action."""

    def record_action_feedback(
        self,
        *,
        ok: bool,
        error: str | None,
        has_changed: bool | None,
        raw_response: str,
    ) -> None:
        """Optional hook after environment step."""
