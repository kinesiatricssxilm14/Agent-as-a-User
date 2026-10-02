"""Shared fixtures and helpers."""

from __future__ import annotations

import os
from pathlib import Path

import pytest
from textual.widgets import Static

from toolh.clipboard import Clipboard
from toolh.config import Paths
from toolh.models import Snippet
from toolh.storage import SnippetStore


def rendered(widget: Static) -> str:
    """Flatten a Static's content into plain text for assertions.

    Textual moved from ``.renderable`` to ``.visual`` (which may wrap a Rich
    renderable in a ``RichVisual``); handle both, and fall back to rendering
    through a Rich console so Tables and Syntax blocks become text.
    """
    content = getattr(widget, "visual", None)
    if content is None:
        content = getattr(widget, "renderable", "")
    content = getattr(content, "_renderable", content)  # unwrap RichVisual

    if isinstance(content, str):
        return content
    plain = getattr(content, "plain", None)
    if isinstance(plain, str):
        return plain

    from rich.console import Console

    with open(os.devnull, "w") as devnull:
        console = Console(width=200, record=True, file=devnull)
        console.print(content)
        return console.export_text()


def detail_text(app) -> str:
    """Everything currently visible in the detail pane, as one string."""
    return "\n".join(
        rendered(app.query_one(widget_id, Static))
        for widget_id in (
            "#detail-title",
            "#detail-meta",
            "#detail-description",
            "#detail-code",
        )
    )


async def type_text(pilot, text: str) -> None:
    """Type a literal string one key at a time (as a human would)."""
    for character in text:
        if character == " ":
            await pilot.press("space")
        elif character == "\n":
            await pilot.press("enter")
        else:
            await pilot.press(character)


@pytest.fixture()
def db_path(tmp_path: Path) -> Path:
    return tmp_path / "data" / "snippets.db"


@pytest.fixture()
def store(db_path: Path):
    store = SnippetStore(db_path)
    yield store
    store.close()


@pytest.fixture()
def paths(db_path: Path, tmp_path: Path) -> Paths:
    return Paths(
        database=db_path,
        config=tmp_path / "config.json",
        database_source="test fixture",
    )


@pytest.fixture()
def clipboard_sink(tmp_path: Path) -> Path:
    """File that the fake-but-real clipboard command writes into."""
    return tmp_path / "clipboard-sink.txt"


@pytest.fixture()
def clipboard(clipboard_sink: Path, tmp_path: Path) -> Clipboard:
    # A real subprocess writing to a real file: exercises the same code path a
    # user's xclip would, while staying verifiable in a headless test run.
    return Clipboard(
        command="cat > {}".format(clipboard_sink),
        mirror_path=tmp_path / "mirror.txt",
        allow_osc52=False,
    )


@pytest.fixture()
def sample_snippets(store: SnippetStore):
    data = [
        Snippet(
            title="hello-world",
            language="python",
            description="Print a greeting message",
            code='print("hello")',
            tags=["demo"],
        ),
        Snippet(
            title="git undo",
            language="bash",
            description="Undo the last commit, keeping changes staged",
            code="git reset --soft HEAD~1",
            tags=["git", "cli"],
        ),
        Snippet(
            title="sqlite top rows",
            language="sql",
            description="Select the newest rows from a table",
            code="SELECT * FROM events ORDER BY id DESC LIMIT 10;",
            tags=["db", "cli"],
        ),
    ]
    return [store.create(item) for item in data]
