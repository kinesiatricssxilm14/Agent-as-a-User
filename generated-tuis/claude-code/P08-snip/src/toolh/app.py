"""The toolh Textual application.

Layout (one screen, everything visible at once)::

    +-----------------------------------------------------------------+
    | Search: [ ...                    ]        sort: Recently updated |
    +-------------------------+---------------------------------------+
    | Snippets (3/5)          | Snippet detail                        |
    |  #  Title    Lang  Tags |  Title:       hello-world             |
    | >1  hello-w  python demo|  Language:    python                  |
    |  2  git und  bash   git |  Tags:        demo, cli               |
    |                         |  Description: Print a greeting        |
    |                         |  Code:                                |
    |                         |    print("hello")                     |
    +-------------------------+---------------------------------------+
    | (inline rename / delete-confirm bars appear here when needed)   |
    | status: 5 snippets - db /root/.local/share/toolh/snippets.db    |
    | ^n New  ^e Edit  r Rename  d Delete  c Copy  / Search  ? Help   |
    +-----------------------------------------------------------------+

Deliberate choices:

* The detail pane shows *all five* fields together -- no tabs, no pop-ups.
* Rename and delete-confirm are inline bars, so the snippet they act on stays
  on screen while you confirm.
* Every action is a key binding; the Footer plus a ``?`` help screen document
  them, so the keyboard map is discoverable from inside the app.
"""

from __future__ import annotations

from typing import List, Optional

from rich.syntax import Syntax
from rich.table import Table
from rich.text import Text
from textual import events, on
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import Screen
from textual.widgets import (
    DataTable,
    Footer,
    Header,
    Input,
    Static,
    TextArea,
)

from . import __version__
from .clipboard import Clipboard
from .config import Paths
from .models import Snippet, parse_tags
from .storage import SORT_MODES, SnippetStore, StorageError

__all__ = ["ToolhApp", "HelpScreen"]

#: Languages Textual can syntax-highlight in the editor; anything else is shown
#: as plain text rather than refusing to open.
_TEXTAREA_LANGUAGE_ALIASES = {
    "py": "python",
    "python3": "python",
    "sh": "bash",
    "shell": "bash",
    "zsh": "bash",
    "bash": "bash",
    "js": "javascript",
    "jsx": "javascript",
    "ts": "javascript",
    "node": "javascript",
    "yml": "yaml",
    "golang": "go",
    "rs": "rust",
    "md": "markdown",
    "htm": "html",
}

KEY_HELP: List[tuple] = [
    ("Navigation", "up / down, j / k", "Move the selection in the snippet list"),
    ("Navigation", "home / end, g / G", "Jump to the first / last snippet"),
    ("Navigation", "pageup / pagedown", "Scroll the list a screen at a time"),
    ("Navigation", "tab / shift+tab", "Move focus between search, list and panes"),
    ("Navigation", "enter", "Focus the detail pane to scroll a long snippet"),
    ("Search", "/ or ctrl+f", "Jump to the search box (filters as you type)"),
    ("Search", "escape", "Clear the search box / close the editor or a prompt"),
    ("Search", "s", "Cycle the sort order"),
    ("Create & edit", "ctrl+n", "Create a new snippet"),
    ("Create & edit", "ctrl+e or F2", "Edit the selected snippet"),
    ("Create & edit", "r", "Rename the selected snippet (title only)"),
    ("Create & edit", "ctrl+d", "Duplicate the selected snippet"),
    ("Create & edit", "ctrl+s", "Save while editing"),
    ("Create & edit", "d or delete", "Delete the selected snippet (asks first)"),
    ("Clipboard", "c", "Copy the snippet's code to the clipboard"),
    ("Clipboard", "y", "Copy the whole snippet (all fields) to the clipboard"),
    ("Other", "? or F1", "Show this help"),
    ("Other", "ctrl+r", "Reload from the database"),
    ("Other", "ctrl+q", "Quit toolh"),
]

SEARCH_HELP = (
    "Type any words to match title, language, description, code or tags. "
    "Scope a word with title:, lang:, tag:, desc:, code: or id:  -  quote "
    'phrases like "two words"  -  prefix with - to exclude.'
)


