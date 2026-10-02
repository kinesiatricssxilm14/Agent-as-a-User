"""End-to-end interface tests: every action is driven by real key presses.

These tests are the ones that matter most for the promises toolh makes -- that
the whole tool is usable from the keyboard, that the detail view shows all five
fields at once, and that what the UI shows is what the database contains.
"""

from __future__ import annotations

import sqlite3
from pathlib import Path

import pytest
from textual.widgets import DataTable, Input, TextArea

from toolh.app import HelpScreen, ToolhApp
from toolh.models import Snippet
from toolh.storage import SnippetStore

from .conftest import detail_text, type_text


@pytest.fixture()
def app(store: SnippetStore, paths, clipboard):
    return ToolhApp(store, paths, clipboard=clipboard)


# -- creation -------------------------------------------------------------
async def test_create_snippet_with_keyboard_only(app: ToolhApp, store: SnippetStore):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("ctrl+n")
        await pilot.pause()
        assert app.editing is True

        await type_text(pilot, "hello-world")
        await pilot.press("tab")
        await type_text(pilot, "python")
        await pilot.press("tab")
        await type_text(pilot, "demo")
        await pilot.press("tab")
        await type_text(pilot, "Print a greeting message")
        await pilot.press("tab")
        await type_text(pilot, 'print("hello")')
        await pilot.press("ctrl+s")
        await pilot.pause()

        assert app.editing is False
        stored = store.list_snippets()
        assert len(stored) == 1
        snippet = stored[0]
        assert snippet.title == "hello-world"
        assert snippet.language == "python"
        assert snippet.tags == ["demo"]
        assert snippet.description == "Print a greeting message"
        assert snippet.code == 'print("hello")'


async def test_created_snippet_is_written_to_the_real_database(
    app: ToolhApp, db_path: Path
):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("ctrl+n")
        await pilot.pause()
        await type_text(pilot, "from-ui")
        await pilot.press("ctrl+s")
        await pilot.pause()

    with sqlite3.connect(str(db_path)) as conn:
        titles = [row[0] for row in conn.execute("SELECT title FROM snippets")]
    assert titles == ["from-ui"]


async def test_save_without_title_is_refused(app: ToolhApp, store: SnippetStore):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("ctrl+n")
        await pilot.pause()
        await pilot.press("tab")
        await type_text(pilot, "python")
        await pilot.press("ctrl+s")
        await pilot.pause()
        assert app.editing is True  # still in the editor
        assert store.count() == 0


async def test_escape_cancels_creation_without_writing(
    app: ToolhApp, store: SnippetStore
):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("ctrl+n")
        await pilot.pause()
        await type_text(pilot, "abandoned")
        await pilot.press("escape")
        await pilot.pause()
        assert app.editing is False
        assert store.count() == 0


# -- detail view ----------------------------------------------------------
async def test_detail_view_shows_all_five_fields_at_once(
    app: ToolhApp, sample_snippets
):
    """title, language, tags, description and code on the same screen."""
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("slash")
        await type_text(pilot, "hello")
        await pilot.pause()
        await pilot.press("escape")
        await pilot.pause()

        # Select the hello-world snippet explicitly.
        target = next(s for s in app.snippets if s.title == "hello-world")
        table = app.query_one("#snippet-table", DataTable)
        table.move_cursor(row=app.snippets.index(target))
        await pilot.pause()

        text = detail_text(app)
        assert "hello-world" in text                 # title
        assert "python" in text                      # language
        assert "demo" in text                        # tags
        assert "Print a greeting message" in text    # description
        assert 'print("hello")' in text              # code

        # All four detail widgets are visible simultaneously -- no tabs/modals.
        for widget_id in (
            "#detail-title",
            "#detail-meta",
            "#detail-description",
            "#detail-code",
        ):
            assert app.query_one(widget_id).display is True
        assert app.query_one("#detail-view").display is True


