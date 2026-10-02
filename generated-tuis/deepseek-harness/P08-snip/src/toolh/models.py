"""Data model for a code snippet."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional


@dataclass
class Snippet:
    """A single stored code snippet.

    ``tags`` is stored as a comma-separated string; the ``tag_list`` property
    exposes the parsed, trimmed list of individual tags.
    """

    id: Optional[int] = None
    title: str = ""
    language: str = ""
    tags: str = ""
    description: str = ""
    code: str = ""
    created_at: str = ""
    updated_at: str = ""

    @property
    def tag_list(self) -> list[str]:
        """Return the individual, trimmed tags."""
        return [tag.strip() for tag in self.tags.split(",") if tag.strip()]
