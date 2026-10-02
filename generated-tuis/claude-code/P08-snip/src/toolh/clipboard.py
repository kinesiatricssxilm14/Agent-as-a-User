"""Real clipboard integration.

A snippet manager whose "copy" button does nothing useful is worthless, but the
one environment guaranteed by the task (a Debian slim container, running
headless) has no X display and therefore no system clipboard at all.  So this
module tries a chain of *real* mechanisms and honestly reports which one worked:

1. ``TOOLH_CLIPBOARD_COMMAND`` / ``clipboard_command`` -- an explicit shell
   command that receives the payload on stdin.  Always wins when set.
2. A platform clipboard binary that is actually present and actually succeeds:
   ``pbcopy`` (macOS), ``wl-copy`` (Wayland), ``xclip``/``xsel`` (X11),
   ``clip.exe``/``powershell`` (WSL).
3. ``pyperclip``, if the optional dependency is installed and working.
4. OSC 52, an ANSI escape sequence that asks the *terminal emulator* to put the
   text on the clipboard of whatever machine the human is sitting at.  This is
   the one that survives ssh and Docker, so it is a genuine last resort rather
   than a fake one.

Independently of the above, the payload is written to a mirror file when one is
configured (``TOOLH_CLIPBOARD_FILE``), which makes "did copy work?" answerable
in a headless container.  The mirror is a *record*, never a substitute: if no
real backend worked, :attr:`CopyResult.ok` is ``False`` and the UI says so.
"""

from __future__ import annotations

import base64
import importlib.util
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, List, Optional, Sequence, Tuple

__all__ = ["CopyResult", "Clipboard", "osc52_sequence"]

_TIMEOUT = 5.0


@dataclass
class CopyResult:
    """Outcome of a copy attempt."""

    ok: bool
    backend: str
    detail: str = ""
    mirrored: Optional[Path] = None
    attempts: Sequence[str] = ()

    def message(self) -> str:
        """A short sentence suitable for a toast notification."""
        if self.ok:
            text = "Copied to clipboard via {}".format(self.backend)
            if self.mirrored:
                text += " (mirrored to {})".format(self.mirrored)
            return text
        text = "Could not reach a system clipboard"
        if self.detail:
            text += ": {}".format(self.detail)
        if self.mirrored:
            text += ". Text written to {}".format(self.mirrored)
        return text


def osc52_sequence(text: str) -> str:
    """Build the OSC 52 escape sequence that sets the terminal clipboard."""
    payload = base64.b64encode(text.encode("utf-8")).decode("ascii")
    return "\x1b]52;c;{}\x07".format(payload)


#: ``(backend name, argv)`` candidates in priority order per platform.
def _platform_commands() -> List[Tuple[str, List[str]]]:
    candidates: List[Tuple[str, List[str]]] = []
    if sys.platform == "darwin":
        candidates.append(("pbcopy", ["pbcopy"]))
    elif sys.platform == "win32":  # pragma: no cover - not the target platform
        candidates.append(("clip", ["clip"]))
    else:
        if os.environ.get("WAYLAND_DISPLAY"):
            candidates.append(("wl-copy", ["wl-copy"]))
        candidates.append(("xclip", ["xclip", "-selection", "clipboard"]))
        candidates.append(("xsel", ["xsel", "--clipboard", "--input"]))
        if not os.environ.get("WAYLAND_DISPLAY"):
            candidates.append(("wl-copy", ["wl-copy"]))
        # WSL: the Windows clipboard is reachable through interop binaries.
        candidates.append(("clip.exe", ["clip.exe"]))
    return candidates


