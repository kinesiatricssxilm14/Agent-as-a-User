"""Configuration and database-path resolution.

The persistence path is decided by the following priority (highest first):

1. ``--db PATH`` command-line argument
2. ``TOOLH_DB`` environment variable (full path to the database file)
3. ``database`` key in ``config.toml`` (full path to the database file)
4. ``TOOLH_DATA_DIR`` environment variable or ``data_dir`` key in
   ``config.toml`` (a directory; the file is ``snippets.db`` inside it)
5. The XDG data directory default: ``~/.local/share/toolh/snippets.db``

The config file lives at ``~/.config/toolh/config.toml`` (or under
``$XDG_CONFIG_HOME`` when set).
"""

from __future__ import annotations

import os
import tomllib
from pathlib import Path

APP_DIR_NAME = "toolh"
DEFAULT_DB_FILENAME = "snippets.db"


def _xdg_config_home() -> Path:
    base = os.environ.get("XDG_CONFIG_HOME")
    if base:
        return Path(base).expanduser()
    return Path.home() / ".config"


def _xdg_data_home() -> Path:
    base = os.environ.get("XDG_DATA_HOME")
    if base:
        return Path(base).expanduser()
    return Path.home() / ".local" / "share"


def config_file_path() -> Path:
    """Return the path to the toolh config file."""
    return _xdg_config_home() / APP_DIR_NAME / "config.toml"


def load_config() -> dict:
    """Load ``config.toml`` if it exists.

    Any parse error or missing file results in an empty ``dict`` so a broken
    config can never stop the tool from starting.
    """
    path = config_file_path()
    if not path.is_file():
        return {}
    try:
        with path.open("rb") as handle:
            data = tomllib.load(handle)
    except Exception:
        return {}
    return data if isinstance(data, dict) else {}


def resolve_db_path(cli_path: str | None = None) -> str:
    """Resolve the SQLite database path according to the documented priority."""
    if cli_path:
        return str(Path(cli_path).expanduser())

    env_db = os.environ.get("TOOLH_DB")
    if env_db:
        return str(Path(env_db).expanduser())

    cfg = load_config()
    if cfg.get("database"):
        return str(Path(str(cfg["database"])).expanduser())

    data_dir = os.environ.get("TOOLH_DATA_DIR")
    if not data_dir and cfg.get("data_dir"):
        data_dir = str(cfg["data_dir"])
    if data_dir:
        return str(Path(data_dir).expanduser() / DEFAULT_DB_FILENAME)

    return str(_xdg_data_home() / APP_DIR_NAME / DEFAULT_DB_FILENAME)
