"""Path resolution: the storage location must be overridable."""

from __future__ import annotations

import json
from pathlib import Path

from toolh.config import (
    ENV_CLIPBOARD_COMMAND,
    ENV_CLIPBOARD_MIRROR,
    ENV_CONFIG,
    ENV_DB,
    ENV_HOME,
    config_path,
    load_config,
    resolve_paths,
)


def test_default_location_follows_xdg(tmp_path: Path):
    env = {"XDG_DATA_HOME": str(tmp_path / "data")}
    paths = resolve_paths(environ=env)
    assert paths.database == tmp_path / "data" / "toolh" / "snippets.db"


def test_home_fallback_without_xdg(tmp_path: Path, monkeypatch):
    monkeypatch.setattr(Path, "home", classmethod(lambda cls: tmp_path))
    monkeypatch.setenv("HOME", str(tmp_path))
    paths = resolve_paths(environ={"HOME": str(tmp_path)})
    assert paths.database == tmp_path / ".local" / "share" / "toolh" / "snippets.db"


def test_env_var_overrides_default(tmp_path: Path):
    target = tmp_path / "custom" / "mine.db"
    paths = resolve_paths(environ={ENV_DB: str(target)})
    assert paths.database == target
    assert ENV_DB in paths.database_source


def test_toolh_home_override(tmp_path: Path):
    paths = resolve_paths(environ={ENV_HOME: str(tmp_path / "state")})
    assert paths.database == tmp_path / "state" / "snippets.db"


def test_cli_flag_beats_environment(tmp_path: Path):
    chosen = tmp_path / "cli.db"
    paths = resolve_paths(
        cli_database=str(chosen), environ={ENV_DB: str(tmp_path / "env.db")}
    )
    assert paths.database == chosen
    assert "--db" in paths.database_source


def test_config_file_override(tmp_path: Path):
    config = tmp_path / "config.json"
    wanted = tmp_path / "from-config.db"
    config.write_text(
        json.dumps(
            {
                "database": str(wanted),
                "clipboard_command": "my-copy",
                "clipboard_mirror": str(tmp_path / "mirror.txt"),
            }
        ),
        encoding="utf-8",
    )
    paths = resolve_paths(environ={ENV_CONFIG: str(config)})
    assert paths.database == wanted
    assert paths.clipboard_command == "my-copy"
    assert paths.clipboard_mirror == tmp_path / "mirror.txt"
    assert paths.config_loaded is True


def test_env_var_beats_config_file(tmp_path: Path):
    config = tmp_path / "config.json"
    config.write_text(json.dumps({"database": str(tmp_path / "cfg.db")}), encoding="utf-8")
    env_db = tmp_path / "env.db"
    paths = resolve_paths(environ={ENV_CONFIG: str(config), ENV_DB: str(env_db)})
    assert paths.database == env_db


def test_clipboard_env_beats_config(tmp_path: Path):
    config = tmp_path / "config.json"
    config.write_text(json.dumps({"clipboard_command": "from-config"}), encoding="utf-8")
    paths = resolve_paths(
        environ={
            ENV_CONFIG: str(config),
            ENV_CLIPBOARD_COMMAND: "from-env",
            ENV_CLIPBOARD_MIRROR: str(tmp_path / "m.txt"),
        }
    )
    assert paths.clipboard_command == "from-env"
    assert paths.clipboard_mirror == tmp_path / "m.txt"


def test_broken_config_is_ignored_not_fatal(tmp_path: Path):
    config = tmp_path / "config.json"
    config.write_text("{not valid json,,,", encoding="utf-8")
    assert load_config({ENV_CONFIG: str(config)}) == {}
    paths = resolve_paths(environ={ENV_CONFIG: str(config), "XDG_DATA_HOME": str(tmp_path)})
    assert paths.database == tmp_path / "toolh" / "snippets.db"


def test_missing_config_is_ignored(tmp_path: Path):
    assert load_config({ENV_CONFIG: str(tmp_path / "nope.json")}) == {}


def test_tilde_and_variables_are_expanded(tmp_path: Path, monkeypatch):
    monkeypatch.setenv("HOME", str(tmp_path))
    monkeypatch.setenv("MY_DIR", str(tmp_path / "expanded"))
    paths = resolve_paths(environ={ENV_DB: "$MY_DIR/db.sqlite", "HOME": str(tmp_path)})
    assert paths.database == tmp_path / "expanded" / "db.sqlite"


def test_directory_target_gets_default_filename(tmp_path: Path):
    directory = tmp_path / "existing-dir"
    directory.mkdir()
    paths = resolve_paths(environ={ENV_DB: str(directory)})
    assert paths.database == directory / "snippets.db"


def test_config_path_precedence(tmp_path: Path):
    assert config_path({ENV_CONFIG: str(tmp_path / "x.json")}) == tmp_path / "x.json"
    assert (
        config_path({ENV_HOME: str(tmp_path / "home")})
        == tmp_path / "home" / "config.json"
    )
    assert (
        config_path({"XDG_CONFIG_HOME": str(tmp_path / "cfg")})
        == tmp_path / "cfg" / "toolh" / "config.json"
    )


def test_ensure_parent_creates_directory(tmp_path: Path):
    paths = resolve_paths(environ={ENV_DB: str(tmp_path / "deep" / "nest" / "db.sqlite")})
    assert not paths.database.parent.exists()
    paths.ensure_parent()
    assert paths.database.parent.is_dir()