_AVAILABLE_LANGUAGES: Optional[frozenset] = None


def _textarea_language(language: str) -> Optional[str]:
    """Map a snippet language onto a TextArea highlighter, if one exists."""
    global _AVAILABLE_LANGUAGES
    name = (language or "").strip().lower()
    if not name:
        return None
    name = _TEXTAREA_LANGUAGE_ALIASES.get(name, name)
    if _AVAILABLE_LANGUAGES is None:
        try:
            _AVAILABLE_LANGUAGES = frozenset(TextArea().available_languages)
        except Exception:  # pragma: no cover - tree-sitter unavailable
            _AVAILABLE_LANGUAGES = frozenset()
    return name if name in _AVAILABLE_LANGUAGES else None


class HelpScreen(Screen):
    """Full-screen key reference (reachable with ``?``, closed with any key)."""

    BINDINGS = [
        # A local action (rather than ``app.pop_screen``) so it is resolved in
        # this screen's namespace and cannot be vetoed by the main app's
        # ``check_action`` guard.
        Binding("escape,q,question_mark,f1,enter,space", "close", "Back", show=True),
    ]

    def __init__(self, clipboard_backends: List[str], paths: Paths) -> None:
        super().__init__()
        self._backends = clipboard_backends
        self._paths = paths

    def action_close(self) -> None:
        self.app.pop_screen()

    def compose(self) -> ComposeResult:
        yield Header(show_clock=False)
        with VerticalScroll(id="help-body"):
            yield Static(
                Text.from_markup(
                    "[bold]toolh {}[/bold] - code snippet manager\n"
                    "[dim]Press escape or q to go back.[/dim]".format(__version__)
                ),
                id="help-title",
            )
            table = Table(
                show_header=True, header_style="bold", expand=True, pad_edge=False
            )
            table.add_column("Area", style="cyan", no_wrap=True)
            table.add_column("Keys", style="bold yellow", no_wrap=True)
            table.add_column("Action")
            last_area = None
            for area, keys, description in KEY_HELP:
                table.add_row("" if area == last_area else area, keys, description)
                last_area = area
            yield Static(table)
            yield Static(
                Text.from_markup(
                    "\n[bold cyan]Search syntax[/bold cyan]\n{}\n".format(SEARCH_HELP)
                )
            )
            backends = ", ".join(self._backends) if self._backends else "none detected"
            yield Static(
                Text.from_markup(
                    "\n[bold cyan]Storage & clipboard[/bold cyan]\n"
                    "Database:  {db}\n"
                    "Chosen by: {src}\n"
                    "Config:    {cfg}{cfg_state}\n"
                    "Clipboard: {backends}\n"
                    "\n[dim]Override the database with TOOLH_DB=/path/snippets.db, "
                    "TOOLH_HOME=/dir, the 'database' key in the config file, or "
                    "toolh --db /path. Set TOOLH_CLIPBOARD_COMMAND to pipe copies "
                    "into your own command, and TOOLH_CLIPBOARD_FILE to mirror "
                    "them to a file.[/dim]".format(
                        db=self._paths.database,
                        src=self._paths.database_source,
                        cfg=self._paths.config,
                        cfg_state=""
                        if self._paths.config_loaded
                        else " (not present)",
                        backends=backends,
                    )
                )
            )
        yield Footer()


