"""Awkward situations: read-only storage, tiny terminals, hostile content."""

from __future__ import annotations

import os
import stat
from pathlib import Path

import pytest
from textual.widgets import DataTable, Input, TextArea

from toolh.app import ToolhApp
from toolh.clipboard import Clipboard
from toolh.config import Paths
from toolh.models import Snippet
from toolh.storage import SnippetStore, StorageError

from .conftest import detail_text


@pytest.fixture()
def readonly_store(tmp_path: Path):
    """An existing library whose file has been made read-only."""
    directory = tmp_path / "ro"
    directory.mkdir()
    db = directory / "snippets.db"
    seed = SnippetStore(db)
    seed.create(Snippet(title="existing", language="python", code="print(1)"))
    seed.close()

    # Drop write permission on both the file and its directory.
    os.chmod(db, stat.S_IRUSR)
    os.chmod(directory, stat.S_IRUSR | stat.S_IXUSR)
    try:
        yield db
    finally:
        os.chmod(directory, 0o755)
        os.chmod(db, 0o644)


def test_readonly_library_is_still_browsable(readonly_store: Path):
    store = SnippetStore(readonly_store)
    try:
        assert [s.title for s in store.list_snippets()] == ["existing"]
    finally:
        store.close()


def test_writing_to_readonly_storage_raises_storage_error(readonly_store: Path):
    """A read-only database must produce an actionable error, not a traceback."""
    store = SnippetStore(readonly_store)
    try:
        with pytest.raises(StorageError):
            store.create(Snippet(title="nope"))
    finally:
        store.close()


async def test_ui_reports_failure_to_save_without_crashing(
    readonly_store: Path, tmp_path: Path
):
    store = SnippetStore(readonly_store)
    paths = Paths(database=readonly_store, config=tmp_path / "c.json")
    app = ToolhApp(store, paths, clipboard=Clipboard(command="true", allow_osc52=False))
    try:
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("ctrl+n")
            await pilot.pause()
            for character in "doomed":
                await pilot.press(character)
            await pilot.press("ctrl+s")
            await pilot.pause()
            # Still alive, still in the editor, nothing silently "saved".
            assert app.is_running
            assert app.editing is True
            assert [s.title for s in store.list_snippets()] == ["existing"]
    finally:
        store.close()


async def test_small_terminal_still_works(store: SnippetStore, paths, clipboard):
    """A cramped terminal must not break the layout or the key bindings."""
    store.create(Snippet(title="one", language="python", code="print(1)", tags=["a"]))
    store.create(Snippet(title="two", language="bash", code="echo hi", tags=["b"]))
    app = ToolhApp(store, paths, clipboard=clipboard)
    async with app.run_test(size=(40, 12)) as pilot:
        await pilot.pause()
        assert app.query_one("#snippet-table", DataTable).row_count == 2
        await pilot.press("ctrl+n")
        await pilot.pause()
        assert app.editing is True
        await pilot.press("escape")
        await pilot.pause()
        await pilot.press("question_mark")
        await pilot.pause()
        await pilot.press("escape")
        await pilot.pause()
        await pilot.press("c")
        await pilot.pause()
        assert app.is_running


async def test_rich_markup_in_content_is_not_interpreted(
    store: SnippetStore, paths, clipboard
):
    """Square brackets in user content must not be parsed as Rich markup."""
    store.create(
        Snippet(
            title="[bold]literal[/bold]",
            language="python",
            description="brackets [here] and {braces}",
            code="a = [1, 2, 3]\nprint(f'{a[0]}')",
            tags=["[tag]"],
        )
    )
    app = ToolhApp(store, paths, clipboard=clipboard)
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        assert app.is_running

        text = detail_text(app)
        assert "[bold]literal[/bold]" in text
        assert "brackets [here]" in text


async def test_large_snippet_is_handled(store: SnippetStore, paths, clipboard, clipboard_sink: Path):
    body = "\n".join("line {}".format(i) for i in range(1000))
    store.create(Snippet(title="huge", language="python", code=body))
    app = ToolhApp(store, paths, clipboard=clipboard)
    async with app.run_test(size=(100, 30)) as pilot:
        await pilot.pause()
        await pilot.press("c")
        await pilot.pause()
        assert clipboard_sink.read_text(encoding="utf-8") == body
        assert app.is_running


async def test_unicode_content_through_the_ui(
    store: SnippetStore, paths, clipboard, clipboard_sink: Path
):
    app = ToolhApp(store, paths, clipboard=clipboard)
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("ctrl+n")
        await pilot.pause()
        title = app.query_one("#f-title", Input)
        title.value = "unicode ✓ English-only text"
        app.query_one("#f-code", TextArea).text = 'print("English-only text ✓")'
        await pilot.press("ctrl+s")
        await pilot.pause()
        stored = store.list_snippets()[0]
        assert stored.title == "unicode ✓ English-only text"
        assert stored.code == 'print("English-only text ✓")'
        await pilot.press("c")
        await pilot.pause()
        assert clipboard_sink.read_text(encoding="utf-8") == 'print("English-only text ✓")'


def test_unknown_language_does_not_break_storage(store: SnippetStore):
    snippet = store.create(
        Snippet(title="odd", language="not-a-real-language", code="???")
    )
    assert store.get(snippet.id).language == "not-a-real-language"


async def test_unknown_language_does_not_break_the_editor(
    store: SnippetStore, paths, clipboard
):
    store.create(Snippet(title="odd", language="brainfuck-9000", code="+++"))
    app = ToolhApp(store, paths, clipboard=clipboard)
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("ctrl+e")
        await pilot.pause()
        assert app.editing is True
        # No highlighter exists for it, which must be a no-op rather than a crash.
        assert app.query_one("#f-code", TextArea).language is None
        assert app.is_running
