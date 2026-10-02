"""SQLite-backed persistent storage for snippets."""

from __future__ import annotations

import os
import sqlite3
from datetime import datetime, timezone
from typing import Optional

from .models import Snippet

_SCHEMA = """
CREATE TABLE IF NOT EXISTS snippets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT    NOT NULL,
    language    TEXT    NOT NULL DEFAULT '',
    tags        TEXT    NOT NULL DEFAULT '',
    description TEXT    NOT NULL DEFAULT '',
    code        TEXT    NOT NULL DEFAULT '',
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
)
"""


def _now() -> str:
    """Return an ISO-8601 timestamp in local time (readable in the DB)."""
    return datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")


class SnippetStore:
    """Thin persistence layer over a single SQLite database file."""

    def __init__(self, path: str) -> None:
        self.path = os.path.abspath(path)
        directory = os.path.dirname(self.path)
        os.makedirs(directory, exist_ok=True)
        self._conn = sqlite3.connect(self.path)
        self._conn.row_factory = sqlite3.Row
        self._conn.execute(_SCHEMA)
        self._conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_snippets_updated "
            "ON snippets(updated_at)"
        )
        self._conn.commit()

    @staticmethod
    def _row_to_snippet(row: sqlite3.Row) -> Snippet:
        return Snippet(
            id=row["id"],
            title=row["title"],
            language=row["language"],
            tags=row["tags"],
            description=row["description"],
            code=row["code"],
            created_at=row["created_at"],
            updated_at=row["updated_at"],
        )

    def add(self, snippet: Snippet) -> int:
        """Insert a new snippet and return its generated id."""
        now = _now()
        cursor = self._conn.execute(
            """
            INSERT INTO snippets
                (title, language, tags, description, code, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (
                snippet.title,
                snippet.language,
                snippet.tags,
                snippet.description,
                snippet.code,
                now,
                now,
            ),
        )
        self._conn.commit()
        return int(cursor.lastrowid)

    def update(self, snippet: Snippet) -> None:
        """Update every editable field of an existing snippet."""
        if snippet.id is None:
            raise ValueError("Cannot update a snippet without an id")
        self._conn.execute(
            """
            UPDATE snippets
            SET title = ?, language = ?, tags = ?, description = ?,
                code = ?, updated_at = ?
            WHERE id = ?
            """,
            (
                snippet.title,
                snippet.language,
                snippet.tags,
                snippet.description,
                snippet.code,
                _now(),
                snippet.id,
            ),
        )
        self._conn.commit()

    def rename(self, snippet_id: int, title: str) -> None:
        """Change only the title of a snippet."""
        self._conn.execute(
            "UPDATE snippets SET title = ?, updated_at = ? WHERE id = ?",
            (title, _now(), snippet_id),
        )
        self._conn.commit()

    def delete(self, snippet_id: int) -> None:
        """Remove a snippet permanently."""
        self._conn.execute("DELETE FROM snippets WHERE id = ?", (snippet_id,))
        self._conn.commit()

    def get(self, snippet_id: int) -> Optional[Snippet]:
        """Return a single snippet by id, or ``None``."""
        row = self._conn.execute(
            "SELECT * FROM snippets WHERE id = ?", (snippet_id,)
        ).fetchone()
        return self._row_to_snippet(row) if row else None

    def all(self) -> list[Snippet]:
        """Return every snippet, most recently updated first."""
        rows = self._conn.execute(
            "SELECT * FROM snippets ORDER BY updated_at DESC, id DESC"
        ).fetchall()
        return [self._row_to_snippet(row) for row in rows]

    def search(self, query: str) -> list[Snippet]:
        """Case-insensitive substring search across title, language, tags,
        description and code body."""
        like = f"%{query}%"
        rows = self._conn.execute(
            """
            SELECT * FROM snippets
            WHERE title LIKE ?
               OR language LIKE ?
               OR tags LIKE ?
               OR description LIKE ?
               OR code LIKE ?
            ORDER BY updated_at DESC, id DESC
            """,
            (like, like, like, like, like),
        ).fetchall()
        return [self._row_to_snippet(row) for row in rows]

    def count(self) -> int:
        """Return the total number of stored snippets."""
        row = self._conn.execute("SELECT COUNT(*) AS c FROM snippets").fetchone()
        return int(row["c"]) if row else 0

    def close(self) -> None:
        """Close the underlying connection (best effort)."""
        try:
            self._conn.close()
        except Exception:
            pass