class ToolhApp(App):
    """The single-screen snippet manager."""

    CSS_PATH = "styles.tcss"
    TITLE = "toolh"
    SUB_TITLE = "code snippet manager"

    BINDINGS = [
        # Only keys that must work even while a text field has focus get
        # priority; plain letters are left alone so typing stays typing.
        Binding("ctrl+q", "quit", "Quit", priority=True, show=True),
        Binding("f1", "help", "Help", priority=True, show=False),
        Binding("question_mark", "help", "Help", show=True),
        Binding("ctrl+n", "new_snippet", "New", priority=True, show=True),
        Binding("ctrl+e", "edit_snippet", "Edit", priority=True, show=True),
        Binding("f2", "edit_snippet", "Edit", priority=True, show=False),
        Binding("r", "rename_snippet", "Rename", show=True),
        Binding("d", "delete_snippet", "Delete", show=True),
        Binding("delete", "delete_snippet", "Delete", show=False),
        Binding("c", "copy_code", "Copy code", show=True),
        Binding("y", "copy_all", "Copy all", show=True),
        Binding("ctrl+d", "duplicate_snippet", "Duplicate", priority=True, show=False),
        Binding("slash", "focus_search", "Search", show=True),
        Binding("ctrl+f", "focus_search", "Search", priority=True, show=False),
        Binding("s", "cycle_sort", "Sort", show=True),
        Binding("ctrl+r", "reload", "Reload", priority=True, show=False),
        Binding("escape", "escape", "Back", priority=True, show=False),
        Binding("j", "cursor_down", "Down", show=False),
        Binding("k", "cursor_up", "Up", show=False),
        Binding("g", "cursor_first", "First", show=False),
        Binding("G", "cursor_last", "Last", show=False),
    ]

    def __init__(
        self,
        store: SnippetStore,
        paths: Paths,
        *,
        clipboard: Optional[Clipboard] = None,
    ) -> None:
        super().__init__()
        self.store = store
        self.paths = paths
        self.clipboard_service = clipboard or Clipboard(
            command=paths.clipboard_command,
            mirror_path=paths.clipboard_mirror,
            osc52_writer=self._write_escape,
        )
        self.snippets: List[Snippet] = []
        self.selected_id: Optional[int] = None
        self.sort_index = 0
        self.editing = False
        self.edit_target: Optional[int] = None  # None + editing -> creating
        self._pending_delete: Optional[int] = None
        self._status_note = ""

    # -- terminal plumbing ------------------------------------------------
    def _write_escape(self, sequence: str) -> None:
        """Send a raw escape sequence through Textual's driver (for OSC 52)."""
        driver = getattr(self, "_driver", None)
        if driver is None:
            raise RuntimeError("terminal not attached")
        driver.write(sequence)

    def check_action(self, action: str, parameters: tuple) -> Optional[bool]:
        """Silence the main key map while an overlay screen (help) is on top.

        Without this, the app-level ``escape``/``d``/``c`` bindings -- some of
        which are ``priority`` so they work from inside text fields -- would
        keep firing underneath the help screen and swallow its own keys.
        """
        if len(self.screen_stack) > 1 and action != "quit":
            return False
        return True

    # -- composition ------------------------------------------------------
    def compose(self) -> ComposeResult:
        yield Header(show_clock=False)

        with Horizontal(id="search-row"):
            yield Input(
                placeholder="Search snippets - words, tag:cli, lang:python, "
                'title:foo, "exact phrase", -exclude',
                id="search",
            )
            yield Static("", id="sort-indicator")

        with Horizontal(id="main"):
            list_pane = Vertical(id="list-pane")
            list_pane.border_title = "Snippets"
            with list_pane:
                table = DataTable(id="snippet-table", cursor_type="row", zebra_stripes=True)
                table.add_columns("#", "Title", "Language", "Tags", "Lines", "Updated")
                yield table

            right = Vertical(id="right-pane")
            right.border_title = "Detail"
            with right:
                with VerticalScroll(id="detail-view"):
                    yield Static("", id="detail-title")
                    yield Static("", id="detail-meta")
                    yield Static("", id="detail-description")
                    yield Static("", id="detail-code")
                with VerticalScroll(id="edit-view"):
                    yield Static("", id="edit-heading")
                    yield Static("Title", classes="section-label")
                    yield Input(placeholder="short, memorable name", id="f-title")
                    yield Static("Language", classes="section-label")
                    yield Input(placeholder="python, bash, sql, ...", id="f-language")
                    yield Static("Tags", classes="section-label")
                    yield Input(
                        placeholder="comma separated, e.g. cli, git", id="f-tags"
                    )
                    yield Static("Description", classes="section-label")
                    yield TextArea(id="f-description", soft_wrap=True)
                    yield Static("Code", classes="section-label")
                    yield TextArea(id="f-code", soft_wrap=True)
                    yield Static(
                        Text.from_markup(
                            "[dim]tab / shift+tab move between fields  -  "
                            "ctrl+s saves  -  escape cancels[/dim]"
                        ),
                        id="edit-hint",
                    )

        with Horizontal(id="rename-bar"):
            yield Static("Rename to:", id="rename-label")
            yield Input(id="f-rename")
        yield Static("", id="confirm-bar")
        yield Static("", id="status")
        yield Footer()

    # -- start-up ---------------------------------------------------------
    def on_mount(self) -> None:
        self.query_one("#edit-view").display = False
        self.query_one("#snippet-table", DataTable).focus()
        self.refresh_list()
        if not self.snippets and not self.query_one("#search", Input).value:
            self._status_note = (
                "Library is empty - press ctrl+n to create your first snippet, "
                "? for help."
            )
        self.update_status()

    # -- data flow --------------------------------------------------------
    @property
    def sort_key(self) -> str:
        return SORT_MODES[self.sort_index % len(SORT_MODES)][0]

    @property
    def sort_label(self) -> str:
        return SORT_MODES[self.sort_index % len(SORT_MODES)][1]

    def refresh_list(self, keep_id: Optional[int] = None) -> None:
        """Re-read the database and rebuild the list, preserving the cursor."""
        query = self.query_one("#search", Input).value
        target = keep_id if keep_id is not None else self.selected_id
        try:
            self.snippets = self.store.list_snippets(query, sort=self.sort_key)
        except StorageError as exc:
            self.snippets = []
            self.notify(str(exc), title="Database error", severity="error")

        table = self.query_one("#snippet-table", DataTable)
        table.clear()
        for snippet in self.snippets:
            tags = ", ".join(snippet.tags)
            table.add_row(
                Text(str(snippet.id), style="dim"),
                Text(snippet.title or "(untitled)", style="bold"),
                Text(snippet.language or "-", style="cyan"),
                Text(tags or "-", style="magenta"),
                Text(str(snippet.line_count), style="dim", justify="right"),
                Text((snippet.updated_at or "")[:10], style="dim"),
                key=str(snippet.id),
            )

        pane = self.query_one("#list-pane")
        total = self.store.count()
        if query:
            pane.border_title = "Snippets ({} of {} match)".format(
                len(self.snippets), total
            )
        else:
            pane.border_title = "Snippets ({})".format(total)

        row = 0
        if target is not None:
            row = next(
                (i for i, s in enumerate(self.snippets) if s.id == target), 0
            )
        if self.snippets:
            table.move_cursor(row=row)
            self.selected_id = self.snippets[row].id
        else:
            self.selected_id = None
        self.show_detail()
        self.update_status()

    def current_snippet(self) -> Optional[Snippet]:
        if self.selected_id is None:
            return None
        return next((s for s in self.snippets if s.id == self.selected_id), None)

    # -- detail rendering -------------------------------------------------
    def show_detail(self) -> None:
        """Render every field of the selected snippet into the right pane."""
        snippet = self.current_snippet()
        title_widget = self.query_one("#detail-title", Static)
        meta_widget = self.query_one("#detail-meta", Static)
        description_widget = self.query_one("#detail-description", Static)
        code_widget = self.query_one("#detail-code", Static)
        pane = self.query_one("#right-pane")

        if snippet is None:
            pane.border_title = "Detail"
            query = self.query_one("#search", Input).value
            if query:
                message = Text.from_markup(
                    "[yellow]No snippet matches[/yellow] [bold]{}[/bold]\n\n"
                    "[dim]Press escape to clear the search, or ctrl+n to create a "
                    "snippet.[/dim]\n\n{}".format(query, SEARCH_HELP)
                )
            else:
                message = Text.from_markup(
                    "[bold]No snippets yet.[/bold]\n\n"
                    "Press [bold yellow]ctrl+n[/bold yellow] to create one. Each "
                    "snippet has a title, language, tags, a description and the "
                    "code itself.\n\n"
                    "Press [bold yellow]?[/bold yellow] for the full key list."
                )
            title_widget.update(message)
            meta_widget.update("")
            description_widget.update("")
            code_widget.update("")
            return

        pane.border_title = "Detail - #{}".format(snippet.id)
        title_widget.update(
            Text.from_markup(
                "[bold cyan]Title[/bold cyan]  [bold]{}[/bold]".format(
                    _escape(snippet.title or "(untitled)")
                )
            )
        )

        meta = Table.grid(padding=(0, 2))
        meta.add_column(style="bold cyan", no_wrap=True)
        meta.add_column(overflow="fold")
        meta.add_row("Language", snippet.language or "[dim]not set[/dim]")
        meta.add_row("Tags", ", ".join(snippet.tags) or "[dim]none[/dim]")
        meta.add_row(
            "Updated",
            "[dim]{}[/dim]".format(snippet.updated_at or "-"),
        )
        meta.add_row(
            "Created",
            "[dim]{}[/dim]".format(snippet.created_at or "-"),
        )
        meta_widget.update(meta)

        description = snippet.description.strip()
        description_widget.update(
            Text.from_markup(
                "[bold cyan]Description[/bold cyan]\n{}".format(
                    _escape(description) if description else "[dim]none[/dim]"
                )
            )
        )

        if snippet.code:
            body = Syntax(
                snippet.code,
                snippet.language or "text",
                theme="ansi_dark",
                line_numbers=True,
                word_wrap=True,
                background_color="default",
            )
            heading = Text.from_markup(
                "[bold cyan]Code[/bold cyan] [dim]({} line{})[/dim]".format(
                    snippet.line_count, "" if snippet.line_count == 1 else "s"
                )
            )
            code_widget.update(_stack(heading, body))
        else:
            code_widget.update(
                Text.from_markup("[bold cyan]Code[/bold cyan]\n[dim]empty[/dim]")
            )

    def update_status(self) -> None:
        """Refresh the status line under the panes."""
        query = self.query_one("#search", Input).value
        parts = [
            "[bold]{}[/bold] snippet(s)".format(self.store.count()),
            "sort: {}".format(self.sort_label),
        ]
        if query:
            parts.append("filter: {}".format(_escape(query)))
        parts.append("db: {}".format(_escape(str(self.paths.database))))

        first = " [dim]|[/dim] ".join(parts)
        second = self._status_note or SEARCH_HELP
        self.query_one("#status", Static).update(
            Text.from_markup("{}\n[dim]{}[/dim]".format(first, _escape_keep(second)))
        )
        self.query_one("#sort-indicator", Static).update(
            Text.from_markup("[dim]sort:[/dim] {}  [dim](s)[/dim]".format(self.sort_label))
        )

    def note(self, text: str) -> None:
        self._status_note = text
        self.update_status()

    # -- list events ------------------------------------------------------
    @on(DataTable.RowHighlighted, "#snippet-table")
    def _row_highlighted(self, event: DataTable.RowHighlighted) -> None:
        if 0 <= event.cursor_row < len(self.snippets):
            self.selected_id = self.snippets[event.cursor_row].id
            if not self.editing:
                self.show_detail()

    @on(DataTable.RowSelected, "#snippet-table")
    def _row_selected(self, event: DataTable.RowSelected) -> None:
        # Enter on a row: focus the detail pane so long code can be scrolled.
        if self.snippets and not self.editing:
            self.query_one("#detail-view", VerticalScroll).focus()
            self.note("Detail pane focused - arrows scroll, tab returns to the list.")

    @on(Input.Changed, "#search")
    def _search_changed(self, event: Input.Changed) -> None:
        self._status_note = ""
        self.refresh_list()

    @on(Input.Submitted, "#search")
    def _search_submitted(self) -> None:
        if self.snippets:
            self.query_one("#snippet-table", DataTable).focus()

    # -- actions: navigation ---------------------------------------------
    def _table_focused(self) -> bool:
        return isinstance(self.focused, DataTable)

    def action_cursor_down(self) -> None:
        if self._table_focused():
            self.query_one("#snippet-table", DataTable).action_cursor_down()

    def action_cursor_up(self) -> None:
        if self._table_focused():
            self.query_one("#snippet-table", DataTable).action_cursor_up()

    def action_cursor_first(self) -> None:
        if self._table_focused() and self.snippets:
            self.query_one("#snippet-table", DataTable).move_cursor(row=0)

    def action_cursor_last(self) -> None:
        if self._table_focused() and self.snippets:
            self.query_one("#snippet-table", DataTable).move_cursor(
                row=len(self.snippets) - 1
            )

    def action_focus_search(self) -> None:
        self._close_confirm()
        self._close_rename()
        search = self.query_one("#search", Input)
        search.focus()
        self.note("Type to filter. " + SEARCH_HELP)

    def action_cycle_sort(self) -> None:
        self.sort_index = (self.sort_index + 1) % len(SORT_MODES)
        self.refresh_list()
        self.note("Sorted by {}.".format(self.sort_label))

    def action_reload(self) -> None:
        self.refresh_list()
        self.note("Reloaded from {}.".format(self.paths.database))

    def action_help(self) -> None:
        if isinstance(self.screen, HelpScreen):
            return
        self.push_screen(
            HelpScreen(self.clipboard_service.describe_backends(), self.paths)
        )

    # -- actions: editing -------------------------------------------------
    def _open_editor(self, snippet: Optional[Snippet]) -> None:
        """Show the inline editor.  ``None`` means "create a new snippet"."""
        self._close_confirm()
        self._close_rename()
        self.editing = True
        self.edit_target = snippet.id if snippet else None

        self.query_one("#f-title", Input).value = snippet.title if snippet else ""
        self.query_one("#f-language", Input).value = snippet.language if snippet else ""
        self.query_one("#f-tags", Input).value = snippet.tag_text if snippet else ""
        description = self.query_one("#f-description", TextArea)
        description.text = snippet.description if snippet else ""
        code = self.query_one("#f-code", TextArea)
        code.text = snippet.code if snippet else ""
        self._apply_code_language(snippet.language if snippet else "")

        heading = (
            "[bold]Editing #{}[/bold] [dim]{}[/dim]".format(
                snippet.id, _escape(snippet.title)
            )
            if snippet
            else "[bold]New snippet[/bold]"
        )
        self.query_one("#edit-heading", Static).update(Text.from_markup(heading))

        pane = self.query_one("#right-pane")
        pane.border_title = (
            "Edit - #{}".format(snippet.id) if snippet else "Edit - new snippet"
        )
        self.query_one("#detail-view").display = False
        self.query_one("#edit-view").display = True
        self.query_one("#f-title", Input).focus()
        self.note("ctrl+s saves, escape cancels, tab moves between fields.")

    def _apply_code_language(self, language: str) -> None:
        code = self.query_one("#f-code", TextArea)
        try:
            code.language = _textarea_language(language)
        except Exception:
            code.language = None

    def _close_editor(self) -> None:
        self.editing = False
        self.edit_target = None
        self.query_one("#edit-view").display = False
        self.query_one("#detail-view").display = True
        self.show_detail()
        self.query_one("#snippet-table", DataTable).focus()

    def action_new_snippet(self) -> None:
        if self.editing:
            self.note("Finish this snippet first (ctrl+s to save, escape to cancel).")
            return
        self._open_editor(None)

    def action_edit_snippet(self) -> None:
        if self.editing:
            return
        snippet = self.current_snippet()
        if snippet is None:
            self.notify(
                "Nothing to edit - press ctrl+n to create a snippet.",
                severity="warning",
            )
            return
        self._open_editor(snippet)

    @on(Input.Changed, "#f-language")
    def _language_changed(self, event: Input.Changed) -> None:
        # Live syntax highlighting in the code field as the language is typed.
        self._apply_code_language(event.value)

    @on(Input.Submitted, "#f-title")
    @on(Input.Submitted, "#f-language")
    @on(Input.Submitted, "#f-tags")
    def _field_submitted(self) -> None:
        self.screen.focus_next()

    def action_save(self) -> None:
        """Persist the editor contents (bound to ctrl+s, see ``on_key``)."""
        if not self.editing:
            return
        title = self.query_one("#f-title", Input).value.strip()
        if not title:
            self.notify("A title is required.", severity="error")
            self.query_one("#f-title", Input).focus()
            return

        candidate = Snippet(
            id=self.edit_target,
            title=title,
            language=self.query_one("#f-language", Input).value,
            description=self.query_one("#f-description", TextArea).text,
            code=self.query_one("#f-code", TextArea).text,
            tags=parse_tags(self.query_one("#f-tags", Input).value),
        )
        try:
            if self.edit_target is None:
                stored = self.store.create(candidate)
                message = "Created #{} '{}'.".format(stored.id, stored.title)
            else:
                stored = self.store.update(candidate)
                message = "Saved #{} '{}'.".format(stored.id, stored.title)
        except StorageError as exc:
            self.notify(str(exc), title="Could not save", severity="error")
            return

        self.editing = False
        self.edit_target = None
        self.query_one("#edit-view").display = False
        self.query_one("#detail-view").display = True
        self.selected_id = stored.id
        self.refresh_list(keep_id=stored.id)
        self.query_one("#snippet-table", DataTable).focus()
        self.notify(message, title="Saved")
        self.note(message)

    # -- actions: rename --------------------------------------------------
    def action_rename_snippet(self) -> None:
        snippet = self.current_snippet()
        if self.editing:
            self.query_one("#f-title", Input).focus()
            return
        if snippet is None:
            self.notify("Nothing to rename.", severity="warning")
            return
        self._close_confirm()
        bar = self.query_one("#rename-bar")
        bar.add_class("visible")
        field = self.query_one("#f-rename", Input)
        field.value = snippet.title
        self.query_one("#rename-label", Static).update(
            Text.from_markup("[bold]Rename #{} to:[/bold]".format(snippet.id))
        )
        field.focus()
        self.note("Type the new title, enter confirms, escape cancels.")

    def _close_rename(self) -> None:
        bar = self.query_one("#rename-bar")
        if bar.has_class("visible"):
            bar.remove_class("visible")
            if self.snippets:
                self.query_one("#snippet-table", DataTable).focus()

    @on(Input.Submitted, "#f-rename")
    def _rename_submitted(self, event: Input.Submitted) -> None:
        snippet = self.current_snippet()
        if snippet is None or snippet.id is None:
            self._close_rename()
            return
        new_title = event.value.strip()
        if not new_title:
            self.notify("A title is required.", severity="error")
            return
        try:
            stored = self.store.rename(snippet.id, new_title)
        except StorageError as exc:
            self.notify(str(exc), title="Could not rename", severity="error")
            return
        self._close_rename()
        self.refresh_list(keep_id=stored.id)
        self.notify("Renamed to '{}'.".format(stored.title), title="Renamed")
        self.note("Renamed #{} to '{}'.".format(stored.id, stored.title))

    # -- actions: duplicate ----------------------------------------------
    def action_duplicate_snippet(self) -> None:
        snippet = self.current_snippet()
        if self.editing or snippet is None or snippet.id is None:
            return
        try:
            stored = self.store.duplicate(snippet.id)
        except StorageError as exc:
            self.notify(str(exc), title="Could not duplicate", severity="error")
            return
        self.refresh_list(keep_id=stored.id)
        self.notify("Duplicated as '{}'.".format(stored.title), title="Duplicated")
        self.note("Duplicated #{} as #{}.".format(snippet.id, stored.id))

    # -- actions: delete --------------------------------------------------
    def action_delete_snippet(self) -> None:
        if self.editing:
            return
        snippet = self.current_snippet()
        if snippet is None or snippet.id is None:
            self.notify("Nothing to delete.", severity="warning")
            return
        self._close_rename()
        self._pending_delete = snippet.id
        bar = self.query_one("#confirm-bar", Static)
        bar.add_class("visible")
        bar.update(
            Text.from_markup(
                "[bold]Delete #{id} '{title}'?[/bold]  "
                "[bold yellow]y[/bold yellow] = yes, "
                "[bold yellow]n[/bold yellow] or [bold yellow]escape[/bold yellow]"
                " = keep it\n[dim]This removes it from the database "
                "permanently.[/dim]".format(
                    id=snippet.id, title=_escape(snippet.title)
                )
            )
        )
        self.note("Confirm deletion with y, cancel with n or escape.")

    def _close_confirm(self) -> None:
        self._pending_delete = None
        bar = self.query_one("#confirm-bar", Static)
        if bar.has_class("visible"):
            bar.remove_class("visible")
            bar.update("")

    def _confirm_delete(self) -> None:
        snippet_id = self._pending_delete
        self._close_confirm()
        if snippet_id is None:
            return
        snippet = next((s for s in self.snippets if s.id == snippet_id), None)
        title = snippet.title if snippet else "#{}".format(snippet_id)
        try:
            deleted = self.store.delete(snippet_id)
        except StorageError as exc:
            self.notify(str(exc), title="Could not delete", severity="error")
            return
        if not deleted:
            self.notify("Snippet was already gone.", severity="warning")
        else:
            self.notify("Deleted '{}'.".format(title), title="Deleted")
            self.note("Deleted '{}' from {}.".format(title, self.paths.database))
        self.selected_id = None
        self.refresh_list()
        if self.snippets:
            self.query_one("#snippet-table", DataTable).focus()

    # -- actions: clipboard ----------------------------------------------
    def _copy(self, text: str, what: str) -> None:
        result = self.clipboard_service.copy(text)
        if result.ok:
            self.notify(
                "{} copied ({}).".format(what, result.backend), title="Clipboard"
            )
        else:
            self.notify(result.message(), title="Clipboard", severity="warning")
        # Textual's own clipboard call adds OSC 52 support for terminals that
        # want it; harmless when a native backend already succeeded.
        try:
            self.copy_to_clipboard(text)
        except Exception:
            pass
        self.note(result.message())

    def action_copy_code(self) -> None:
        snippet = self.current_snippet()
        if self.editing or snippet is None:
            return
        if not snippet.code:
            self.notify("This snippet has no code to copy.", severity="warning")
            return
        self._copy(snippet.code, "Code of '{}'".format(snippet.title))

    def action_copy_all(self) -> None:
        snippet = self.current_snippet()
        if self.editing or snippet is None:
            return
        payload = (
            "Title: {}\nLanguage: {}\nTags: {}\nDescription: {}\n\n{}".format(
                snippet.title,
                snippet.language,
                snippet.tag_text,
                snippet.description,
                snippet.code,
            )
        )
        self._copy(payload, "Snippet '{}'".format(snippet.title))

    # -- escape / key routing --------------------------------------------
    def action_escape(self) -> None:
        if self._pending_delete is not None:
            self._close_confirm()
            self.note("Deletion cancelled.")
            return
        if self.query_one("#rename-bar").has_class("visible"):
            self._close_rename()
            self.note("Rename cancelled.")
            return
        if self.editing:
            self._close_editor()
            self.note("Edit cancelled - nothing was written to the database.")
            return
        search = self.query_one("#search", Input)
        if search.value:
            search.value = ""
            self.refresh_list()
            self.note("Search cleared.")
            self.query_one("#snippet-table", DataTable).focus()
            return
        if not self._table_focused() and self.snippets:
            self.query_one("#snippet-table", DataTable).focus()
            return
        self.note("Press ctrl+q to quit, ? for help.")

    def on_key(self, event: events.Key) -> None:
        """Handle keys that depend on what is currently on screen."""
        if len(self.screen_stack) > 1:
            return  # an overlay screen (help) owns the keyboard

        # ctrl+s saves from any field in the editor.
        if event.key == "ctrl+s" and self.editing:
            event.stop()
            event.prevent_default()
            self.action_save()
            return

        if self._pending_delete is not None:
            if event.key in ("y", "Y"):
                event.stop()
                event.prevent_default()
                self._confirm_delete()
                return
            if event.key in ("n", "N"):
                event.stop()
                event.prevent_default()
                self._close_confirm()
                self.note("Deletion cancelled.")
                return


def _escape(text: str) -> str:
    """Escape Rich markup so snippet content is shown verbatim."""
    return (text or "").replace("[", "\\[")


def _escape_keep(text: str) -> str:
    """Escape markup in status text that we build ourselves."""
    return (text or "").replace("[", "\\[")


def _stack(*renderables: object) -> Table:
    """Stack renderables vertically inside one Static."""
    grid = Table.grid(expand=True)
    grid.add_column(overflow="fold")
    for renderable in renderables:
        grid.add_row(renderable)
    return grid
