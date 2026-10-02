"""Storage layer: CRUD, search and real on-disk persistence."""

from __future__ import annotations

import sqlite3
from pathlib import Path

import pytest

from toolh.models import Snippet, parse_tags
from toolh.storage import SnippetStore, StorageError, parse_query


def test_database_file_is_actually_created(db_path: Path, store: SnippetStore):
    assert db_path.exists()
    assert store.count() == 0


def test_create_round_trips_every_field(store: SnippetStore):
    created = store.create(
        Snippet(
            title="hello-world",
            language="Python",
            description="Print a greeting message",
            code='print("hello")',
            tags=["demo", "Demo", "  cli  "],
        )
    )
    assert created.id is not None
    assert created.title == "hello-world"
    assert created.language == "python"  # normalised
    assert created.description == "Print a greeting message"
    assert created.code == 'print("hello")'
    assert created.tags == ["cli", "demo"]  # de-duplicated, sorted by storage
    assert created.created_at and created.updated_at


def test_data_survives_reopening_the_database(db_path: Path):
    store = SnippetStore(db_path)
    store.create(Snippet(title="persisted", code="x = 1", tags=["keep"]))
    store.close()

    reopened = SnippetStore(db_path)
    try:
        snippets = reopened.list_snippets()
        assert [s.title for s in snippets] == ["persisted"]
        assert snippets[0].code == "x = 1"
        assert snippets[0].tags == ["keep"]
    finally:
        reopened.close()


def test_rows_are_visible_to_an_independent_sqlite_connection(db_path: Path, store):
    """Persistence must be real SQLite, not a pickle with a .db name."""
    store.create(Snippet(title="visible", language="sql", code="SELECT 1"))
    with sqlite3.connect(str(db_path)) as conn:
        rows = conn.execute("SELECT title, language, code FROM snippets").fetchall()
    assert rows == [("visible", "sql", "SELECT 1")]


def test_update_changes_fields_and_tags(store: SnippetStore):
    snippet = store.create(
        Snippet(title="a", language="python", code="1", tags=["old"])
    )
    updated = store.update(
        snippet.copy(title="b", language="bash", code="2", tags=["new", "extra"])
    )
    assert updated.id == snippet.id
    assert (updated.title, updated.language, updated.code) == ("b", "bash", "2")
    assert updated.tags == ["extra", "new"]
    # The old tag is gone from the snippet and pruned from the tag table.
    assert "old" not in dict(store.all_tags())


def test_rename_only_touches_the_title(store: SnippetStore):
    snippet = store.create(
        Snippet(
            title="before",
            language="python",
            description="desc",
            code="code",
            tags=["t"],
        )
    )
    renamed = store.rename(snippet.id, "after")
    assert renamed.title == "after"
    assert renamed.language == "python"
    assert renamed.description == "desc"
    assert renamed.code == "code"
    assert renamed.tags == ["t"]


def test_delete_removes_from_list_and_storage(store: SnippetStore, db_path: Path):
    snippet = store.create(Snippet(title="doomed", tags=["gone"]))
    assert store.delete(snippet.id) is True

    assert store.get(snippet.id) is None
    assert [s.title for s in store.list_snippets()] == []
    with sqlite3.connect(str(db_path)) as conn:
        assert conn.execute("SELECT COUNT(*) FROM snippets").fetchone()[0] == 0
        # tag links must not survive their snippet
        assert conn.execute("SELECT COUNT(*) FROM snippet_tags").fetchone()[0] == 0
    assert store.delete(snippet.id) is False  # deleting twice is not an error


def test_empty_title_is_rejected(store: SnippetStore):
    with pytest.raises(StorageError):
        store.create(Snippet(title="   "))


def test_update_of_missing_row_raises(store: SnippetStore):
    with pytest.raises(StorageError):
        store.update(Snippet(title="ghost", id=4242))


def test_duplicate_picks_a_free_title(store: SnippetStore):
    original = store.create(Snippet(title="base", code="x", tags=["t"]))
    first = store.duplicate(original.id)
    second = store.duplicate(original.id)
    assert first.title == "base (copy)"
    assert second.title == "base (copy) (2)"
    assert first.code == "x" and first.tags == ["t"]


