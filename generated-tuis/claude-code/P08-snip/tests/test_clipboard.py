"""Clipboard: a real backend must be invoked, and failure must be honest."""

from __future__ import annotations

import base64
from pathlib import Path

import toolh.clipboard as clipboard_module
from toolh.clipboard import Clipboard, osc52_sequence


def test_configured_command_receives_the_text(tmp_path: Path):
    sink = tmp_path / "sink.txt"
    board = Clipboard(command="cat > {}".format(sink), allow_osc52=False)
    result = board.copy("print('hello')\nsecond line\n")
    assert result.ok is True
    assert result.backend == "configured command"
    # The payload really went through a subprocess to a real file.
    assert sink.read_text(encoding="utf-8") == "print('hello')\nsecond line\n"


def test_unicode_survives_the_round_trip(tmp_path: Path):
    sink = tmp_path / "sink.txt"
    board = Clipboard(command="cat > {}".format(sink), allow_osc52=False)
    payload = 'def f():\n    return "English-only text ✓"\n'
    assert board.copy(payload).ok is True
    assert sink.read_text(encoding="utf-8") == payload


def test_mirror_file_records_the_payload(tmp_path: Path):
    mirror = tmp_path / "nested" / "mirror.txt"
    board = Clipboard(command="cat > /dev/null", mirror_path=mirror, allow_osc52=False)
    result = board.copy("mirrored text")
    assert result.mirrored == mirror
    assert mirror.read_text(encoding="utf-8") == "mirrored text"


def test_failing_command_falls_through_to_the_next_backend(tmp_path: Path, monkeypatch):
    sink = tmp_path / "second.txt"
    monkeypatch.setattr(
        clipboard_module,
        "_platform_commands",
        lambda: [("fake-tool", ["sh", "-c", "cat > {}".format(sink)])],
    )
    board = Clipboard(command="exit 7", allow_osc52=False)
    result = board.copy("payload")
    assert result.ok is True
    assert result.backend == "fake-tool"
    assert sink.read_text(encoding="utf-8") == "payload"
    assert any("configured command" in attempt for attempt in result.attempts)


def test_missing_binaries_are_skipped(monkeypatch):
    monkeypatch.setattr(
        clipboard_module,
        "_platform_commands",
        lambda: [("ghost", ["definitely-not-a-real-binary-xyz"])],
    )
    monkeypatch.setattr(Clipboard, "_try_pyperclip", lambda self, text: (False, "absent"))
    board = Clipboard(allow_osc52=False)
    result = board.copy("text")
    assert result.ok is False
    assert result.backend == "none"


def test_total_failure_is_reported_not_faked(monkeypatch, tmp_path: Path):
    monkeypatch.setattr(clipboard_module, "_platform_commands", lambda: [])
    monkeypatch.setattr(Clipboard, "_try_pyperclip", lambda self, text: (False, "absent"))
    mirror = tmp_path / "mirror.txt"
    board = Clipboard(mirror_path=mirror, allow_osc52=False)
    result = board.copy("text")
    assert result.ok is False
    assert "Could not reach a system clipboard" in result.message()
    # The mirror still records the text, but it is not claimed as success.
    assert mirror.read_text(encoding="utf-8") == "text"


def test_osc52_is_used_as_a_last_resort(monkeypatch):
    monkeypatch.setattr(clipboard_module, "_platform_commands", lambda: [])
    monkeypatch.setattr(Clipboard, "_try_pyperclip", lambda self, text: (False, "absent"))
    written = []
    board = Clipboard(osc52_writer=written.append)
    result = board.copy("terminal clipboard")
    assert result.ok is True
    assert result.backend == "OSC 52"
    payload = written[0].split(";", 2)[2].rstrip("\x07")
    assert base64.b64decode(payload).decode("utf-8") == "terminal clipboard"


def test_osc52_sequence_shape():
    sequence = osc52_sequence("hi")
    assert sequence.startswith("\x1b]52;c;")
    assert sequence.endswith("\x07")


def test_describe_backends_lists_something(tmp_path: Path):
    board = Clipboard(command="cat", mirror_path=tmp_path / "m.txt")
    names = board.describe_backends()
    assert "configured command" in names
    assert "OSC 52 terminal escape" in names


def test_unwritable_mirror_does_not_break_copying(tmp_path: Path):
    blocker = tmp_path / "not-a-dir"
    blocker.write_text("i am a file", encoding="utf-8")
    sink = tmp_path / "sink.txt"
    board = Clipboard(
        command="cat > {}".format(sink),
        mirror_path=blocker / "mirror.txt",
        allow_osc52=False,
    )
    result = board.copy("still works")
    assert result.ok is True
    assert result.mirrored is None
    assert sink.read_text(encoding="utf-8") == "still works"
