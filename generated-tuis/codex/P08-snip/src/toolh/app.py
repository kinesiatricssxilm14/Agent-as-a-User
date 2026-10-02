"""Textual user interface for toolh."""

from __future__ import annotations

from typing import Dict, List, Optional

from rich.text import Text
from textual import on
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Container, Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import (
    Button,
    Footer,
    Header,
    Input,
    Label,
    OptionList,
    Static,
    TextArea,
)
from textual.widgets.option_list import Option

from .models import Snippet
from .storage import SnippetStore


class SnippetForm(ModalScreen[Optional[Dict[str, str]]]):
    """Create/edit form. All fields stay together in one scrollable view."""

    BINDINGS = [
        Binding("ctrl+s", "save", "Save", priority=True),
        Binding("escape", "cancel", "Cancel", priority=True),
    ]

    CSS = """
    SnippetForm {
        align: center middle;
        background: $background 80%;
    }
    #form-dialog {
        width: 88%;
        height: 92%;
        border: round $accent;
        background: $surface;
        padding: 1 2;
    }
    #form-title {
        text-style: bold;
        color: $accent;
        margin-bottom: 1;
    }
    .field-label {
        margin-top: 1;
        color: $text-muted;
    }
    #description-input {
        height: 5;
    }
    #code-input {
        height: 1fr;
        min-height: 8;
        border: tall $primary;
    }
    #form-error {
        height: 1;
        color: $error;
        margin-top: 1;
    }
    #form-buttons {
        height: 3;
        align-horizontal: right;
    }
    #form-buttons Button {
        margin-left: 1;
    }
    """

    def __init__(self, snippet: Optional[Snippet] = None) -> None:
        super().__init__()
        self.snippet = snippet

    def compose(self) -> ComposeResult:
        item = self.snippet
        with VerticalScroll(id="form-dialog"):
            yield Static("Edit snippet" if item else "Create snippet", id="form-title")
            yield Label("Title *", classes="field-label")
            yield Input(value=item.title if item else "", id="title-input")
            yield Label("Language", classes="field-label")
            yield Input(value=item.language if item else "", id="language-input")
            yield Label("Tags (comma-separated)", classes="field-label")
            yield Input(value=item.tags if item else "", id="tags-input")
            yield Label("Description", classes="field-label")
            yield TextArea(item.description if item else "", id="description-input")
            yield Label("Code", classes="field-label")
            yield TextArea(item.code if item else "", id="code-input", show_line_numbers=True)
            yield Static("", id="form-error")
            with Horizontal(id="form-buttons"):
                yield Button("Cancel  Esc", id="cancel", variant="default")
                yield Button("Save  Ctrl+S", id="save", variant="primary")

    def on_mount(self) -> None:
        self.query_one("#title-input", Input).focus()

    def action_cancel(self) -> None:
        self.dismiss(None)

    def action_save(self) -> None:
        title = self.query_one("#title-input", Input).value.strip()
        if not title:
            self.query_one("#form-error", Static).update("Title is required.")
            self.query_one("#title-input", Input).focus()
            return
        self.dismiss(
            {
                "title": title,
                "language": self.query_one("#language-input", Input).value.strip(),
                "tags": self.query_one("#tags-input", Input).value.strip(),
                "description": self.query_one("#description-input", TextArea).text,
                "code": self.query_one("#code-input", TextArea).text,
            }
        )

    @on(Button.Pressed, "#save")
    def save_button(self) -> None:
        self.action_save()

    @on(Button.Pressed, "#cancel")
    def cancel_button(self) -> None:
        self.action_cancel()


