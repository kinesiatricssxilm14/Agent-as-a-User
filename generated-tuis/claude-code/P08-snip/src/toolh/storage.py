"""Real SQLite persistence for snippets.

Everything the UI shows comes from here, and every write goes straight to the
database file inside a transaction -- there is no in-memory shadow copy that
could drift away from what is stored.

Schema (three tables, so tags are queryable rather than a mushed string)::

    snippets(id, title, language, description, code, created_at, updated_at)
    tags(id, name, name_folded UNIQUE)
    snippet_tags(snippet_id, tag_id)   -- composite PK, cascades on delete

Search is performed *by SQLite*, not by filtering Python lists, so it keeps
working when the library is large.  A query is split on whitespace and every
term must match (AND).  A term may be scoped to one field:

``title:foo`` ``lang:python`` ``tag:cli`` ``desc:greeting`` ``code:print``
``id:12``

Unscoped terms match any of title, language, description, code or tag name.
Quoted terms (``"two words"``) are kept intact, and a leading ``-`` negates.
"""

from __future__ import annotations

import re
import shlex
import sqlite3
from contextlib import closing, contextmanager
from datetime import datetime
from pathlib import Path
from typing import Iterable, Iterator, List, Optional, Sequence, Tuple

from .models import Snippet, normalize_language, normalize_tags

__all__ = ["SnippetStore", "StorageError", "SearchTerm", "parse_query", "SORT_MODES"]

SCHEMA_VERSION = 1

#: ``(key, label, ORDER BY clause)`` -- the UI cycles through these.
SORT_MODES: Tuple[Tuple[str, str, str], ...] = (
    ("updated", "Recently updated", "s.updated_at DESC, s.id DESC"),
    ("created", "Recently created", "s.created_at DESC, s.id DESC"),
    ("title", "Title A-Z", "s.title COLLATE NOCASE ASC, s.id ASC"),
    ("language", "Language A-Z", "s.language COLLATE NOCASE ASC, s.title COLLATE NOCASE ASC"),
)

_FIELD_ALIASES = {
    "title": "title",
    "name": "title",
    "lang": "language",
    "language": "language",
    "desc": "description",
    "description": "description",
    "code": "code",
    "body": "code",
    "tag": "tag",
    "tags": "tag",
    "id": "id",
}

_SCOPE_RE = re.compile(r"^(?P<field>[A-Za-z]+):(?P<value>.*)$", re.DOTALL)


class StorageError(RuntimeError):
    """Raised for problems the user can act on (bad title, missing row, ...)."""


def _utc_now() -> str:
    return datetime.utcnow().replace(microsecond=0).isoformat(timespec="seconds") + "Z"


def _like_escape(value: str) -> str:
    """Escape ``%``, ``_`` and ``\\`` so user input is matched literally."""
    return (
        value.replace("\\", "\\\\")
        .replace("%", "\\%")
        .replace("_", "\\_")
    )


class SearchTerm:
    """One parsed component of a search query."""

    __slots__ = ("field", "value", "negated")

    def __init__(self, value: str, field: Optional[str] = None, negated: bool = False) -> None:
        self.value = value
        self.field = field
        self.negated = negated

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return "SearchTerm({!r}, field={!r}, negated={!r})".format(
            self.value, self.field, self.negated
        )

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, SearchTerm):
            return NotImplemented
        return (
            self.value == other.value
            and self.field == other.field
            and self.negated == other.negated
        )


def parse_query(query: Optional[str]) -> List[SearchTerm]:
    """Split a query string into :class:`SearchTerm` objects."""
    if not query or not query.strip():
        return []
    try:
        raw_terms = shlex.split(query)
    except ValueError:
        # Unbalanced quote while the user is still typing.  Fall back to a
        # whitespace split and drop the stray quote characters, so `"git`
        # keeps matching `git` instead of searching for a literal quote.
        raw_terms = [part.strip("\"'") for part in query.split()]

    terms: List[SearchTerm] = []
    for raw in raw_terms:
        if not raw:
            continue
        negated = False
        if raw.startswith("-") and len(raw) > 1:
            negated, raw = True, raw[1:]
        field = None
        match = _SCOPE_RE.match(raw)
        if match:
            candidate = _FIELD_ALIASES.get(match.group("field").lower())
            value = match.group("value")
            if candidate:
                # ``lang:`` with nothing after it is a query the user is still
                # typing.  Drop the term so results stay visible instead of
                # collapsing to nothing between keystrokes.
                if not value:
                    continue
                field, raw = candidate, value
        if not raw:
            continue
        terms.append(SearchTerm(raw, field=field, negated=negated))
    return terms


