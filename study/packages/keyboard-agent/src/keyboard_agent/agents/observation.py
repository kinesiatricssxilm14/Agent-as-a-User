from __future__ import annotations

import base64
from pathlib import Path

from ..models import ScreenObservation


def build_llm_observation_content(
    observation: ScreenObservation,
    *,
    run_dir: Path,
    note: str = "",
) -> str | list[dict]:
    """Build OpenAI-compatible user message content for one screen observation."""
    header = f"Turn {observation.turn} — screen"
    if note:
        header = f"{header} ({note})"

    if observation.mode in ("plain", "semantic"):
        body = observation.text or observation.plain_text or ""
        return f"{header}:\n\n{body}"

    if observation.mode in ("png", "svg") and observation.image_relative:
        image_path = run_dir / observation.image_relative
        if not image_path.is_file():
            fallback = observation.plain_text or "(image missing)"
            return f"{header}:\n\n{fallback}"
        b64 = base64.b64encode(image_path.read_bytes()).decode("ascii")
        mime = "image/png" if observation.mode == "png" else "image/svg+xml"
        caption = (
            observation.text
            or f"{header}. If a text cursor is present, it is already shown in the image."
        )
        return [
            {"type": "text", "text": caption},
            {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{b64}"}},
        ]

    return f"{header}:\n\n{observation.plain_text or ''}"