class DeleteDialog(ModalScreen[bool]):
    BINDINGS = [
        Binding("y", "confirm", "Yes", priority=True),
        Binding("enter", "confirm", "Yes", priority=True),
        Binding("n", "cancel", "No", priority=True),
        Binding("escape", "cancel", "No", priority=True),
    ]

    CSS = """
    DeleteDialog {
        align: center middle;
        background: $background 70%;
    }
    #delete-dialog {
        width: 58;
        height: auto;
        border: round $error;
        background: $surface;
        padding: 2;
    }
    #delete-question {
        margin-bottom: 1;
    }
    #delete-buttons {
        height: 3;
        align-horizontal: right;
    }
    #delete-buttons Button {
        margin-left: 1;
    }
    """

    def __init__(self, title: str) -> None:
        super().__init__()
        self.snippet_title = title

    def compose(self) -> ComposeResult:
        with Vertical(id="delete-dialog"):
            yield Static(
                Text.assemble(
                    ("Delete “", "bold"),
                    (self.snippet_title, "bold red"),
                    ("”?\nThis permanently removes it from SQLite.", "bold"),
                ),
                id="delete-question",
            )
            with Horizontal(id="delete-buttons"):
                yield Button("No  N/Esc", id="no")
                yield Button("Delete  Y/Enter", id="yes", variant="error")

    def action_confirm(self) -> None:
        self.dismiss(True)

    def action_cancel(self) -> None:
        self.dismiss(False)

    @on(Button.Pressed, "#yes")
    def yes_button(self) -> None:
        self.action_confirm()

    @on(Button.Pressed, "#no")
    def no_button(self) -> None:
        self.action_cancel()


class HelpDialog(ModalScreen[None]):
    BINDINGS = [
        Binding("escape", "close", "Close", priority=True),
        Binding("question_mark", "close", "Close", priority=True),
        Binding("q", "close", "Close", priority=True),
    ]

    CSS = """
    HelpDialog {
        align: center middle;
        background: $background 75%;
    }
    #help-dialog {
        width: 72;
        height: auto;
        max-height: 90%;
        border: round $accent;
        background: $surface;
        padding: 1 2;
    }
    #help-title {
        text-style: bold;
        color: $accent;
        margin-bottom: 1;
    }
    #help-close {
        width: 100%;
        margin-top: 1;
    }
    """

    HELP = """[b]Library[/b]
  ↑/↓ or j/k   select snippet       Enter   focus detail
  /            focus live search    Esc     clear/leave search
  n            create               e       edit / rename
  d            delete               c       copy code
  r            refresh SQLite       q       quit

[b]Create / Edit[/b]
  Tab / Shift+Tab   move between fields
  Ctrl+S            validate and save
  Esc               cancel without saving

[b]Delete confirmation[/b]
  y or Enter        permanently delete
  n or Esc          keep snippet

Search covers title, language, tags, description, and code.
Storage location is shown in the main screen status line."""

    def compose(self) -> ComposeResult:
        with VerticalScroll(id="help-dialog"):
            yield Static("toolh keyboard reference", id="help-title")
            yield Static(self.HELP)
            yield Button("Close  Esc / ? / Q", id="help-close", variant="primary")

    def action_close(self) -> None:
        self.dismiss(None)

    @on(Button.Pressed, "#help-close")
    def close_button(self) -> None:
        self.action_close()


