"""Command-line entry point for toolh."""

from __future__ import annotations

import argparse

from . import __version__
from .config import resolve_db_path


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="toolh",
        description="toolh — a keyboard-first code snippet management TUI.",
    )
    parser.add_argument(
        "--db",
        metavar="PATH",
        help="Path to the SQLite database file (overrides config/env).",
    )
    parser.add_argument(
        "--print-db",
        action="store_true",
        help="Print the resolved database path and exit.",
    )
    parser.add_argument(
        "-V", "--version", action="store_true", help="Show the version and exit."
    )
    return parser


def main(argv: list[str] | None = None) -> None:
    args = build_parser().parse_args(argv)

    if args.version:
        print(f"toolh {__version__}")
        return

    db_path = resolve_db_path(args.db)

    if args.print_db:
        print(db_path)
        return

    # Import lazily so --version/--print-db work without a terminal.
    from .app import ToolhApp

    ToolhApp(db_path=db_path).run()
