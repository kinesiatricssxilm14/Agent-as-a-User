"""SQLite persistence for toolh."""

from __future__ import annotations

import os
import sqlite3
from pathlib import Path
from typing import List, Optional

from .models import Snippet


def default_database_path() -> Path:
    """Return the configured database path without opening it."""
    explicit = os.environ.get("TOOLH_DB_PATH")
    if explicit:
        return Path(explicit).expanduser()

    data_dir = os.environ.get("TOOLH_DATA_DIR")
    if data_dir:
        return Path(data_dir).expanduser() / "snippets.db"

    xdg_data = os.environ.get("XDG_DATA_HOME")
    base = Path(xdg_data).expanduser() if xdg_data else Path.home() / ".local" / "share"
    return base / "toolh" / "snippets.db"


class SnippetStore:
    """Small repository around a durable SQLite database."""

    def __init__(self, path: Optional[Path] = None) -> None:
        self.path = Path(path) if path is not None else default_database_path()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._initialize()

    def _connect(self) -> sqlite3.Connection:
        connection = sqlite3.connect(str(self.path))
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA foreign_keys = ON")
        connection.execute("PRAGMA busy_timeout = 5000")
        return connection

    def _initialize(self) -> None:
        with self._connect() as connection:
            connection.execute("PRAGMA journal_mode = WAL")
            connection.execute(
                """
                CREATE TABLE IF NOT EXISTS snippets (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    title TEXT NOT NULL,
                    language TEXT NOT NULL DEFAULT '',
                    tags TEXT NOT NULL DEFAULT '',
                    description TEXT NOT NULL DEFAULT '',
                    code TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                """
            )
            connection.execute(
                "CREATE INDEX IF NOT EXISTS idx_snippets_title ON snippets(title COLLATE NOCASE)"
            )

    @staticmethod
    def _from_row(row: sqlite3.Row) -> Snippet:
        return Snippet(**dict(row))

    def list(self, query: str = "") -> List[Snippet]:
        query = query.strip()
        with self._connect() as connection:
            if not query:
                rows = connection.execute(
                    "SELECT * FROM snippets ORDER BY updated_at DESC, id DESC"
                ).fetchall()
            else:
                escaped = query.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
                pattern = f"%{escaped}%"
                rows = connection.execute(
                    """
                    SELECT * FROM snippets
                    WHERE title LIKE ? ESCAPE '\\' COLLATE NOCASE
                       OR language LIKE ? ESCAPE '\\' COLLATE NOCASE
                       OR tags LIKE ? ESCAPE '\\' COLLATE NOCASE
                       OR description LIKE ? ESCAPE '\\' COLLATE NOCASE
                       OR code LIKE ? ESCAPE '\\' COLLATE NOCASE
                    ORDER BY updated_at DESC, id DESC
                    """,
                    (pattern,) * 5,
                ).fetchall()
        return [self._from_row(row) for row in rows]

    def get(self, snippet_id: int) -> Optional[Snippet]:
        with self._connect() as connection:
            row = connection.execute(
                "SELECT * FROM snippets WHERE id = ?", (snippet_id,)
            ).fetchone()
        return self._from_row(row) if row is not None else None

    def create(
        self, title: str, language: str, tags: str, description: str, code: str
    ) -> Snippet:
        with self._connect() as connection:
            cursor = connection.execute(
                """
                INSERT INTO snippets(title, language, tags, description, code)
                VALUES (?, ?, ?, ?, ?)
                """,
                (title, language, tags, description, code),
            )
            snippet_id = int(cursor.lastrowid)
        snippet = self.get(snippet_id)
        assert snippet is not None
        return snippet

    def update(
        self,
        snippet_id: int,
        title: str,
        language: str,
        tags: str,
        description: str,
        code: str,
    ) -> Optional[Snippet]:
        with self._connect() as connection:
            cursor = connection.execute(
                """
                UPDATE snippets
                SET title = ?, language = ?, tags = ?, description = ?, code = ?,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = ?
                """,
                (title, language, tags, description, code, snippet_id),
            )
            if cursor.rowcount == 0:
                return None
        return self.get(snippet_id)

    def delete(self, snippet_id: int) -> bool:
        with self._connect() as connection:
            cursor = connection.execute("DELETE FROM snippets WHERE id = ?", (snippet_id,))
            return cursor.rowcount > 0

