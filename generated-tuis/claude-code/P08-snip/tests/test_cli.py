"""The ``toolh`` command line entry point."""

from __future__ import annotations

import json
import os
import shutil
import sqlite3
import subprocess
import sys
from pathlib import Path

import pytest

from toolh.cli import main
from toolh.config import ENV_DB


def test_where_reports_the_resolved_database(tmp_path: Path, monkeypatch, capsys):
    target = tmp_path / "explicit.db"
    monkeypatch.setenv(ENV_DB, str(target))
    assert main(["--where"]) == 0
    out = capsys.readouterr().out
    assert str(target) in out
    assert ENV_DB in out


def test_cli_flag_wins_over_environment(tmp_path: Path, monkeypatch, capsys):
    monkeypatch.setenv(ENV_DB, str(tmp_path / "env.db"))
    chosen = tmp_path / "flag.db"
    assert main(["--db", str(chosen), "--where"]) == 0
    assert str(chosen) in capsys.readouterr().out


def test_add_then_list_round_trip(tmp_path: Path, monkeypatch, capsys):
    db = tmp_path / "cli.db"
    monkeypatch.setenv(ENV_DB, str(db))

    assert (
        main(
            [
                "--add",
                "hello-world",
                "--language",
                "python",
                "--tags",
                "demo, cli",
                "--description",
                "Print a greeting message",
                "--code",
                'print("hello")',
            ]
        )
        == 0
    )
    capsys.readouterr()

    assert main(["--list"]) == 0
    payload = json.loads(capsys.readouterr().out)
    assert len(payload) == 1
    record = payload[0]
    assert record["title"] == "hello-world"
    assert record["language"] == "python"
    assert record["tags"] == ["cli", "demo"]
    assert record["description"] == "Print a greeting message"
    assert record["code"] == 'print("hello")'

    # ...and it is genuinely in the SQLite file.
    with sqlite3.connect(str(db)) as conn:
        assert conn.execute("SELECT COUNT(*) FROM snippets").fetchone()[0] == 1


def test_search_flag_filters(tmp_path: Path, monkeypatch, capsys):
    monkeypatch.setenv(ENV_DB, str(tmp_path / "cli.db"))
    main(["--add", "one", "--language", "python", "--code", "a"])
    main(["--add", "two", "--language", "bash", "--code", "b"])
    capsys.readouterr()

    assert main(["--search", "lang:bash"]) == 0
    payload = json.loads(capsys.readouterr().out)
    assert [record["title"] for record in payload] == ["two"]


def test_add_reads_code_from_stdin(tmp_path: Path, monkeypatch, capsys):
    monkeypatch.setenv(ENV_DB, str(tmp_path / "cli.db"))
    monkeypatch.setattr("sys.stdin", __import__("io").StringIO("from stdin\nline 2\n"))
    assert main(["--add", "piped", "--code", "-"]) == 0
    capsys.readouterr()
    main(["--list"])
    payload = json.loads(capsys.readouterr().out)
    assert payload[0]["code"] == "from stdin\nline 2\n"


def test_add_without_title_is_rejected(tmp_path: Path, monkeypatch, capsys):
    monkeypatch.setenv(ENV_DB, str(tmp_path / "cli.db"))
    # argparse requires a value for --add; an all-space title is a storage error.
    assert main(["--add", "   "]) == 1
    assert "Title is required" in capsys.readouterr().err


def test_database_directory_is_created_on_demand(tmp_path: Path, monkeypatch, capsys):
    db = tmp_path / "deep" / "nested" / "snippets.db"
    monkeypatch.setenv(ENV_DB, str(db))
    assert main(["--add", "auto-dir"]) == 0
    assert db.exists()


def test_version_flag(capsys):
    try:
        main(["--version"])
    except SystemExit as exc:
        assert exc.code == 0
    assert "toolh" in capsys.readouterr().out


def _installed_toolh() -> "str | None":
    """Locate the ``toolh`` installed alongside the running interpreter.

    Preferring ``sys.executable``'s directory matters because an unrelated
    binary of the same name may exist elsewhere on PATH; we want to test *our*
    console script, not whatever else is called toolh.
    """
    candidate = Path(sys.executable).parent / "toolh"
    if candidate.exists():
        return str(candidate)
    found = shutil.which("toolh")
    if found is None:
        return None
    # Only trust a PATH hit if it really is this package's entry point.
    try:
        probe = subprocess.run(
            [found, "--version"], capture_output=True, text=True, timeout=60
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return found if "toolh" in probe.stdout else None


def test_installed_command_is_on_the_path(tmp_path: Path):
    """The documented launch command must exist as a real executable."""
    executable = _installed_toolh()
    if executable is None:
        pytest.skip("the toolh console script is not installed in this environment")

    db = tmp_path / "subprocess.db"
    env = dict(os.environ)
    env[ENV_DB] = str(db)
    result = subprocess.run(
        [executable, "--where"],
        capture_output=True,
        text=True,
        env=env,
        timeout=60,
    )
    assert result.returncode == 0, result.stderr
    assert str(db) in result.stdout