def _term_sql(term: SearchTerm) -> Tuple[str, List[object]]:
    """Translate one term into a SQL predicate plus its parameters."""
    if term.field == "id":
        digits = term.value.strip()
        if digits.isdigit():
            clause, params = "s.id = ?", [int(digits)]
        else:  # not a number -> match nothing rather than erroring out
            clause, params = "0 = 1", []
        return ("NOT ({})".format(clause) if term.negated else clause), params

    pattern = "%{}%".format(_like_escape(term.value))
    tag_exists = (
        "EXISTS (SELECT 1 FROM snippet_tags st JOIN tags t ON t.id = st.tag_id "
        "WHERE st.snippet_id = s.id AND t.name LIKE ? ESCAPE '\\')"
    )

    if term.field == "tag":
        clause, params = tag_exists, [pattern]
    elif term.field in ("title", "language", "description", "code"):
        clause = "s.{} LIKE ? ESCAPE '\\'".format(term.field)
        params = [pattern]
    else:
        clause = (
            "(s.title LIKE ? ESCAPE '\\' OR s.language LIKE ? ESCAPE '\\' "
            "OR s.description LIKE ? ESCAPE '\\' OR s.code LIKE ? ESCAPE '\\' "
            "OR {})".format(tag_exists)
        )
        params = [pattern] * 5

    return ("NOT ({})".format(clause) if term.negated else clause), params


