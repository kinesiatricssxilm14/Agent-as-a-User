"""Full-screen views used by the toolh TUI.

Every "prompt" here is a real, full-screen ``Screen`` — not a modal overlay —
so the main list and detail panels are never obscured behind a dialog.
"""

from __future__ import annotations

from rich.panel import Panel
from rich.table import Table
from rich.text import Text
from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Center, VerticalScroll
from textual.screen import Screen
from textual.widgets import Footer, Header, Input, Label, Static, TextArea

from .models import Snippet


class SnippetFormScreen(Screen):
    """Create or edit a snippet with all fields visible at once."""

    BINDINGS = [
        Binding("ctrl+s", "save", "Save"),
        Binding("escape", "cancel", "Cancel"),
    ]

    def __init__(self, snippet: Snippet | None = None) -> None:
        super().__init__()
        self.snippet = snippet if snippet is not None else Snippet()
        self.is_new = snippet is None

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        heading = (
            "New snippet — fill in every field, then [b]Ctrl+S[/b] to save "
            "or [b]Esc[/b] to cancel"
            if self.is_new
            else f"Edit snippet #{self.snippet.id} — [b]Ctrl+S[/b] to save, "
            "[b]Esc[/b] to cancel"
        )
        yield Static(heading, id="form-heading")
        with VerticalScroll(id="form-scroll"):
            yield Label("Title", classes="form-label")
            yield Input(
                value=self.snippet.title,
                placeholder="Short title, e.g. hello-world",
                id="form-title",
            )
            yield Label("Language", classes="form-label")
            yield Input(
                value=self.snippet.language,
                placeholder="e.g. python, bash, sql, javascript",
                id="form-language",
            )
            yield Label("Tags (comma-separated)", classes="form-label")
            yield Input(
                value=self.snippet.tags,
                placeholder="e.g. demo, web, util",
                id="form-tags",
            )
            yield Label("Description", classes="form-label")
            yield Input(
                value=self.snippet.description,
                placeholder="What does this snippet do?",
                id="form-description",
            )
            yield Label("Code", classes="form-label")
            yield TextArea(
                self.snippet.code,
                id="form-code",
                show_line_numbers=True,
                soft_wrap=True,
                placeholder="# paste or type your code here",
            )
        yield Footer()

    def on_mount(self) -> None:
        self.query_one("#form-title", Input).focus()

    def action_save(self) -> None:
        title = self.query_one("#form-title", Input).value.strip()
        if not title:
            self.notify("Title is required.", severity="error")
            self.query_one("#form-title", Input).focus()
            return
        data = {
            "title": title,
            "language": self.query_one("#form-language", Input).value.strip(),
            "tags": self.query_one("#form-tags", Input).value.strip(),
            "description": self.query_one("#form-description", Input).value.strip(),
            "code": self.query_one("#form-code", TextArea).text,
        }
        self.dismiss((self.snippet.id, data))

    def action_cancel(self) -> None:
        self.dismiss(None)


class RenameScreen(Screen):
    """Change only the title of a snippet."""

    BINDINGS = [
        Binding("ctrl+s", "save", "Save"),
        Binding("escape", "cancel", "Cancel"),
    ]

    def __init__(self, snippet: Snippet) -> None:
        super().__init__()
        self.snippet = snippet

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        yield Static(
            f"Rename snippet #{self.snippet.id} — [b]Ctrl+S[/b] or [b]Enter[/b] "
            "to save, [b]Esc[/b] to cancel",
            id="form-heading",
        )
        with VerticalScroll(id="form-scroll"):
            yield Label("Current title", classes="form-label")
            yield Static(
                Text(self.snippet.title or "(untitled)", style="dim"), id="rename-current"
            )
            yield Label("New title", classes="form-label")
            yield Input(value=self.snippet.title, placeholder="New title", id="rename-title")
        yield Footer()

    def on_mount(self) -> None:
        title_input = self.query_one("#rename-title", Input)
        title_input.focus()
        title_input.action_end()

    def action_save(self) -> None:
        title = self.query_one("#rename-title", Input).value.strip()
        if not title:
            self.notify("Title cannot be empty.", severity="error")
            self.query_one("#rename-title", Input).focus()
            return
        self.dismiss((self.snippet.id, title))

    def action_cancel(self) -> None:
        self.dismiss(None)

    @on(Input.Submitted, "#rename-title")
    def _on_rename_submitted(self) -> None:
        self.action_save()


class ConfirmScreen(Screen):
    """Full-screen yes/no confirmation."""

    BINDINGS = [
        Binding("y", "confirm", "Yes"),
        Binding("enter", "confirm", "Yes"),
        Binding("n", "cancel", "No"),
        Binding("escape", "cancel", "No"),
    ]

    def __init__(self, message: Text | str) -> None:
        super().__init__()
        self.message = message

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        with Center():
            yield Static(self.message, id="confirm-message")
        yield Footer()

    def action_confirm(self) -> None:
        self.dismiss(True)

    def action_cancel(self) -> None:
        self.dismiss(False)


class HelpScreen(Screen):
    """Scrollable keyboard-shortcut reference."""

    BINDINGS = [
        Binding("escape", "close", "Close"),
        Binding("q", "close", "Close"),
    ]

    def __init__(self, items: list[tuple[str, str]]) -> None:
        super().__init__()
        self.items = items

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        with VerticalScroll(id="help-scroll"):
            yield Static(self._build_help(), id="help-content")
        yield Footer()

    def _build_help(self) -> Panel:
        table = Table(title="toolh — Keyboard Shortcuts", expand=True)
        table.add_column("Key", style="bold cyan", no_wrap=True)
        table.add_column("Action", style="white")
        for key, description in self.items:
            table.add_row(key, description)
        return Panel(
            table,
            border_style="blue",
            title="Help",
            subtitle="Press Esc or q to close",
        )

    def action_close(self) -> None:
        self.dismiss(None)
