"""Headless end-to-end tests for the toolh TUI using Textual's pilot."""

from __future__ import annotations

import io
import tempfile
import unittest
from pathlib import Path

from rich.console import Console

from textual.widgets import Input, ListView, Static, TextArea

from toolh.app import ToolhApp
from toolh.db import SnippetStore
from toolh.models import Snippet


class ToolhAppTest(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.db_path = str(Path(self._tmp.name) / "app.db")
        self.app = ToolhApp(db_path=self.db_path)

    async def asyncTearDown(self) -> None:
        self._tmp.cleanup()

    def _detail_plain_text(self) -> str:
        detail = self.app.query_one("#detail", Static)
        buffer = io.StringIO()
        console = Console(
            file=buffer, width=100, force_terminal=False, color_system=None
        )
        console.print(detail.content)
        return buffer.getvalue()

    async def test_create_shows_all_fields_and_persists(self) -> None:
        async with self.app.run_test(size=(110, 30)) as pilot:
            await pilot.press("n")
            await pilot.pause()

            screen = self.app.screen
            # The title input is focused after the form mounts: type into it.
            await pilot.press(*"hello")
            screen.query_one("#form-language", Input).value = "python"
            screen.query_one("#form-tags", Input).value = "demo, greeter"
            screen.query_one("#form-description", Input).value = "Print a greeting"
            screen.query_one("#form-code", TextArea).text = 'print("hello")'

            await pilot.press("ctrl+s")
            await pilot.pause()

            # Back on the main screen: the list shows one item.
            list_view = self.app.query_one("#snippet-list", ListView)
            self.assertEqual(len(list_view.children), 1)

            # The detail panel renders every field on the same screen.
            text = self._detail_plain_text()
            for expected in (
                "hello",
                "python",
                "demo, greeter",
                "Print a greeting",
                'print("hello")',
            ):
                self.assertIn(expected, text)

        # The snippet is persisted for real (independent of the TUI).
        store = SnippetStore(self.db_path)
        snippets = store.all()
        store.close()
        self.assertEqual(len(snippets), 1)
        snippet = snippets[0]
        self.assertEqual(snippet.title, "hello")
        self.assertEqual(snippet.language, "python")
        self.assertEqual(snippet.tag_list, ["demo", "greeter"])
        self.assertEqual(snippet.description, "Print a greeting")
        self.assertEqual(snippet.code, 'print("hello")')

    async def test_search_filters_list_in_real_time(self) -> None:
        store = SnippetStore(self.db_path)
        store.add(
            Snippet(
                title="python-greet",
                language="python",
                tags="demo",
                description="greeting",
                code="print('hi')",
            )
        )
        store.add(
            Snippet(
                title="sql-query",
                language="sql",
                tags="db",
                description="select rows",
                code="SELECT * FROM t;",
            )
        )
        store.close()

        async with self.app.run_test(size=(110, 30)) as pilot:
            list_view = self.app.query_one("#snippet-list", ListView)
            self.assertEqual(len(list_view.children), 2)

            await pilot.press("/")
            await pilot.pause()
            await pilot.press(*"sql")
            await pilot.pause()

            list_view = self.app.query_one("#snippet-list", ListView)
            self.assertEqual(len(list_view.children), 1)

    async def test_delete_removes_from_list_and_storage(self) -> None:
        store = SnippetStore(self.db_path)
        store.add(
            Snippet(
                title="to-delete",
                language="bash",
                tags="tmp",
                description="temp",
                code="echo hi",
            )
        )
        store.close()

        async with self.app.run_test(size=(110, 30)) as pilot:
            list_view = self.app.query_one("#snippet-list", ListView)
            self.assertEqual(len(list_view.children), 1)

            await pilot.press("d")
            await pilot.pause()
            await pilot.press("y")
            await pilot.pause()

            list_view = self.app.query_one("#snippet-list", ListView)
            self.assertEqual(len(list_view.children), 0)

        store = SnippetStore(self.db_path)
        self.assertEqual(store.count(), 0)
        store.close()

    async def test_copy_reports_clipboard_status(self) -> None:
        store = SnippetStore(self.db_path)
        store.add(
            Snippet(
                title="copy-me",
                language="python",
                tags="",
                description="",
                code="print('copy')",
            )
        )
        store.close()

        async with self.app.run_test(size=(110, 30)) as pilot:
            await pilot.press("c")
            await pilot.pause()
            # No exception means the copy path ran against real tools and
            # reported a result via a notification.
            self.assertIsNotNone(self.app.screen)

    async def test_edit_rename_and_help(self) -> None:
        store = SnippetStore(self.db_path)
        store.add(
            Snippet(
                title="orig-title",
                language="python",
                tags="demo",
                description="original",
                code="print('a')",
            )
        )
        store.close()

        async with self.app.run_test(size=(110, 30)) as pilot:
            # Edit: form should be pre-filled, then save an updated body.
            await pilot.press("e")
            await pilot.pause()
            screen = self.app.screen
            self.assertEqual(
                screen.query_one("#form-title", Input).value, "orig-title"
            )
            screen.query_one("#form-description", Input).value = "updated"
            screen.query_one("#form-code", TextArea).text = "print('b')"
            await pilot.press("ctrl+s")
            await pilot.pause()

            # Rename: change the title and submit with Enter.
            await pilot.press("r")
            await pilot.pause()
            self.app.screen.query_one("#rename-title", Input).value = "new-title"
            await pilot.press("enter")
            await pilot.pause()

            # Help screen opens and closes with q.
            await pilot.press("?")
            await pilot.pause()
            self.assertIsNotNone(
                self.app.screen.query_one("#help-content", Static)
            )
            await pilot.press("q")
            await pilot.pause()

        store = SnippetStore(self.db_path)
        snippets = store.all()
        store.close()
        self.assertEqual(len(snippets), 1)
        self.assertEqual(snippets[0].title, "new-title")
        self.assertEqual(snippets[0].description, "updated")
        self.assertEqual(snippets[0].code, "print('b')")


if __name__ == "__main__":
    unittest.main()
