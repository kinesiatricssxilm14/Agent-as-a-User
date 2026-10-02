"""The snippet record and helpers for normalising its fields."""

from __future__ import annotations

import re
from dataclasses import dataclass, field, replace
from typing import Any, Dict, Iterable, List, Optional, Sequence

__all__ = ["Snippet", "normalize_tags", "parse_tags", "format_tags", "normalize_language"]

_TAG_SPLIT = re.compile(r"[,\s]+")


def normalize_tags(tags: Optional[Iterable[str]]) -> List[str]:
    """Clean a tag iterable: strip, drop empties, de-duplicate case-insensitively.

    Order of first appearance is preserved so the list stays predictable for the
    user rather than alphabetised behind their back.
    """
    result: List[str] = []
    seen = set()
    for tag in tags or ():
        cleaned = str(tag).strip()
        if not cleaned:
            continue
        key = cleaned.casefold()
        if key in seen:
            continue
        seen.add(key)
        result.append(cleaned)
    return result


def parse_tags(text: Optional[str]) -> List[str]:
    """Parse a free-form tag string (comma and/or whitespace separated)."""
    if not text:
        return []
    return normalize_tags(_TAG_SPLIT.split(str(text)))


def format_tags(tags: Optional[Sequence[str]]) -> str:
    """Render tags the way the input field expects them back."""
    return ", ".join(normalize_tags(tags))


def normalize_language(language: Optional[str]) -> str:
    """Normalise a language name: trimmed, lower-cased, empty means unset."""
    return (language or "").strip().lower()


@dataclass
class Snippet:
    """A single stored snippet.

    ``id`` is ``None`` until the record has been written by the storage layer.
    ``created_at``/``updated_at`` are ISO-8601 UTC strings assigned by storage.
    """

    title: str = ""
    language: str = ""
    description: str = ""
    code: str = ""
    tags: List[str] = field(default_factory=list)
    id: Optional[int] = None
    created_at: Optional[str] = None
    updated_at: Optional[str] = None

    def __post_init__(self) -> None:
        self.title = (self.title or "").strip()
        self.language = normalize_language(self.language)
        self.description = self.description or ""
        self.code = self.code or ""
        self.tags = normalize_tags(self.tags)

    # -- derived helpers -------------------------------------------------
    @property
    def tag_text(self) -> str:
        """Tags rendered as a single editable string."""
        return format_tags(self.tags)

    @property
    def line_count(self) -> int:
        if not self.code:
            return 0
        return len(self.code.splitlines())

    def summary(self) -> str:
        """First non-blank line of the description, for list display."""
        for line in (self.description or "").splitlines():
            stripped = line.strip()
            if stripped:
                return stripped
        return ""

    def copy(self, **changes: Any) -> "Snippet":
        """Return a modified copy (the dataclass stays effectively immutable)."""
        return replace(self, **changes)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "id": self.id,
            "title": self.title,
            "language": self.language,
            "description": self.description,
            "code": self.code,
            "tags": list(self.tags),
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Snippet":
        return cls(
            id=data.get("id"),
            title=data.get("title", ""),
            language=data.get("language", ""),
            description=data.get("description", ""),
            code=data.get("code", ""),
            tags=data.get("tags") or [],
            created_at=data.get("created_at"),
            updated_at=data.get("updated_at"),
        )
