"""The ``toolh`` console entry point.

``toolh`` with no arguments launches the TUI against the resolved database,
which is the documented default.  The extra flags exist so the same code can be
scripted or inspected without a terminal (handy in a container).
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import List, Optional

from . import __version__
from .clipboard import Clipboard
from .config import (
    ENV_CLIPBOARD_COMMAND,
    ENV_CLIPBOARD_MIRROR,
    ENV_CONFIG,
    ENV_DB,
    ENV_HOME,
    resolve_paths,
)
from .models import Snippet, parse_tags
from .storage import SnippetStore, StorageError

__all__ = ["main", "build_parser"]

_EPILOG = """\
environment variables:
  {db}        path of the SQLite database (highest priority after --db)
  {home}      base directory for toolh state ({db} wins if both are set)
  {config}    path of the JSON configuration file
  {clip}   shell command that receives copied text on stdin
  {mirror}      file that mirrors whatever was last copied

Run `toolh` with no arguments to open the interface, then press ? for the
full key map.
""".format(
    db=ENV_DB.ljust(10),
    home=ENV_HOME.ljust(10),
    config=ENV_CONFIG.ljust(10),
    clip=ENV_CLIPBOARD_COMMAND,
    mirror=ENV_CLIPBOARD_MIRROR.ljust(10),
)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="toolh",
        description="toolh - a keyboard-driven TUI for managing code snippets.",
        epilog=_EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--db",
        metavar="PATH",
        help="SQLite database to use (overrides {} and the config file)".format(ENV_DB),
    )
    parser.add_argument(
        "--version", action="version", version="toolh {}".format(__version__)
    )
    parser.add_argument(
        "--where",
        action="store_true",
        help="print the resolved paths and exit",
    )
    parser.add_argument(
        "--list",
        dest="list_snippets",
        action="store_true",
        help="print stored snippets as JSON and exit (no terminal needed)",
    )
    parser.add_argument(
        "--search",
        metavar="QUERY",
        help="restrict --list to snippets matching QUERY",
    )
    parser.add_argument(
        "--add",
        metavar="TITLE",
        help="create a snippet non-interactively and exit",
    )
    parser.add_argument("--language", metavar="LANG", default="", help="language for --add")
    parser.add_argument("--tags", metavar="TAGS", default="", help="comma separated tags for --add")
    parser.add_argument("--description", metavar="TEXT", default="", help="description for --add")
    parser.add_argument(
        "--code",
        metavar="TEXT",
        help="code body for --add (use - to read stdin)",
    )
    return parser


def _open_store(args: argparse.Namespace):
    paths = resolve_paths(cli_database=args.db)
    paths.ensure_parent()
    return SnippetStore(paths.database), paths


def main(argv: Optional[List[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        store, paths = _open_store(args)
    except StorageError as exc:
        print("toolh: {}".format(exc), file=sys.stderr)
        return 1

    try:
        if args.where:
            clipboard = Clipboard(
                command=paths.clipboard_command, mirror_path=paths.clipboard_mirror
            )
            print("database:          {}".format(paths.database))
            print("database source:   {}".format(paths.database_source))
            print(
                "config:            {}{}".format(
                    paths.config, "" if paths.config_loaded else " (not present)"
                )
            )
            print("clipboard command: {}".format(paths.clipboard_command or "(auto)"))
            print("clipboard mirror:  {}".format(paths.clipboard_mirror or "(none)"))
            print("clipboard backends: {}".format(", ".join(clipboard.describe_backends())))
            print("snippets:          {}".format(store.count()))
            return 0

        if args.add:
            code = args.code
            if code == "-":
                code = sys.stdin.read()
            snippet = Snippet(
                title=args.add,
                language=args.language,
                description=args.description,
                code=code or "",
                tags=parse_tags(args.tags),
            )
            stored = store.create(snippet)
            print("created #{}: {}".format(stored.id, stored.title))
            return 0

        if args.list_snippets or args.search:
            snippets = store.list_snippets(args.search)
            json.dump([s.to_dict() for s in snippets], sys.stdout, indent=2)
            sys.stdout.write("\n")
            return 0

        # Default: launch the interface.
        from .app import ToolhApp

        app = ToolhApp(store, paths)
        app.run()
        return app.return_code or 0
    except StorageError as exc:
        print("toolh: {}".format(exc), file=sys.stderr)
        return 1
    except KeyboardInterrupt:  # pragma: no cover - interactive
        return 130
    finally:
        store.close()


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
