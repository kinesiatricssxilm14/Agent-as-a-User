"""OpenAI-compatible chat helpers — thinking mode + reasoning token parsing."""

from __future__ import annotations

import json
import re
import urllib.error
import urllib.request
from typing import Any
from urllib.parse import urlparse

from .config import LLMConfig
from .models import TokenUsage

_THINKING_MODEL_RE = re.compile(
    r"(deepseek-v4|deepseek-reasoner|deepseek-r1|/o1|/o3|-o1|-o3|reasoner|thinking)",
    re.I,
)
_TOKEN_ESTIMATE_OVERHEAD: dict[tuple[str, str], int] = {}


def resolve_thinking(llm: LLMConfig) -> bool | None:
    """Return True/False to set thinking, or None to omit extra_body."""
    mode = (llm.thinking or "auto").lower()
    if mode == "enabled":
        return True
    if mode == "disabled":
        return False
    if mode == "auto":
        return bool(_THINKING_MODEL_RE.search(llm.model))
    raise ValueError(f"llm.thinking must be auto|enabled|disabled, got {llm.thinking!r}")


def build_chat_kwargs(llm: LLMConfig, messages: list[dict[str, Any]]) -> dict[str, Any]:
    kwargs: dict[str, Any] = {
        "model": llm.model,
        "messages": messages,
        "temperature": llm.temperature,
        "max_tokens": llm.max_tokens,
        "response_format": {"type": "json_object"},
    }
    thinking = resolve_thinking(llm)
    if thinking is True:
        kwargs["extra_body"] = {"thinking": {"type": "enabled"}}
        if llm.reasoning_effort:
            kwargs["reasoning_effort"] = llm.reasoning_effort
    elif thinking is False:
        kwargs["extra_body"] = {"thinking": {"type": "disabled"}}
    return kwargs


def _details_get(details: Any, key: str) -> int:
    if details is None:
        return 0
    if isinstance(details, dict):
        return int(details.get(key) or 0)
    return int(getattr(details, key, 0) or 0)


def parse_usage(usage: Any) -> TokenUsage:
    """Parse provider usage; only official API-reported reasoning_tokens."""
    if usage is None:
        return TokenUsage()

    if isinstance(usage, dict):
        prompt = int(usage.get("prompt_tokens") or 0)
        completion = int(usage.get("completion_tokens") or 0)
        total = int(usage.get("total_tokens") or 0)
        reasoning = _details_get(usage.get("completion_tokens_details"), "reasoning_tokens")
        if not reasoning:
            reasoning = int(usage.get("reasoning_tokens") or 0)
        if not reasoning:
            reasoning = _details_get(usage.get("output_tokens_details"), "reasoning_tokens")
        cached = _details_get(usage.get("prompt_tokens_details"), "cached_tokens")
    else:
        prompt = int(getattr(usage, "prompt_tokens", 0) or 0)
        completion = int(getattr(usage, "completion_tokens", 0) or 0)
        total = int(getattr(usage, "total_tokens", 0) or 0)
        reasoning = _details_get(getattr(usage, "completion_tokens_details", None), "reasoning_tokens")
        if not reasoning:
            reasoning = int(getattr(usage, "reasoning_tokens", 0) or 0)
        if not reasoning:
            reasoning = _details_get(getattr(usage, "output_tokens_details", None), "reasoning_tokens")
        cached = _details_get(getattr(usage, "prompt_tokens_details", None), "cached_tokens")
        model_extra = getattr(usage, "model_extra", None) or {}
        if not reasoning and isinstance(model_extra, dict):
            reasoning = int(model_extra.get("reasoning_tokens") or 0)

    return TokenUsage(
        prompt_tokens=prompt,
        completion_tokens=completion,
        total_tokens=total,
        reasoning_tokens=reasoning,
        cached_prompt_tokens=cached,
    )


def _moonshot_estimate_url(base_url: str | None) -> str | None:
    """Return Kimi/Moonshot tokenizer endpoint for OpenAI-compatible base URLs."""
    if not base_url:
        return None
    parsed = urlparse(base_url)
    if "moonshot." not in parsed.netloc:
        return None
    return f"{parsed.scheme}://{parsed.netloc}/v1/tokenizers/estimate-token-count"


def _estimate_kimi_message_tokens(llm: LLMConfig, text: str) -> int | None:
    """Estimate tokens for one assistant message via Kimi's tokenizer API."""
    if not text.strip() or not llm.api_key:
        return None
    endpoint = _moonshot_estimate_url(llm.base_url)
    if endpoint is None or "kimi" not in llm.model.lower():
        return None
    payload = {
        "model": llm.model,
        "messages": [{"role": "assistant", "content": text}],
    }
    request = urllib.request.Request(
        endpoint,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {llm.api_key}",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30.0) as response:
            data = json.loads(response.read().decode("utf-8"))
    except (OSError, urllib.error.HTTPError, json.JSONDecodeError):
        return None
    try:
        return int(data["data"]["total_tokens"])
    except (KeyError, TypeError, ValueError):
        return None


def estimate_reasoning_tokens(llm: LLMConfig, reasoning_content: str | None) -> int:
    """Best-effort Kimi estimate for tokens in returned reasoning_content.

    Kimi reports reasoning text but not a dedicated official reasoning token
    count in usage. This value is an estimate from Kimi's tokenizer endpoint,
    with assistant-message overhead subtracted.
    """
    if not reasoning_content:
        return 0

    endpoint = _moonshot_estimate_url(llm.base_url)
    if endpoint is None:
        return 0
    cache_key = (endpoint, llm.model)
    overhead = _TOKEN_ESTIMATE_OVERHEAD.get(cache_key)
    if overhead is None:
        probe = _estimate_kimi_message_tokens(llm, "x")
        if probe is None:
            return 0
        overhead = max(0, probe - 1)
        _TOKEN_ESTIMATE_OVERHEAD[cache_key] = overhead

    estimate = _estimate_kimi_message_tokens(llm, reasoning_content)
    if estimate is None:
        return 0
    return max(0, estimate - overhead)


def extract_reasoning_content(message: Any) -> str | None:
    raw = getattr(message, "reasoning_content", None)
    if raw is None and hasattr(message, "model_extra") and message.model_extra:
        raw = message.model_extra.get("reasoning_content")
    if raw is None:
        return None
    text = str(raw).strip()
    return text or None


def parse_chat_response(resp: Any) -> tuple[str, str | None, TokenUsage]:
    message = resp.choices[0].message
    content = (getattr(message, "content", None) or "").strip()
    reasoning_content = extract_reasoning_content(message)
    usage = parse_usage(resp.usage)
    return content, reasoning_content, usage


def assistant_message_dict(content: str, _reasoning_content: str | None = None) -> dict[str, Any]:
    """Build assistant message for the next API request.

    DeepSeek official API returns 400 if ``reasoning_content`` is resent in
    history; only the final ``content`` belongs in multi-turn messages.
    """
    return {"role": "assistant", "content": content}
