"""The toolh Textual application."""

from __future__ import annotations

from rich.console import Group
from rich.markup import escape
from rich.panel import Panel
from rich.syntax import Syntax
from rich.text import Text

from textual import on
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.widgets import (
    Footer,
    Header,
    Input,
    Label,
    ListItem,
    ListView,
    Static,
)

from .clipboard import copy_to_clipboard
from .db import SnippetStore
from .models import Snippet
from .screens import ConfirmScreen, HelpScreen, RenameScreen, SnippetFormScreen


class ToolhApp(App):
    """Keyboard-first code snippet management TUI."""

    TITLE = "toolh"
    SUB_TITLE = "code snippet library"

    CSS = """
    #search-row {
        height: 3;
        padding: 0 1;
        align: center middle;
    }
    #search {
        width: 1fr;
    }
    #count {
        width: auto;
        padding: 0 1;
        color: $text-muted;
    }
    #main {
        height: 1fr;
    }
    #list-pane {
        width: 34%;
        border: round #38465c;
        height: 1fr;
    }
    #detail-pane {
        width: 1fr;
        border: round #38465c;
        padding: 0 1;
    }
    #snippet-list {
        border: none;
        height: 1fr;
    }
    #detail {
        border: none;
        width: 100%;
    }
    #form-heading {
        height: 2;
        padding: 0 1;
        color: $text-muted;
        background: $boost;
        content-align: left middle;
    }
    #form-scroll {
        padding: 0 1;
    }
    .form-label {
        color: $text-muted;
        margin: 1 0 0 0;
        height: auto;
    }
    #form-title, #form-language, #form-tags, #form-description {
        border: solid $panel;
        margin-bottom: 0;
    }
    #form-code {
        height: 18;
        border: solid $panel;
        margin-bottom: 1;
    }
    #rename-current {
        padding: 1 2;
        border: round $panel;
        height: auto;
        margin-bottom: 1;
    }
    #confirm-message {
        width: auto;
        height: auto;
        padding: 1 3;
        border: round #38465c;
    }
    #help-scroll {
        padding: 0 1;
    }
    """

    BINDINGS = [
        Binding("n", "new_snippet", "New"),
        Binding("e", "edit_snippet", "Edit"),
        Binding("r", "rename_snippet", "Rename"),
        Binding("c", "copy_snippet", "Copy"),
        Binding("d", "delete_snippet", "Delete"),
        Binding("/", "focus_search", "Search"),
        Binding("?", "show_help", "Help"),
        Binding("q", "quit", "Quit"),
        Binding("j", "list_down", "Down", show=False),
        Binding("k", "list_up", "Up", show=False),
    ]

    HELP_ITEMS: list[tuple[str, str]] = [
        ("↑ / ↓  or  j / k", "Move the highlight through the snippet list"),
        ("Enter", "Return focus to the snippet list (from search)"),
        ("/", "Focus the search box and filter snippets in real time"),
        ("n", "Create a new snippet"),
        ("e", "Edit the highlighted snippet (all fields)"),
        ("r", "Rename the highlighted snippet (change its title)"),
        ("c", "Copy the highlighted snippet's code to the clipboard"),
        ("d", "Delete the highlighted snippet (asks for confirmation)"),
        ("?", "Show this help screen"),
        ("Ctrl+S", "Save the current form"),
        ("Esc", "Cancel the current screen / go back"),
        ("Tab", "Move focus between form fields"),
        ("q  /  Ctrl+Q", "Quit toolh"),
    ]

    def __init__(self, db_path: str | None = None) -> None:
        super().__init__()
        self._db_path = db_path
        self.store: SnippetStore | None = None
        self._snippets: list[Snippet] = []

    # ------------------------------------------------------------------ #
    # Lifecycle
    # ------------------------------------------------------------------ #
    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        with Horizontal(id="search-row"):
            yield Input(
                placeholder="Search title, language, tags, description, code…",
                id="search",
            )
            yield Static("0 snippets", id="count")
        with Horizontal(id="main"):
            with Vertical(id="list-pane"):
                yield ListView(id="snippet-list")
            with VerticalScroll(id="detail-pane"):
                yield Static(id="detail")
        yield Footer()

    def on_mount(self) -> None:
        self.store = SnippetStore(self._db_path)
        self.reload()
        self.query_one("#snippet-list", ListView).focus()

    def on_unmount(self) -> None:
        if self.store is not None:
            self.store.close()
            self.store = None

    # ------------------------------------------------------------------ #
    # Rendering helpers
    # ------------------------------------------------------------------ #
    @staticmethod
    def _count_text(count: int) -> str:
        return f"{count} snippet{'s' if count != 1 else ''}"

    @staticmethod
    def _build_item(snippet: Snippet) -> ListItem:
        label = Text()
        label.append(snippet.title or "(untitled)", style="bold")
        if snippet.language:
            label.append("   ")
            label.append(snippet.language, style="dim italic")
        return ListItem(Label(label))

    def reload(self, keep_id: int | None = None) -> None:
        """Rebuild the list from storage and update the detail panel."""
        if self.store is None:
            return

        query = self.query_one("#search", Input).value.strip()
        snippets = self.store.search(query) if query else self.store.all()
        self._snippets = snippets

        list_view = self.query_one("#snippet-list", ListView)
        list_view.clear()
        if snippets:
            list_view.extend([self._build_item(s) for s in snippets])

        self.query_one("#count", Static).update(self._count_text(len(snippets)))

        if not snippets:
            self._show_detail(None)
            return

        index = 0
        if keep_id is not None:
            for i, snippet in enumerate(snippets):
                if snippet.id == keep_id:
                    index = i
                    break
        list_view.index = index
        self._show_current()

    def _show_current(self) -> None:
        list_view = self.query_one("#snippet-list", ListView)
        index = list_view.index
        if index is None or not (0 <= index < len(self._snippets)):
            self._show_detail(None)
        else:
            self._show_detail(self._snippets[index])

    def _show_detail(self, snippet: Snippet | None) -> None:
        detail = self.query_one("#detail", Static)
        if snippet is None:
            hint = Text()
            hint.append("No snippets to show.", style="bold white")
            hint.append("\n\nPress ", style="dim")
            hint.append("n", style="bold cyan")
            hint.append(" to create a new snippet, ", style="dim")
            hint.append("/", style="bold cyan")
            hint.append(" to search, or ", style="dim")
            hint.append("?", style="bold cyan")
            hint.append(" for help.", style="dim")
            detail.update(Panel(hint, title="Snippet", border_style="blue"))
            return

        meta = Group(
            Text.assemble(
                ("Title:       ", "bold cyan"),
                (snippet.title or "(untitled)", "bold white"),
            ),
            Text.assemble(
                ("Language:    ", "bold cyan"), (snippet.language or "—", "white")
            ),
            Text.assemble(
                ("Tags:        ", "bold cyan"),
                (", ".join(snippet.tag_list) if snippet.tag_list else "—", "white"),
            ),
            Text.assemble(
                ("Description: ", "bold cyan"),
                (snippet.description or "—", "white"),
            ),
            Text.assemble(
                ("Updated:     ", "bold cyan"),
                (snippet.updated_at or "—", "dim"),
            ),
        )

        try:
            lexer = snippet.language.strip() or "text"
            code = Syntax(
                snippet.code or "(empty)",
                lexer,
                theme="monokai",
                word_wrap=True,
                line_numbers=False,
            )
        except Exception:
            code = Text(snippet.code or "(empty)")

        body = Group(meta, Text(), Text("Code:", style="bold yellow"), code)
        detail.update(
            Panel(body, title=f"Snippet #{snippet.id}", border_style="blue")
        )

    def _current(self) -> Snippet | None:
        list_view = self.query_one("#snippet-list", ListView)
        index = list_view.index
        if index is None or not (0 <= index < len(self._snippets)):
            return None
        return self._snippets[index]

    # ------------------------------------------------------------------ #
    # Events
    # ------------------------------------------------------------------ #
    @on(Input.Changed, "#search")
    def _on_search_changed(self) -> None:
        self.reload()

    @on(Input.Submitted, "#search")
    def _on_search_submitted(self) -> None:
        self.query_one("#snippet-list", ListView).focus()

    @on(ListView.Highlighted)
    def _on_highlighted(self) -> None:
        self._show_current()

    # ------------------------------------------------------------------ #
    # Actions
    # ------------------------------------------------------------------ #
    def action_new_snippet(self) -> None:
        self.push_screen(SnippetFormScreen(), callback=self._on_form_result)

    def action_edit_snippet(self) -> None:
        current = self._current()
        if current is None:
            self.notify("No snippet selected.", severity="warning")
            return
        self.push_screen(SnippetFormScreen(current), callback=self._on_form_result)

    def action_rename_snippet(self) -> None:
        current = self._current()
        if current is None:
            self.notify("No snippet selected.", severity="warning")
            return
        self.push_screen(RenameScreen(current), callback=self._on_rename_result)

    def action_delete_snippet(self) -> None:
        current = self._current()
        if current is None:
            self.notify("No snippet selected.", severity="warning")
            return
        snippet_id = current.id

        message = Text.assemble(
            ("Delete snippet ", "bold"),
            (f'"{current.title}"', "bold red"),
            ("?", "bold"),
            ("\n\nThis cannot be undone.", "dim"),
        )

        def _confirmed(confirmed: bool) -> None:
            if confirmed:
                self._delete_snippet(snippet_id)

        self.push_screen(ConfirmScreen(message), callback=_confirmed)

    def action_copy_snippet(self) -> None:
        current = self._current()
        if current is None:
            self.notify("No snippet selected.", severity="warning")
            return
        ok, message = copy_to_clipboard(current.code)
        self.notify(message, severity="success" if ok else "warning")

    def action_focus_search(self) -> None:
        self.query_one("#search", Input).focus()

    def action_show_help(self) -> None:
        self.push_screen(HelpScreen(self.HELP_ITEMS))

    def action_list_down(self) -> None:
        self._move_highlight(1)

    def action_list_up(self) -> None:
        self._move_highlight(-1)

    def _move_highlight(self, delta: int) -> None:
        if not self._snippets:
            return
        list_view = self.query_one("#snippet-list", ListView)
        current = list_view.index if list_view.index is not None else -1
        new_index = max(0, min(len(self._snippets) - 1, current + delta))
        list_view.index = new_index

    # ------------------------------------------------------------------ #
    # Persistence callbacks
    # ------------------------------------------------------------------ #
    def _on_form_result(self, result: object) -> None:
        if not result:
            return
        snippet_id, data = result  # type: ignore[misc]
        if snippet_id is None:
            new_id = self.store.add(Snippet(**data))
            self.notify(
                f"Created snippet {escape(data['title'])}.", severity="success"
            )
            self.reload(keep_id=new_id)
        else:
            self.store.update(Snippet(id=snippet_id, **data))
            self.notify(
                f"Updated snippet {escape(data['title'])}.", severity="success"
            )
            self.reload(keep_id=snippet_id)

    def _on_rename_result(self, result: object) -> None:
        if not result:
            return
        snippet_id, title = result  # type: ignore[misc]
        self.store.rename(snippet_id, title)
        self.notify(f"Renamed snippet to {escape(title)}.", severity="success")
        self.reload(keep_id=snippet_id)

    def _delete_snippet(self, snippet_id: int) -> None:
        snippet = self.store.get(snippet_id)
        self.store.delete(snippet_id)
        name = snippet.title if snippet else str(snippet_id)
        self.notify(f"Deleted snippet {escape(name)}.", severity="success")
        self.reload()