class ToolhApp(App[None]):
    TITLE = "toolh"
    SUB_TITLE = "code snippet library"
    ENABLE_COMMAND_PALETTE = False

    CSS = """
    Screen {
        layout: vertical;
    }
    Header {
        height: 1;
    }
    #search-row {
        height: 3;
        padding: 0 1;
        background: $panel;
    }
    #search-label {
        width: 10;
        padding: 1 1 0 0;
        color: $text-muted;
    }
    #search {
        width: 1fr;
    }
    #body {
        height: 1fr;
    }
    #sidebar {
        width: 34%;
        min-width: 25;
        border-right: solid $primary;
    }
    #count {
        height: 2;
        padding: 0 1;
        color: $text-muted;
    }
    #snippet-list {
        height: 1fr;
        border: none;
    }
    #detail {
        width: 66%;
        padding: 0 2 1 2;
    }
    #detail-title {
        height: auto;
        min-height: 2;
        text-style: bold;
        color: $accent;
        padding-top: 1;
    }
    .meta {
        height: auto;
        min-height: 1;
        color: $text;
    }
    .section-title {
        height: 2;
        padding-top: 1;
        color: $text-muted;
        text-style: bold;
    }
    #detail-description {
        height: auto;
        min-height: 3;
        max-height: 25%;
        border: round $panel-lighten-2;
        padding: 0 1;
    }
    #detail-code {
        height: 1fr;
        min-height: 7;
        border: round $primary;
    }
    #empty-state {
        height: 1fr;
        content-align: center middle;
        color: $text-muted;
        display: none;
    }
    #storage-status {
        height: 1;
        padding: 0 1;
        background: $panel;
        color: $text-muted;
    }
    Footer {
        height: 1;
    }
    """

    BINDINGS = [
        Binding("n", "new", "New"),
        Binding("e", "edit", "Edit"),
        Binding("d", "delete", "Delete"),
        Binding("c", "copy", "Copy"),
        Binding("slash", "search", "Search"),
        Binding("r", "refresh", "Refresh", show=False),
        Binding("question_mark", "help", "Help"),
        Binding("q", "quit", "Quit"),
        Binding("j", "down", "Down", show=False),
        Binding("k", "up", "Up", show=False),
        Binding("escape", "escape", "Clear search", show=False),
    ]

    def __init__(self, store: Optional[SnippetStore] = None) -> None:
        super().__init__()
        self.store = store or SnippetStore()
        self.snippets: List[Snippet] = []
        self.selected_id: Optional[int] = None

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal(id="search-row"):
            yield Label("Search  /", id="search-label")
            yield Input(placeholder="Type to filter title, language, tags, description, or code…", id="search")
        with Horizontal(id="body"):
            with Vertical(id="sidebar"):
                yield Static("0 snippets", id="count")
                yield OptionList(id="snippet-list")
            with Container(id="detail"):
                yield Static("", id="detail-title")
                yield Static("", id="detail-language", classes="meta")
                yield Static("", id="detail-tags", classes="meta")
                yield Static("DESCRIPTION", classes="section-title")
                yield Static("", id="detail-description")
                yield Static("CODE", classes="section-title")
                yield TextArea("", id="detail-code", read_only=True, show_line_numbers=True)
                yield Static(
                    "No snippets yet.\n\nPress N to create your first snippet.",
                    id="empty-state",
                )
        yield Static(f"SQLite: {self.store.path}", id="storage-status")
        yield Footer()

    def on_mount(self) -> None:
        self.refresh_snippets()
        self.query_one("#snippet-list", OptionList).focus()

    def refresh_snippets(self, select_id: Optional[int] = None) -> None:
        search = self.query_one("#search", Input).value if self.is_mounted else ""
        self.snippets = self.store.list(search)
        options = [
            Option(
                Text.assemble(
                    (snippet.title, "bold"),
                    "\n",
                    (snippet.language or "plain text", "dim cyan"),
                    (f"  {snippet.tags}" if snippet.tags else "", "dim"),
                ),
                id=str(snippet.id),
            )
            for snippet in self.snippets
        ]
        listing = self.query_one("#snippet-list", OptionList)
        listing.clear_options()
        listing.add_options(options)

        noun = "snippet" if len(self.snippets) == 1 else "snippets"
        suffix = " matching search" if search else ""
        self.query_one("#count", Static).update(f"{len(self.snippets)} {noun}{suffix}")

        wanted = select_id if select_id is not None else self.selected_id
        index = next(
            (i for i, snippet in enumerate(self.snippets) if snippet.id == wanted),
            0 if self.snippets else None,
        )
        if index is None:
            self.selected_id = None
            self.show_snippet(None)
        else:
            listing.highlighted = index
            self.selected_id = self.snippets[index].id
            self.show_snippet(self.snippets[index])

    def current_snippet(self) -> Optional[Snippet]:
        if self.selected_id is None:
            return None
        return next((item for item in self.snippets if item.id == self.selected_id), None)

    def show_snippet(self, snippet: Optional[Snippet]) -> None:
        detail_widgets = [
            self.query_one("#detail-title", Static),
            self.query_one("#detail-language", Static),
            self.query_one("#detail-tags", Static),
            *list(self.query(".section-title")),
            self.query_one("#detail-description", Static),
            self.query_one("#detail-code", TextArea),
        ]
        empty = self.query_one("#empty-state", Static)
        if snippet is None:
            for widget in detail_widgets:
                widget.display = False
            empty.display = True
            return

        for widget in detail_widgets:
            widget.display = True
        empty.display = False
        self.query_one("#detail-title", Static).update(Text(snippet.title))
        self.query_one("#detail-language", Static).update(
            Text.assemble(("Language: ", "bold"), snippet.language or "—")
        )
        self.query_one("#detail-tags", Static).update(
            Text.assemble(("Tags: ", "bold"), snippet.tags or "—")
        )
        self.query_one("#detail-description", Static).update(
            Text(snippet.description or "—")
        )
        code = self.query_one("#detail-code", TextArea)
        code.load_text(snippet.code)
        # Language is user-defined data. TextArea only knows a fixed set of
        # syntax names, so an uncommon language must never prevent viewing the
        # stored code exactly as entered.
        try:
            code.language = snippet.language.lower() if snippet.language else None
        except Exception:
            code.language = None

    @on(Input.Changed, "#search")
    def search_changed(self) -> None:
        self.selected_id = None
        self.refresh_snippets()

    @on(Input.Submitted, "#search")
    def search_submitted(self) -> None:
        self.query_one("#snippet-list", OptionList).focus()

    @on(OptionList.OptionHighlighted, "#snippet-list")
    def option_highlighted(self, event: OptionList.OptionHighlighted) -> None:
        try:
            snippet_id = int(event.option_id)
        except (TypeError, ValueError):
            return
        self.selected_id = snippet_id
        self.show_snippet(self.current_snippet())

    @on(OptionList.OptionSelected, "#snippet-list")
    def option_selected(self) -> None:
        self.query_one("#detail-code", TextArea).focus()

    def action_new(self) -> None:
        self.push_screen(SnippetForm(), self._created)

    def _created(self, values: Optional[Dict[str, str]]) -> None:
        if values is None:
            self.notify("Create cancelled", severity="warning")
            return
        snippet = self.store.create(**values)
        self.query_one("#search", Input).value = ""
        self.refresh_snippets(snippet.id)
        self.notify(f"Saved “{snippet.title}”", title="Snippet created")

    def action_edit(self) -> None:
        snippet = self.current_snippet()
        if snippet is None:
            self.notify("Select a snippet to edit", severity="warning")
            return
        self.push_screen(SnippetForm(snippet), self._edited)

    def _edited(self, values: Optional[Dict[str, str]]) -> None:
        if values is None:
            self.notify("Edit cancelled", severity="warning")
            return
        snippet_id = self.selected_id
        if snippet_id is None:
            return
        updated = self.store.update(snippet_id, **values)
        if updated is None:
            self.notify("Snippet no longer exists", severity="error")
            self.refresh_snippets()
            return
        self.refresh_snippets(updated.id)
        self.notify(f"Saved “{updated.title}”", title="Snippet updated")

    def action_delete(self) -> None:
        snippet = self.current_snippet()
        if snippet is None:
            self.notify("Select a snippet to delete", severity="warning")
            return
        self.push_screen(DeleteDialog(snippet.title), self._deleted)

    def _deleted(self, confirmed: bool) -> None:
        if not confirmed:
            self.notify("Delete cancelled")
            return
        snippet = self.current_snippet()
        if snippet is None:
            return
        if self.store.delete(snippet.id):
            title = snippet.title
            self.selected_id = None
            self.refresh_snippets()
            self.notify(f"Deleted “{title}”", title="Snippet deleted")
        else:
            self.notify("Snippet no longer exists", severity="error")

    def action_copy(self) -> None:
        snippet = self.current_snippet()
        if snippet is None:
            self.notify("Select a snippet to copy", severity="warning")
            return
        self.copy_to_clipboard(snippet.code)
        self.notify(f"Copied {len(snippet.code)} characters", title="Code copied")

    def action_search(self) -> None:
        self.query_one("#search", Input).focus()

    def action_refresh(self) -> None:
        self.refresh_snippets()
        self.notify("Reloaded snippets from SQLite")

    def action_help(self) -> None:
        self.push_screen(HelpDialog())

    def action_down(self) -> None:
        listing = self.query_one("#snippet-list", OptionList)
        listing.focus()
        listing.action_cursor_down()

    def action_up(self) -> None:
        listing = self.query_one("#snippet-list", OptionList)
        listing.focus()
        listing.action_cursor_up()

    def action_escape(self) -> None:
        search = self.query_one("#search", Input)
        if search.value:
            search.value = ""
        self.query_one("#snippet-list", OptionList).focus()


def main() -> None:
    ToolhApp().run()
