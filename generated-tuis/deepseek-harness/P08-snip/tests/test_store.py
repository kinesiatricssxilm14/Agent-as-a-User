"""Unit tests for the non-UI core (storage, config, clipboard)."""

from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

from toolh.config import resolve_db_path
from toolh.db import SnippetStore
from toolh.models import Snippet


class SnippetStoreTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.store = SnippetStore(str(Path(self._tmp.name) / "snippets.db"))

    def tearDown(self) -> None:
        self.store.close()
        self._tmp.cleanup()

    def test_crud_round_trip(self) -> None:
        snippet_id = self.store.add(
            Snippet(
                title="hello-world",
                language="python",
                tags="demo, greeter",
                description="Print a greeting",
                code='print("hello")',
            )
        )
        self.assertIsInstance(snippet_id, int)

        fetched = self.store.get(snippet_id)
        self.assertIsNotNone(fetched)
        assert fetched is not None
        self.assertEqual(fetched.title, "hello-world")
        self.assertEqual(fetched.language, "python")
        self.assertEqual(fetched.tag_list, ["demo", "greeter"])
        self.assertEqual(fetched.description, "Print a greeting")
        self.assertEqual(fetched.code, 'print("hello")')
        self.assertEqual(self.store.count(), 1)

        # Update all fields.
        self.store.update(
            Snippet(
                id=snippet_id,
                title="hello-world",
                language="python",
                tags="demo",
                description="Greet the user",
                code='print("hi")',
            )
        )
        fetched = self.store.get(snippet_id)
        assert fetched is not None
        self.assertEqual(fetched.description, "Greet the user")
        self.assertEqual(fetched.code, 'print("hi")')
        self.assertEqual(fetched.tag_list, ["demo"])

        # Rename (title only).
        self.store.rename(snippet_id, "hello")
        fetched = self.store.get(snippet_id)
        assert fetched is not None
        self.assertEqual(fetched.title, "hello")
        # Rename must not clobber other fields.
        self.assertEqual(fetched.code, 'print("hi")')

        # Delete.
        self.store.delete(snippet_id)
        self.assertIsNone(self.store.get(snippet_id))
        self.assertEqual(self.store.count(), 0)

    def test_search_matches_all_fields(self) -> None:
        self.store.add(
            Snippet(
                title="greet",
                language="python",
                tags="demo",
                description="Say hello",
                code="print('hi')",
            )
        )
        self.store.add(
            Snippet(
                title="query",
                language="sql",
                tags="db",
                description="Select rows",
                code="SELECT * FROM users;",
            )
        )
        # Search by title.
        self.assertEqual(len(self.store.search("greet")), 1)
        # Search by language.
        self.assertEqual(len(self.store.search("sql")), 1)
        # Search by tag.
        self.assertEqual(len(self.store.search("db")), 1)
        # Search by description.
        self.assertEqual(len(self.store.search("hello")), 1)
        # Search by code body.
        self.assertEqual(len(self.store.search("SELECT *")), 1)
        # Case-insensitive.
        self.assertEqual(len(self.store.search("USERS")), 1)
        # No match.
        self.assertEqual(len(self.store.search("nomatch")), 0)


class ConfigTest(unittest.TestCase):
    def test_toolh_db_environment_override(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = str(Path(tmp) / "override.db")
            previous = os.environ.get("TOOLH_DB")
            os.environ["TOOLH_DB"] = target
            try:
                self.assertEqual(resolve_db_path(), target)
            finally:
                if previous is None:
                    os.environ.pop("TOOLH_DB", None)
                else:
                    os.environ["TOOLH_DB"] = previous

    def test_toolh_data_dir_environment_override(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            previous = os.environ.get("TOOLH_DATA_DIR")
            os.environ["TOOLH_DATA_DIR"] = tmp
            try:
                self.assertEqual(
                    resolve_db_path(), str(Path(tmp) / "snippets.db")
                )
            finally:
                if previous is None:
                    os.environ.pop("TOOLH_DATA_DIR", None)
                else:
                    os.environ["TOOLH_DATA_DIR"] = previous


if __name__ == "__main__":
    unittest.main()
