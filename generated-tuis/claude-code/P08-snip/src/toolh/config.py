"""Resolution of the paths toolh uses on disk.

The storage location is *implementation defined* but overridable, which is what
this module is about.  Precedence, highest first:

1. an explicit path passed on the command line (``toolh --db PATH``)
2. the ``TOOLH_DB`` environment variable
3. the ``database`` key of the configuration file
4. ``<TOOLH_HOME>/snippets.db``
5. ``<XDG_DATA_HOME>/toolh/snippets.db``
6. ``~/.local/share/toolh/snippets.db``

The configuration file itself is found at ``TOOLH_CONFIG``, else
``<TOOLH_HOME>/config.json``, else ``<XDG_CONFIG_HOME>/toolh/config.json``,
else ``~/.config/toolh/config.json``.  It is plain JSON and every key is
optional::

    {
      "database": "/data/snippets.db",
      "clipboard_command": "xclip -selection clipboard",
      "clipboard_mirror": "/tmp/toolh-clipboard.txt"
    }

Nothing here touches the filesystem except :func:`load_config` (which only
reads) and :meth:`Paths.ensure_parent`.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, Mapping, Optional

__all__ = [
    "APP_NAME",
    "ENV_DB",
    "ENV_HOME",
    "ENV_CONFIG",
    "ENV_CLIPBOARD_COMMAND",
    "ENV_CLIPBOARD_MIRROR",
    "Paths",
    "config_path",
    "load_config",
    "resolve_paths",
]

APP_NAME = "toolh"

#: Absolute (or ``~``-relative) path of the SQLite database.
ENV_DB = "TOOLH_DB"
#: Base directory for all toolh state; used when ``TOOLH_DB`` is unset.
ENV_HOME = "TOOLH_HOME"
#: Location of the JSON configuration file.
ENV_CONFIG = "TOOLH_CONFIG"
#: Shell command that receives the clipboard payload on stdin.
ENV_CLIPBOARD_COMMAND = "TOOLH_CLIPBOARD_COMMAND"
#: File that mirrors whatever was last copied (useful when headless).
ENV_CLIPBOARD_MIRROR = "TOOLH_CLIPBOARD_FILE"

DB_FILENAME = "snippets.db"
CONFIG_FILENAME = "config.json"


def _expand(value: str) -> Path:
    """Expand ``~`` and ``$VAR`` then make the result absolute."""
    return Path(os.path.expanduser(os.path.expandvars(str(value)))).absolute()


def _clean(value: Optional[object]) -> Optional[str]:
    """Return ``value`` as a non-empty stripped string, else ``None``."""
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def _xdg_dir(env_var: str, default: str, environ: Mapping[str, str]) -> Path:
    base = _clean(environ.get(env_var))
    if base:
        return _expand(base)
    return Path(os.path.expanduser(default)).absolute()


def config_path(environ: Optional[Mapping[str, str]] = None) -> Path:
    """Return the configuration file location (it need not exist)."""
    environ = os.environ if environ is None else environ

    explicit = _clean(environ.get(ENV_CONFIG))
    if explicit:
        return _expand(explicit)

    home = _clean(environ.get(ENV_HOME))
    if home:
        return _expand(home) / CONFIG_FILENAME

    return _xdg_dir("XDG_CONFIG_HOME", "~/.config", environ) / APP_NAME / CONFIG_FILENAME


def load_config(
    environ: Optional[Mapping[str, str]] = None,
) -> Dict[str, Any]:
    """Read the JSON configuration file, tolerating absence.

    A malformed or non-object file is ignored rather than fatal: losing the
    whole tool because a config file has a stray comma would be unkind.  The
    caller can surface :func:`config_error` to explain the fallback.
    """
    path = config_path(environ)
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return {}
    try:
        data = json.loads(raw)
    except ValueError:
        return {}
    if not isinstance(data, dict):
        return {}
    return data


@dataclass
class Paths:
    """Everywhere toolh may read or write, plus how each was decided."""

    database: Path
    config: Path
    clipboard_mirror: Optional[Path] = None
    clipboard_command: Optional[str] = None
    #: Human readable provenance of :attr:`database`, shown in the UI.
    database_source: str = "default"
    config_loaded: bool = False
    extra: Dict[str, Any] = field(default_factory=dict)

    def ensure_parent(self) -> None:
        """Create the database's parent directory when missing."""
        parent = self.database.parent
        if str(parent):
            parent.mkdir(parents=True, exist_ok=True)


def resolve_paths(
    cli_database: Optional[str] = None,
    environ: Optional[Mapping[str, str]] = None,
) -> Paths:
    """Work out the effective paths from CLI flags, environment and config."""
    environ = os.environ if environ is None else environ
    cfg = load_config(environ)
    cfg_path = config_path(environ)

    database: Optional[Path] = None
    source = "default"

    cli_value = _clean(cli_database)
    env_value = _clean(environ.get(ENV_DB))
    cfg_value = _clean(cfg.get("database"))
    home_value = _clean(environ.get(ENV_HOME))

    if cli_value:
        database, source = _expand(cli_value), "--db command line option"
    elif env_value:
        database, source = _expand(env_value), "{} environment variable".format(ENV_DB)
    elif cfg_value:
        database, source = _expand(cfg_value), "database key in {}".format(cfg_path)
    elif home_value:
        database = _expand(home_value) / DB_FILENAME
        source = "{} environment variable".format(ENV_HOME)
    else:
        base = _xdg_dir("XDG_DATA_HOME", "~/.local/share", environ)
        database = base / APP_NAME / DB_FILENAME
        source = "default data directory"

    # A directory (existing or trailing-slash) means "put the default file in
    # here", which is what people usually mean when they point at a folder.
    if database.is_dir() or str(cli_database or env_value or "").endswith(os.sep):
        database = database / DB_FILENAME

    mirror = (
        _clean(environ.get(ENV_CLIPBOARD_MIRROR))
        or _clean(cfg.get("clipboard_mirror"))
    )
    command = (
        _clean(environ.get(ENV_CLIPBOARD_COMMAND))
        or _clean(cfg.get("clipboard_command"))
    )

    return Paths(
        database=database,
        config=cfg_path,
        clipboard_mirror=_expand(mirror) if mirror else None,
        clipboard_command=command,
        database_source=source,
        config_loaded=cfg_path.is_file(),
        extra=cfg,
    )
