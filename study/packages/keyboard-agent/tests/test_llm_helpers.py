from keyboard_agent.config import LLMConfig
from keyboard_agent.llm_helpers import (
    assistant_message_dict,
    build_chat_kwargs,
    parse_usage,
    resolve_thinking,
)


def test_resolve_thinking_auto_deepseek():
    llm = LLMConfig(model="deepseek-v4-flash", thinking="auto")
    assert resolve_thinking(llm) is True


def test_parse_usage_reasoning_tokens_dict():
    usage = {
        "prompt_tokens": 100,
        "completion_tokens": 80,
        "total_tokens": 180,
        "completion_tokens_details": {"reasoning_tokens": 55},
    }
    parsed = parse_usage(usage)
    assert parsed.reasoning_tokens == 55


def test_parse_usage_no_reasoning_tokens():
    usage = {
        "prompt_tokens": 100,
        "completion_tokens": 80,
        "total_tokens": 180,
    }
    parsed = parse_usage(usage)
    assert parsed.reasoning_tokens == 0


def test_build_chat_kwargs_thinking_enabled():
    llm = LLMConfig(
        model="deepseek-v4-flash",
        thinking="enabled",
        reasoning_effort="high",
        max_tokens=1024,
    )
    kwargs = build_chat_kwargs(llm, [{"role": "user", "content": "hi"}])
    assert kwargs["extra_body"] == {"thinking": {"type": "enabled"}}
    assert kwargs["reasoning_effort"] == "high"


def test_assistant_message_omits_reasoning_content():
    msg = assistant_message_dict('{"action":"submit"}', "chain of thought")
    assert msg == {"role": "assistant", "content": '{"action":"submit"}'}
