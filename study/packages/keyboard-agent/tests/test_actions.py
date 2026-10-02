import pytest

from keyboard_agent.actions import parse_agent_json, validate_action_payload, DEFAULT_WAIT_MS
from keyboard_agent.models import ObservationMode, TokenUsage


def test_observation_mode_parse():
    assert ObservationMode.parse("semantic") == ObservationMode.SEMANTIC
    assert ObservationMode.parse("PNG") == ObservationMode.PNG
    assert ObservationMode.SEMANTIC.is_image is False
    assert ObservationMode.PNG.is_image is True


def test_prompt_lists_all_press_keys():
    from keyboard_agent.actions import VALID_KEYS, format_valid_keys_for_prompt
    from keyboard_agent.agents.prompts import build_system_prompt

    block = format_valid_keys_for_prompt()
    for key in ("ctrl+a", "ctrl+z", "alt+a", "alt+m", "arrow_up", "enter", "f10", "n", "7", "/"):
        assert key in block
    prompt = build_system_prompt("install tree", observation_mode=ObservationMode.SEMANTIC)
    assert "ALLOWED press keys" in prompt
    assert "press vs type" in prompt
    assert "ctrl+a" in prompt
    assert "alt+i" in prompt
    assert "█" in prompt
    assert str(len(VALID_KEYS)) in prompt


def test_prompt_cursor_by_observation_mode():
    from keyboard_agent.agents.prompts import build_system_prompt

    plain = build_system_prompt("t", observation_mode=ObservationMode.PLAIN)
    semantic = build_system_prompt("t", observation_mode=ObservationMode.SEMANTIC)
    svg = build_system_prompt("t", observation_mode=ObservationMode.SVG)

    assert "plain text observations" in plain
    assert "█" in plain
    assert "already rendered in the screenshot" not in plain

    assert "semantic text observations" in semantic
    assert "<fg:red>" in semantic
    assert "already rendered in the screenshot" not in semantic

    assert "image observations" in svg
    assert "already rendered in the screenshot" in svg
    assert "█" not in svg


def test_press_single_letter():
    a = parse_agent_json('{"action": "press", "key": "n"}')
    assert a.action == "press"
    assert a.key == "n"


def test_press_digit():
    a = parse_agent_json('{"action": "press", "key": "3"}')
    assert a.action == "press"
    assert a.key == "3"


def test_press_slash():
    a = parse_agent_json('{"action": "press", "key": "/"}')
    assert a.action == "press"
    assert a.key == "/"


def test_press_alt_key():
    a = parse_agent_json('{"action": "press", "key": "alt+i"}')
    assert a.action == "press"
    assert a.key == "alt+i"


def test_press_shift_punctuation_rejected():
    with pytest.raises(ValueError, match="Invalid key"):
        parse_agent_json('{"action": "press", "key": "?"}')

    a = parse_agent_json('{"action": "press", "key": "enter"}')
    assert a.action == "press"
    assert a.key == "enter"


def test_press_invalid_key():
    with pytest.raises(ValueError, match="Invalid key"):
        parse_agent_json('{"action": "press", "key": "foo"}')


def test_type():
    a = parse_agent_json('{"action": "type", "text": "tree\\n"}')
    assert a.text == "tree\n"


def test_wait_defaults():
    a = parse_agent_json('{"action": "wait"}')
    assert a.ms == DEFAULT_WAIT_MS


def test_submit():
    a = parse_agent_json('{"action": "submit"}')
    assert a.action == "submit"


def test_rejects_extra_fields():
    with pytest.raises(ValueError, match="must not include"):
        validate_action_payload({"action": "submit", "key": "enter"})


def test_token_usage_from_openai_like():
    class Details:
        reasoning_tokens = 42

    class Usage:
        prompt_tokens = 100
        completion_tokens = 50
        total_tokens = 150
        completion_tokens_details = Details()
        prompt_tokens_details = None

    u = TokenUsage.from_openai(Usage())
    assert u.reasoning_tokens == 42
    assert u.to_dict()["thinking_tokens"] == 42
    assert u.total_tokens == 150
