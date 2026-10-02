import pytest

from toolh.app import SnippetForm, ToolhApp
from toolh.storage import SnippetStore


@pytest.mark.asyncio
async def test_app_create_search_edit_delete(tmp_path):
    store = SnippetStore(tmp_path / "ui.sqlite3")
    app = ToolhApp(store)

    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.press("n")
        await pilot.pause()
        assert isinstance(app.screen, SnippetForm)

        form = app.screen
        form.query_one("#title-input").value = "hello-world"
        form.query_one("#language-input").value = "python"
        form.query_one("#tags-input").value = "demo"
        form.query_one("#description-input").load_text("Print a greeting")
        form.query_one("#code-input").load_text('print("hello")')
        await pilot.press("ctrl+s")
        await pilot.pause()

        assert len(store.list()) == 1
        assert app.query_one("#detail-title").render().plain == "hello-world"
        assert app.query_one("#detail-code").text == 'print("hello")'

        await pilot.press("/")
        await pilot.press("h", "e", "l", "l", "o")
        await pilot.pause()
        assert len(app.snippets) == 1
        await pilot.press("ctrl+a", "x", "m", "i", "s", "s")
        await pilot.pause()
        assert app.snippets == []

        await pilot.press("escape")
        await pilot.press("e")
        await pilot.pause()
        form = app.screen
        form.query_one("#title-input").value = "renamed"
        await pilot.press("ctrl+s")
        await pilot.pause()
        assert store.list()[0].title == "renamed"

        await pilot.press("d")
        await pilot.press("y")
        await pilot.pause()
        assert store.list() == []