# -- search ---------------------------------------------------------------
@pytest.mark.parametrize(
    "query, expected",
    [
        ("", ["git undo", "hello-world", "sqlite top rows"]),
        ("greeting", ["hello-world"]),
        ("GREETING", ["hello-world"]),  # case-insensitive
        ("tag:cli", ["git undo", "sqlite top rows"]),
        ("lang:python", ["hello-world"]),
        ("title:git", ["git undo"]),
        ("desc:newest", ["sqlite top rows"]),
        ("code:SELECT", ["sqlite top rows"]),
        ("-tag:cli", ["hello-world"]),
        ("cli -lang:sql", ["git undo"]),
        ('"last commit"', ["git undo"]),
        ("nothing-matches-this", []),
    ],
)
def test_search(store: SnippetStore, sample_snippets, query, expected):
    found = sorted(s.title for s in store.list_snippets(query))
    assert found == sorted(expected)


def test_search_terms_are_anded(store: SnippetStore, sample_snippets):
    assert [s.title for s in store.list_snippets("git commit")] == ["git undo"]
    assert store.list_snippets("git python") == []


def test_search_by_id(store: SnippetStore, sample_snippets):
    target = sample_snippets[1]
    assert [s.id for s in store.list_snippets("id:{}".format(target.id))] == [target.id]
    assert store.list_snippets("id:not-a-number") == []


def test_like_wildcards_are_matched_literally(store: SnippetStore):
    store.create(Snippet(title="100% coverage", code="a"))
    store.create(Snippet(title="plain", code="b"))
    assert [s.title for s in store.list_snippets("100%")] == ["100% coverage"]
    # A bare % must not behave as "match everything".
    assert [s.title for s in store.list_snippets("%")] == ["100% coverage"]
    assert [s.title for s in store.list_snippets("_")] == []


def test_half_typed_scope_is_ignored(store: SnippetStore, sample_snippets):
    """While typing 'lang:' the list should not blank out."""
    assert len(store.list_snippets("lang:")) == 3


def test_unbalanced_quote_does_not_explode(store: SnippetStore, sample_snippets):
    assert [s.title for s in store.list_snippets('"git')] == ["git undo"]


def test_sort_modes(store: SnippetStore, sample_snippets):
    titles = [s.title for s in store.list_snippets(sort="title")]
    assert titles == sorted(titles, key=str.lower)
    languages = [s.language for s in store.list_snippets(sort="language")]
    assert languages == sorted(languages)
    # Unknown sort keys fall back rather than raising.
    assert len(store.list_snippets(sort="nonsense")) == 3


def test_tags_and_languages_inventories(store: SnippetStore, sample_snippets):
    assert dict(store.all_tags()) == {"cli": 2, "db": 1, "demo": 1, "git": 1}
    assert store.languages() == ["bash", "python", "sql"]


def test_multiline_and_unicode_content(store: SnippetStore):
    code = 'def f():\n    return "English-only text ✓"\n'
    snippet = store.create(
        Snippet(title="unicode ✓", language="python", code=code, tags=["ünïcode"])
    )
    fetched = store.get(snippet.id)
    assert fetched.code == code
    assert fetched.line_count == 2
    assert fetched.tags == ["ünïcode"]
    assert [s.title for s in store.list_snippets("English-only text")] == ["unicode ✓"]


def test_parse_query_shapes():
    assert parse_query("") == []
    terms = parse_query('word tag:x -lang:py "two words"')
    assert (terms[0].value, terms[0].field, terms[0].negated) == ("word", None, False)
    assert (terms[1].value, terms[1].field) == ("x", "tag")
    assert (terms[2].value, terms[2].field, terms[2].negated) == ("py", "language", True)
    assert terms[3].value == "two words"


def test_parse_tags_helper():
    assert parse_tags("a, b  c,,  a") == ["a", "b", "c"]
    assert parse_tags("") == []
    assert parse_tags(None) == []
