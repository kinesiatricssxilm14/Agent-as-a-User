import sqlite3

from toolh.storage import SnippetStore, default_database_path


def test_crud_and_search(tmp_path):
    path = tmp_path / "nested" / "toolh.sqlite3"
    store = SnippetStore(path)

    created = store.create(
        title="hello-world",
        language="python",
        tags="demo, greeting",
        description="Print a greeting",
        code='print("hello")',
    )
    assert path.exists()
    assert store.get(created.id) == created
    assert [item.id for item in store.list("GREET")] == [created.id]
    assert [item.id for item in store.list('print("hello")')] == [created.id]
    assert store.list("does-not-exist") == []

    updated = store.update(
        created.id,
        title="renamed",
        language="python",
        tags="sample",
        description="Changed",
        code="print(42)",
    )
    assert updated is not None
    assert updated.title == "renamed"
    assert updated.code == "print(42)"

    assert store.delete(created.id)
    assert store.get(created.id) is None
    assert store.list() == []
    assert not store.delete(created.id)

    with sqlite3.connect(path) as connection:
        assert connection.execute("SELECT COUNT(*) FROM snippets").fetchone()[0] == 0


def test_search_treats_sql_wildcards_literally(tmp_path):
    store = SnippetStore(tmp_path / "db.sqlite3")
    percent = store.create("100% useful", "", "", "", "x")
    underscore = store.create("under_score", "", "", "", "y")
    store.create("ordinary", "", "", "", "z")

    assert [item.id for item in store.list("%")] == [percent.id]
    assert [item.id for item in store.list("_")] == [underscore.id]


def test_environment_path_precedence(monkeypatch, tmp_path):
    explicit = tmp_path / "exact.sqlite"
    monkeypatch.setenv("TOOLH_DB_PATH", str(explicit))
    monkeypatch.setenv("TOOLH_DATA_DIR", str(tmp_path / "ignored"))
    assert default_database_path() == explicit

    monkeypatch.delenv("TOOLH_DB_PATH")
    assert default_database_path() == tmp_path / "ignored" / "snippets.db"