class SnippetStore:
    """A snippet library backed by a SQLite file.

    The connection stays open for the lifetime of the object; writes are wrapped
    in :meth:`_transaction` so a failure part-way through a tag update cannot
    leave a snippet with half its tags.
    """

    def __init__(self, path: Path | str, *, create_parents: bool = True) -> None:
        self.path = Path(path)
        if create_parents and str(self.path) != ":memory:" and self.path.parent:
            try:
                self.path.parent.mkdir(parents=True, exist_ok=True)
            except OSError as exc:
                raise StorageError(
                    "Cannot create data directory {}: {}".format(self.path.parent, exc)
                ) from exc
        try:
            self._conn = sqlite3.connect(str(self.path))
        except sqlite3.Error as exc:
            raise StorageError("Cannot open database {}: {}".format(self.path, exc)) from exc
        self._conn.row_factory = sqlite3.Row
        try:
            with closing(self._conn.cursor()) as cur:
                cur.execute("PRAGMA foreign_keys = ON")
                cur.execute("PRAGMA journal_mode = WAL")
        except sqlite3.Error:
            # WAL needs a writable directory; a read-only database can still be
            # browsed, so this is not worth failing over.
            pass
        try:
            self._create_schema()
        except sqlite3.Error as exc:
            # Most often a read-only file or directory.  Turn it into the
            # actionable message the CLI and UI know how to display, rather
            # than letting a raw sqlite3 error escape as a traceback.
            self.close()
            raise StorageError(
                "Cannot initialise database {}: {}".format(self.path, exc)
            ) from exc

    # -- lifecycle -------------------------------------------------------
    def close(self) -> None:
        try:
            self._conn.close()
        except sqlite3.Error:  # pragma: no cover - nothing useful to do
            pass

    def __enter__(self) -> "SnippetStore":
        return self

    def __exit__(self, *exc_info: object) -> None:
        self.close()

    @contextmanager
    def _transaction(self) -> Iterator[sqlite3.Cursor]:
        cur = self._conn.cursor()
        try:
            yield cur
        except Exception:
            self._conn.rollback()
            raise
        else:
            self._conn.commit()
        finally:
            cur.close()

    def _schema_present(self) -> bool:
        """True when the expected tables already exist (read-only friendly)."""
        try:
            with closing(self._conn.cursor()) as cur:
                found = {
                    row[0]
                    for row in cur.execute(
                        "SELECT name FROM sqlite_master WHERE type = 'table'"
                    )
                }
        except sqlite3.Error:
            return False
        return {"snippets", "tags", "snippet_tags"}.issubset(found)

    def _create_schema(self) -> None:
        if self._schema_present():
            # Nothing to do -- and importantly, no writes attempted, so an
            # existing library on a read-only mount can still be browsed.
            return
        with self._transaction() as cur:
            cur.executescript(
                """
                CREATE TABLE IF NOT EXISTS snippets (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    title       TEXT NOT NULL,
                    language    TEXT NOT NULL DEFAULT '',
                    description TEXT NOT NULL DEFAULT '',
                    code        TEXT NOT NULL DEFAULT '',
                    created_at  TEXT NOT NULL,
                    updated_at  TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS tags (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    name        TEXT NOT NULL,
                    name_folded TEXT NOT NULL UNIQUE
                );

                CREATE TABLE IF NOT EXISTS snippet_tags (
                    snippet_id INTEGER NOT NULL
                        REFERENCES snippets(id) ON DELETE CASCADE,
                    tag_id     INTEGER NOT NULL
                        REFERENCES tags(id) ON DELETE CASCADE,
                    PRIMARY KEY (snippet_id, tag_id)
                );

                CREATE TABLE IF NOT EXISTS meta (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_snippets_title
                    ON snippets(title COLLATE NOCASE);
                CREATE INDEX IF NOT EXISTS idx_snippets_language
                    ON snippets(language COLLATE NOCASE);
                CREATE INDEX IF NOT EXISTS idx_snippet_tags_tag
                    ON snippet_tags(tag_id);
                """
            )
            cur.execute(
                "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', ?)",
                (str(SCHEMA_VERSION),),
            )

    # -- reading ---------------------------------------------------------
    def _row_to_snippet(self, row: sqlite3.Row, tags: Sequence[str]) -> Snippet:
        return Snippet(
            id=row["id"],
            title=row["title"],
            language=row["language"],
            description=row["description"],
            code=row["code"],
            tags=list(tags),
            created_at=row["created_at"],
            updated_at=row["updated_at"],
        )

    def _tags_for(self, snippet_ids: Sequence[int]) -> dict:
        """Fetch tags for many snippets at once (avoids a query per row)."""
        if not snippet_ids:
            return {}
        placeholders = ",".join("?" for _ in snippet_ids)
        sql = (
            "SELECT st.snippet_id AS sid, t.name AS name "
            "FROM snippet_tags st JOIN tags t ON t.id = st.tag_id "
            "WHERE st.snippet_id IN ({}) "
            "ORDER BY t.name COLLATE NOCASE".format(placeholders)
        )
        mapping: dict = {sid: [] for sid in snippet_ids}
        with closing(self._conn.cursor()) as cur:
            for row in cur.execute(sql, tuple(snippet_ids)):
                mapping.setdefault(row["sid"], []).append(row["name"])
        return mapping

    def list_snippets(
        self,
        query: Optional[str] = None,
        *,
        sort: str = "updated",
        limit: Optional[int] = None,
    ) -> List[Snippet]:
        """Return snippets matching ``query`` (empty query -> everything)."""
        order_by = next(
            (clause for key, _label, clause in SORT_MODES if key == sort),
            SORT_MODES[0][2],
        )
        where_parts: List[str] = []
        params: List[object] = []
        for term in parse_query(query):
            clause, term_params = _term_sql(term)
            where_parts.append(clause)
            params.extend(term_params)

        sql = "SELECT s.* FROM snippets s"
        if where_parts:
            sql += " WHERE " + " AND ".join(where_parts)
        sql += " ORDER BY " + order_by
        if limit is not None:
            sql += " LIMIT ?"
            params.append(int(limit))

        with closing(self._conn.cursor()) as cur:
            rows = cur.execute(sql, tuple(params)).fetchall()
        tag_map = self._tags_for([row["id"] for row in rows])
        return [self._row_to_snippet(row, tag_map.get(row["id"], [])) for row in rows]

    def get(self, snippet_id: int) -> Optional[Snippet]:
        with closing(self._conn.cursor()) as cur:
            row = cur.execute(
                "SELECT * FROM snippets WHERE id = ?", (int(snippet_id),)
            ).fetchone()
        if row is None:
            return None
        return self._row_to_snippet(row, self._tags_for([row["id"]]).get(row["id"], []))

    def count(self) -> int:
        with closing(self._conn.cursor()) as cur:
            return int(cur.execute("SELECT COUNT(*) FROM snippets").fetchone()[0])

    def languages(self) -> List[str]:
        """Distinct non-empty languages, for suggestions."""
        with closing(self._conn.cursor()) as cur:
            rows = cur.execute(
                "SELECT DISTINCT language FROM snippets WHERE language <> '' "
                "ORDER BY language COLLATE NOCASE"
            ).fetchall()
        return [row["language"] for row in rows]

    def all_tags(self) -> List[Tuple[str, int]]:
        """Every tag in use with how many snippets carry it."""
        with closing(self._conn.cursor()) as cur:
            rows = cur.execute(
                "SELECT t.name AS name, COUNT(st.snippet_id) AS n "
                "FROM tags t LEFT JOIN snippet_tags st ON st.tag_id = t.id "
                "GROUP BY t.id HAVING n > 0 ORDER BY t.name COLLATE NOCASE"
            ).fetchall()
        return [(row["name"], int(row["n"])) for row in rows]

    # -- writing ---------------------------------------------------------
    def _validate_title(self, title: str) -> str:
        cleaned = (title or "").strip()
        if not cleaned:
            raise StorageError("Title is required.")
        return cleaned

    def _sync_tags(self, cur: sqlite3.Cursor, snippet_id: int, tags: Iterable[str]) -> None:
        """Make ``snippet_tags`` for ``snippet_id`` exactly ``tags``."""
        wanted = normalize_tags(tags)
        cur.execute("DELETE FROM snippet_tags WHERE snippet_id = ?", (snippet_id,))
        for tag in wanted:
            folded = tag.casefold()
            cur.execute(
                "INSERT INTO tags(name, name_folded) VALUES (?, ?) "
                "ON CONFLICT(name_folded) DO NOTHING",
                (tag, folded),
            )
            row = cur.execute(
                "SELECT id FROM tags WHERE name_folded = ?", (folded,)
            ).fetchone()
            if row is None:  # pragma: no cover - defensive
                continue
            cur.execute(
                "INSERT OR IGNORE INTO snippet_tags(snippet_id, tag_id) VALUES (?, ?)",
                (snippet_id, row["id"]),
            )
        self._prune_tags(cur)

    def _prune_tags(self, cur: sqlite3.Cursor) -> None:
        """Drop tag rows nothing references any more."""
        cur.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM snippet_tags)"
        )

    def create(self, snippet: Snippet) -> Snippet:
        """Insert ``snippet`` and return it with ``id``/timestamps filled in."""
        title = self._validate_title(snippet.title)
        now = _utc_now()
        try:
            with self._transaction() as cur:
                cur.execute(
                    "INSERT INTO snippets(title, language, description, code, "
                    "created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
                    (
                        title,
                        normalize_language(snippet.language),
                        snippet.description or "",
                        snippet.code or "",
                        now,
                        now,
                    ),
                )
                new_id = int(cur.lastrowid)
                self._sync_tags(cur, new_id, snippet.tags)
        except sqlite3.Error as exc:
            raise StorageError("Could not save snippet: {}".format(exc)) from exc
        stored = self.get(new_id)
        if stored is None:  # pragma: no cover - defensive
            raise StorageError("Snippet vanished immediately after insert.")
        return stored

    def update(self, snippet: Snippet) -> Snippet:
        """Persist changes to an existing snippet (identified by ``id``)."""
        if snippet.id is None:
            raise StorageError("Cannot update a snippet that was never saved.")
        title = self._validate_title(snippet.title)
        now = _utc_now()
        try:
            with self._transaction() as cur:
                cur.execute(
                    "UPDATE snippets SET title = ?, language = ?, description = ?, "
                    "code = ?, updated_at = ? WHERE id = ?",
                    (
                        title,
                        normalize_language(snippet.language),
                        snippet.description or "",
                        snippet.code or "",
                        now,
                        int(snippet.id),
                    ),
                )
                if cur.rowcount == 0:
                    raise StorageError(
                        "Snippet #{} no longer exists.".format(snippet.id)
                    )
                self._sync_tags(cur, int(snippet.id), snippet.tags)
        except sqlite3.Error as exc:
            raise StorageError("Could not update snippet: {}".format(exc)) from exc
        stored = self.get(int(snippet.id))
        if stored is None:  # pragma: no cover - defensive
            raise StorageError("Snippet #{} disappeared.".format(snippet.id))
        return stored

    def rename(self, snippet_id: int, title: str) -> Snippet:
        """Change only the title -- the dedicated 'rename' operation."""
        current = self.get(snippet_id)
        if current is None:
            raise StorageError("Snippet #{} no longer exists.".format(snippet_id))
        return self.update(current.copy(title=self._validate_title(title)))

    def delete(self, snippet_id: int) -> bool:
        """Remove a snippet and its tag links.  ``True`` if a row was deleted."""
        try:
            with self._transaction() as cur:
                cur.execute("DELETE FROM snippets WHERE id = ?", (int(snippet_id),))
                deleted = cur.rowcount > 0
                # Rely on the explicit delete as well as the cascade, so the
                # link table is clean even if foreign keys were unavailable.
                cur.execute(
                    "DELETE FROM snippet_tags WHERE snippet_id = ?", (int(snippet_id),)
                )
                self._prune_tags(cur)
        except sqlite3.Error as exc:
            raise StorageError("Could not delete snippet: {}".format(exc)) from exc
        return deleted

    def duplicate(self, snippet_id: int) -> Snippet:
        """Copy a snippet under a free ``... (copy)`` title."""
        current = self.get(snippet_id)
        if current is None:
            raise StorageError("Snippet #{} no longer exists.".format(snippet_id))
        return self.create(current.copy(id=None, title=self._free_title(current.title)))

    def _free_title(self, title: str) -> str:
        base = "{} (copy)".format(title)
        candidate, n = base, 2
        with closing(self._conn.cursor()) as cur:
            while cur.execute(
                "SELECT 1 FROM snippets WHERE title = ? COLLATE NOCASE", (candidate,)
            ).fetchone():
                candidate = "{} ({})".format(base, n)
                n += 1
        return candidate