async def test_detail_updates_when_moving_the_cursor(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        first = detail_text(app)
        await pilot.press("down")
        await pilot.pause()
        second = detail_text(app)
        assert first != second


async def test_list_and_detail_are_visible_together(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        assert app.query_one("#snippet-table", DataTable).row_count == 3
        assert app.query_one("#detail-view").display is True
        # Whichever row the cursor landed on, its detail is rendered alongside.
        selected = app.current_snippet()
        assert selected is not None
        assert selected.title in detail_text(app)


# -- editing --------------------------------------------------------------
async def test_edit_changes_content(app: ToolhApp, store: SnippetStore, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        target = app.snippets[0]
        await pilot.press("ctrl+e")
        await pilot.pause()
        assert app.editing is True

        code = app.query_one("#f-code", TextArea)
        code.text = "updated code body"
        description = app.query_one("#f-description", TextArea)
        description.text = "updated description"
        tags = app.query_one("#f-tags", Input)
        tags.value = "alpha, beta"
        await pilot.press("ctrl+s")
        await pilot.pause()

        stored = store.get(target.id)
        assert stored.code == "updated code body"
        assert stored.description == "updated description"
        assert stored.tags == ["alpha", "beta"]
        # And the UI now shows the new content.
        assert "updated code body" in detail_text(app)


async def test_editor_is_prefilled_with_every_field(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        target = app.snippets[0]
        await pilot.press("ctrl+e")
        await pilot.pause()
        assert app.query_one("#f-title", Input).value == target.title
        assert app.query_one("#f-language", Input).value == target.language
        assert app.query_one("#f-tags", Input).value == target.tag_text
        assert app.query_one("#f-description", TextArea).text == target.description
        assert app.query_one("#f-code", TextArea).text == target.code


async def test_edit_cancel_leaves_storage_untouched(
    app: ToolhApp, store: SnippetStore, sample_snippets
):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        target = app.snippets[0]
        before = store.get(target.id).to_dict()
        await pilot.press("ctrl+e")
        await pilot.pause()
        app.query_one("#f-code", TextArea).text = "throw this away"
        await pilot.press("escape")
        await pilot.pause()
        assert store.get(target.id).to_dict() == before


# -- rename ---------------------------------------------------------------
async def test_rename_via_inline_bar(app: ToolhApp, store: SnippetStore, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        target = app.snippets[0]
        original_code = target.code
        await pilot.press("r")
        await pilot.pause()
        assert app.query_one("#rename-bar").has_class("visible")
        # The snippet being renamed stays on screen while we type.
        assert app.query_one("#detail-view").display is True

        field = app.query_one("#f-rename", Input)
        field.value = ""
        await type_text(pilot, "renamed-title")
        await pilot.press("enter")
        await pilot.pause()

        stored = store.get(target.id)
        assert stored.title == "renamed-title"
        assert stored.code == original_code  # rename must not touch the body
        assert not app.query_one("#rename-bar").has_class("visible")


async def test_rename_cancel(app: ToolhApp, store: SnippetStore, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        target = app.snippets[0]
        await pilot.press("r")
        await pilot.pause()
        app.query_one("#f-rename", Input).value = "not-applied"
        await pilot.press("escape")
        await pilot.pause()
        assert store.get(target.id).title == target.title


async def test_rename_to_blank_is_refused(app: ToolhApp, store: SnippetStore, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        target = app.snippets[0]
        await pilot.press("r")
        await pilot.pause()
        app.query_one("#f-rename", Input).value = "   "
        await pilot.press("enter")
        await pilot.pause()
        assert store.get(target.id).title == target.title


# -- delete ---------------------------------------------------------------
async def test_delete_asks_first_then_removes(
    app: ToolhApp, store: SnippetStore, sample_snippets, db_path: Path
):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        target = app.snippets[0]
        await pilot.press("d")
        await pilot.pause()
        assert app.query_one("#confirm-bar").has_class("visible")
        # The snippet is still there until confirmation.
        assert store.get(target.id) is not None

        await pilot.press("y")
        await pilot.pause()

        assert store.get(target.id) is None
        assert target.id not in [s.id for s in app.snippets]
        assert app.query_one("#snippet-table", DataTable).row_count == 2

    with sqlite3.connect(str(db_path)) as conn:
        remaining = [row[0] for row in conn.execute("SELECT id FROM snippets")]
    assert target.id not in remaining


@pytest.mark.parametrize("cancel_key", ["n", "escape"])
async def test_delete_can_be_cancelled(
    app: ToolhApp, store: SnippetStore, sample_snippets, cancel_key
):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("d")
        await pilot.pause()
        await pilot.press(cancel_key)
        await pilot.pause()
        assert store.count() == 3
        assert not app.query_one("#confirm-bar").has_class("visible")


async def test_delete_key_alias(app: ToolhApp, store: SnippetStore, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("delete")
        await pilot.pause()
        await pilot.press("y")
        await pilot.pause()
        assert store.count() == 2


# -- search ---------------------------------------------------------------
async def test_search_filters_as_you_type(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()
        assert isinstance(app.focused, Input)

        await type_text(pilot, "git")
        await pilot.pause()
        assert [s.title for s in app.snippets] == ["git undo"]
        assert app.query_one("#snippet-table", DataTable).row_count == 1


async def test_search_by_tag_and_language(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("slash")
        await type_text(pilot, "tag:cli")
        await pilot.pause()
        assert sorted(s.title for s in app.snippets) == ["git undo", "sqlite top rows"]

        await pilot.press("escape")
        await pilot.pause()
        await pilot.press("slash")
        await type_text(pilot, "lang:sql")
        await pilot.pause()
        assert [s.title for s in app.snippets] == ["sqlite top rows"]


async def test_escape_clears_the_search(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("slash")
        await type_text(pilot, "git")
        await pilot.pause()
        await pilot.press("escape")
        await pilot.pause()
        assert app.query_one("#search", Input).value == ""
        assert len(app.snippets) == 3


async def test_no_match_explains_itself(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("slash")
        await type_text(pilot, "zzz-no-such-thing")
        await pilot.pause()
        assert app.snippets == []
        assert "No snippet matches" in detail_text(app)


# -- clipboard ------------------------------------------------------------
async def test_copy_code_reaches_a_real_backend(
    app: ToolhApp, sample_snippets, clipboard_sink: Path
):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        target = app.snippets[0]
        await pilot.press("c")
        await pilot.pause()
        assert clipboard_sink.read_text(encoding="utf-8") == target.code


async def test_copy_all_includes_every_field(
    app: ToolhApp, sample_snippets, clipboard_sink: Path
):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        target = app.snippets[0]
        await pilot.press("y")
        await pilot.pause()
        payload = clipboard_sink.read_text(encoding="utf-8")
        assert target.title in payload
        assert target.language in payload
        assert target.description in payload
        assert target.code in payload
        for tag in target.tags:
            assert tag in payload


# -- navigation, sort, help ----------------------------------------------
async def test_vim_and_arrow_navigation(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        first = app.selected_id
        await pilot.press("j")
        await pilot.pause()
        assert app.selected_id != first
        await pilot.press("k")
        await pilot.pause()
        assert app.selected_id == first
        await pilot.press("G")
        await pilot.pause()
        assert app.selected_id == app.snippets[-1].id
        await pilot.press("g")
        await pilot.pause()
        assert app.selected_id == app.snippets[0].id


async def test_sort_cycles(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        labels = []
        for _ in range(5):
            labels.append(app.sort_label)
            await pilot.press("s")
            await pilot.pause()
        assert len(set(labels)) > 1
        assert labels[0] == labels[4]  # four modes, cycles back around


async def test_help_screen_lists_the_keys(app: ToolhApp, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("question_mark")
        await pilot.pause()
        assert isinstance(app.screen, HelpScreen)
        await pilot.press("escape")
        await pilot.pause()
        assert not isinstance(app.screen, HelpScreen)


async def test_keys_do_not_leak_through_the_help_screen(
    app: ToolhApp, store: SnippetStore, sample_snippets
):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("question_mark")
        await pilot.pause()
        await pilot.press("d")  # must not start a deletion behind the help
        await pilot.pause()
        assert not app.query_one("#confirm-bar").has_class("visible")
        assert store.count() == 3


async def test_footer_documents_the_shortcuts(app: ToolhApp, sample_snippets):
    """Shortcuts must be discoverable without external documentation."""
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        shown = {
            binding.key: binding.description
            for (_, binding, _enabled, _tooltip) in app.screen.active_bindings.values()
        }
        for key in ("ctrl+n", "ctrl+e", "r", "d", "c", "y", "slash", "s"):
            assert key in shown, "{} is not advertised in the footer".format(key)


async def test_duplicate_snippet(app: ToolhApp, store: SnippetStore, sample_snippets):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("ctrl+d")
        await pilot.pause()
        assert store.count() == 4
        assert any(s.title.endswith("(copy)") for s in store.list_snippets())


async def test_reload_picks_up_external_changes(
    app: ToolhApp, store: SnippetStore, sample_snippets
):
    """The UI must reflect storage, including writes it did not make itself."""
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        assert len(app.snippets) == 3
        store.create(Snippet(title="added-behind-the-scenes", code="x"))
        await pilot.press("ctrl+r")
        await pilot.pause()
        assert len(app.snippets) == 4
        assert "added-behind-the-scenes" in [s.title for s in app.snippets]


async def test_empty_library_guides_the_user(app: ToolhApp):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        assert app.snippets == []
        text = detail_text(app)
        assert "No snippets yet" in text
        assert "ctrl+n" in text


async def test_actions_on_empty_library_do_not_crash(app: ToolhApp, store: SnippetStore):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        for key in ("c", "y", "d", "r", "ctrl+e", "ctrl+d", "j", "k", "g", "G", "s"):
            await pilot.press(key)
            await pilot.pause()
        assert app.is_running
        assert store.count() == 0


async def test_multiline_code_is_stored_and_shown(app: ToolhApp, store: SnippetStore):
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("ctrl+n")
        await pilot.pause()
        await type_text(pilot, "multi")
        app.query_one("#f-code", TextArea).text = "line one\nline two\nline three"
        await pilot.press("ctrl+s")
        await pilot.pause()
        stored = store.list_snippets()[0]
        assert stored.code == "line one\nline two\nline three"
        assert stored.line_count == 3
        assert "line three" in detail_text(app)


async def test_search_survives_special_characters(app: ToolhApp, store: SnippetStore):
    store.create(Snippet(title="odd [brackets] 100%", code="a[b]", language="python"))
    async with app.run_test(size=(120, 40)) as pilot:
        await pilot.pause()
        await pilot.press("slash")
        await type_text(pilot, "100%")
        await pilot.pause()
        assert [s.title for s in app.snippets] == ["odd [brackets] 100%"]
        # Rich markup in content must be displayed, not interpreted or crash.
        await pilot.press("escape")
        await pilot.pause()
        assert "brackets" in detail_text(app)