class Clipboard:
    """Copy text to the clipboard, trying real backends in order.

    ``osc52_writer`` lets the Textual app hand over its own escape-sequence
    writer (Textual owns the terminal, so writing to ``sys.stdout`` directly
    from here would corrupt the display).
    """

    def __init__(
        self,
        *,
        command: Optional[str] = None,
        mirror_path: Optional[Path] = None,
        osc52_writer: Optional[Callable[[str], None]] = None,
        allow_osc52: bool = True,
    ) -> None:
        self.command = command
        self.mirror_path = Path(mirror_path) if mirror_path else None
        self.osc52_writer = osc52_writer
        self.allow_osc52 = allow_osc52

    # -- individual backends ---------------------------------------------
    def _run(self, argv: Sequence[str], text: str) -> Tuple[bool, str]:
        """Feed ``text`` to ``argv`` on stdin.  Returns ``(ok, detail)``."""
        try:
            proc = subprocess.run(
                list(argv),
                input=text.encode("utf-8"),
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=_TIMEOUT,
            )
        except FileNotFoundError:
            return False, "{} not installed".format(argv[0])
        except subprocess.TimeoutExpired:
            return False, "{} timed out".format(argv[0])
        except OSError as exc:
            return False, "{}: {}".format(argv[0], exc)
        if proc.returncode != 0:
            detail = (proc.stderr or b"").decode("utf-8", "replace").strip()
            return False, detail or "{} exited {}".format(argv[0], proc.returncode)
        return True, ""

    def _try_shell_command(self, text: str) -> Tuple[bool, str]:
        assert self.command
        try:
            proc = subprocess.run(
                self.command,
                shell=True,
                input=text.encode("utf-8"),
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=_TIMEOUT,
            )
        except subprocess.TimeoutExpired:
            return False, "configured command timed out"
        except OSError as exc:
            return False, "configured command failed: {}".format(exc)
        if proc.returncode != 0:
            detail = (proc.stderr or b"").decode("utf-8", "replace").strip()
            return False, detail or "command exited {}".format(proc.returncode)
        return True, ""

    def _try_pyperclip(self, text: str) -> Tuple[bool, str]:
        try:
            import pyperclip  # type: ignore
        except Exception:
            return False, "pyperclip not installed"
        try:
            pyperclip.copy(text)
        except Exception as exc:  # pyperclip raises its own exception types
            return False, "pyperclip: {}".format(exc)
        return True, ""

    def _try_osc52(self, text: str) -> Tuple[bool, str]:
        sequence = osc52_sequence(text)
        try:
            if self.osc52_writer is not None:
                self.osc52_writer(sequence)
            else:
                if not sys.stdout.isatty():
                    return False, "stdout is not a terminal"
                sys.stdout.write(sequence)
                sys.stdout.flush()
        except Exception as exc:
            return False, "OSC 52: {}".format(exc)
        return True, ""

    # -- mirror ----------------------------------------------------------
    def _write_mirror(self, text: str) -> Tuple[Optional[Path], Optional[str]]:
        if self.mirror_path is None:
            return None, None
        try:
            parent = self.mirror_path.parent
            if str(parent):
                parent.mkdir(parents=True, exist_ok=True)
            self.mirror_path.write_text(text, encoding="utf-8")
        except OSError as exc:
            return None, "mirror file: {}".format(exc)
        return self.mirror_path, None

    # -- public API ------------------------------------------------------
    def describe_backends(self) -> List[str]:
        """Names of the backends that look available, for the help screen."""
        names: List[str] = []
        if self.command:
            names.append("configured command")
        for name, argv in _platform_commands():
            if shutil.which(argv[0]):
                names.append(name)
        try:
            if importlib.util.find_spec("pyperclip") is not None:
                names.append("pyperclip")
        except Exception:
            pass
        if self.allow_osc52:
            names.append("OSC 52 terminal escape")
        # Preserve order while removing duplicates (xclip listed once).
        seen, unique = set(), []
        for name in names:
            if name not in seen:
                seen.add(name)
                unique.append(name)
        return unique

    def copy(self, text: str) -> CopyResult:
        """Copy ``text``, returning which backend succeeded."""
        text = "" if text is None else str(text)
        mirrored, mirror_error = self._write_mirror(text)
        attempts: List[str] = []

        backends: List[Tuple[str, Callable[[str], Tuple[bool, str]]]] = []
        if self.command:
            backends.append(("configured command", self._try_shell_command))
        for name, argv in _platform_commands():
            if shutil.which(argv[0]):
                backends.append((name, lambda t, a=argv: self._run(a, t)))
        backends.append(("pyperclip", self._try_pyperclip))
        if self.allow_osc52:
            backends.append(("OSC 52", self._try_osc52))

        last_detail = ""
        for name, handler in backends:
            ok, detail = handler(text)
            if ok:
                return CopyResult(
                    True, name, mirrored=mirrored, attempts=tuple(attempts)
                )
            attempts.append("{} ({})".format(name, detail) if detail else name)
            last_detail = detail or last_detail

        detail = last_detail
        if mirror_error:
            detail = "{}; {}".format(detail, mirror_error) if detail else mirror_error
        return CopyResult(
            False, "none", detail=detail, mirrored=mirrored, attempts=tuple(attempts)
        )
