from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from keyboard_agent.agents import OpenAIAgent
from keyboard_agent.models import ObservationMode, ScreenObservation, TokenUsage


def _observation() -> ScreenObservation:
    return ScreenObservation(
        turn=0,
        mode="semantic",
        plain_text="Press / to search",
        text="Press / to search",
    )


def test_openai_agent_parse_error_does_not_raise():
    agent = OpenAIAgent(
        MagicMock(model="test", api_key="k", base_url=None, temperature=0, max_tokens=100),
        run_dir=Path("/tmp"),
        observation_mode=ObservationMode.SEMANTIC,
    )
    agent.begin_task(
        MagicMock(task_id="T01", description="install tree", observation="filesystem")
    )

    with patch.object(
        agent,
        "_chat",
        return_value=('{"action": "press", "key": "?"}', None, TokenUsage()),
    ):
        turn = agent.decide(_observation())

    assert turn.parse_error is not None
    assert "Invalid key" in turn.parse_error
    assert turn.action is None


def test_openai_agent_llm_error_does_not_raise():
    agent = OpenAIAgent(
        MagicMock(model="test", api_key="k", base_url=None, temperature=0, max_tokens=100),
        run_dir=Path("/tmp"),
        observation_mode=ObservationMode.SEMANTIC,
    )
    agent.begin_task(
        MagicMock(task_id="T01", description="install tree", observation="filesystem")
    )

    with patch.object(agent, "_chat", side_effect=RuntimeError("rate limited")):
        turn = agent.decide(_observation())

    assert turn.llm_error == "rate limited"
    assert turn.action is None
