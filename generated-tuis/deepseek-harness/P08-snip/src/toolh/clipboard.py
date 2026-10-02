"""Clipboard access using real underlying tools.

We never simulate a copy: each attempt actually invokes a real clipboard
program (or the ``pyperclip`` wrapper around them). When no clipboard is
available — common in a headless container — we report that clearly instead
of pretending to succeed.
"""

from __future__ import annotations

import shutil
import subprocess


def copy_to_clipboard(text: str) -> tuple[bool, str]:
    """Copy ``text`` to the system clipboard.

    Returns a ``(success, message)`` tuple. Tries, in order: ``pyperclip``,
    ``xclip``, ``xsel``, ``wl-copy``, ``pbcopy`` and ``clip.exe``.
    """
    # 1) pyperclip: the standard cross-platform wrapper (optional dependency).
    try:
        import pyperclip  # type: ignore

        pyperclip.copy(text)
        return True, "Copied to clipboard"
    except Exception:
        pass

    # 2) Direct command-line tools.
    candidates: list[tuple[list[str], str]] = [
        (["xclip", "-selection", "clipboard"], "xclip"),
        (["xsel", "--clipboard", "--input"], "xsel"),
        (["wl-copy"], "wl-copy"),
        (["pbcopy"], "pbcopy"),
        (["clip.exe"], "clip.exe"),
    ]
    for argv, tool in candidates:
        if shutil.which(argv[0]) is None:
            continue
        try:
            completed = subprocess.run(
                argv,
                input=text.encode("utf-8"),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=5,
            )
        except Exception:
            continue
        if completed.returncode == 0:
            return True, f"Copied to clipboard via {tool}"

    return False, (
        "No clipboard available — install xclip/xsel/wl-copy (Linux) or "
        "run inside a clipboard-capable terminal"
    )
