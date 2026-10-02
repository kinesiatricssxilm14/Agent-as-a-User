#!/usr/bin/env python3
"""Drive the real `toolc` binary through a PTY and verify filesystem effects.

This is an end-to-end check: it launches the installed executable in a
pseudo-terminal, sends keystrokes, and then asserts on what actually landed on
disk. Nothing here inspects internal state.

    python3 scripts/e2e_test.py [path-to-toolc]
"""

import os
import pty
import re
import select
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

BIN = sys.argv[1] if len(sys.argv) > 1 else "toolc"
COLS, ROWS = 120, 34


def _wide(ch):
    """True for East-Asian wide/fullwidth characters, which occupy two cells."""
    import unicodedata

    return unicodedata.east_asian_width(ch) in ("W", "F")


class Screen:
    """A tiny terminal emulator.

    ratatui draws by moving the cursor and repainting only the cells that
    changed, so concatenating the raw byte stream does not reconstruct what the
    user sees. Tracking a real character grid does, which is what makes the
    assertions below trustworthy.
    """

    CSI = re.compile(r"\x1b\[([0-9;?]*)([@-~])")

    def __init__(self, cols, rows):
        self.cols, self.rows = cols, rows
        self.grid = [[" "] * cols for _ in range(rows)]
        self.cx = self.cy = 0
        # Holds a trailing partial escape sequence between feeds.
        self.pending = ""

    def feed(self, data):
        data = self.pending + data
        self.pending = ""
        i = 0
        while i < len(data):
            ch = data[i]
            if ch == "\x1b":
                rest = data[i:]
                match = self.CSI.match(data, i)
                if match:
                    self._csi(match.group(1), match.group(2))
                    i = match.end()
                    continue
                # An escape that is still arriving: keep it for the next feed.
                if self._incomplete_escape(rest):
                    self.pending = rest
                    return
                # Some other complete two-character escape (charset, etc.).
                i += 2
                continue
            if ch == "\n":
                self.cy = min(self.cy + 1, self.rows - 1)
            elif ch == "\r":
                self.cx = 0
            elif ch == "\b":
                self.cx = max(0, self.cx - 1)
            elif ch == "\t":
                self.cx = min(self.cols - 1, (self.cx // 8 + 1) * 8)
            elif ch >= " ":
                if self.cy < self.rows and self.cx < self.cols:
                    self.grid[self.cy][self.cx] = ch
                    # A double-width glyph consumes the next cell too.
                    if _wide(ch) and self.cx + 1 < self.cols:
                        self.grid[self.cy][self.cx + 1] = ""
                        self.cx += 1
                self.cx += 1
                if self.cx >= self.cols:
                    self.cx = self.cols - 1
            i += 1

    @staticmethod
    def _incomplete_escape(rest):
        """True when `rest` is the start of an escape sequence, still unfinished."""
        if rest == "\x1b":
            return True
        if rest[1] == "[":
            # CSI: parameters then a final byte in @-~. Unfinished if no final yet.
            return all(c in "0123456789;?" for c in rest[2:])
        return len(rest) < 2

    def _csi(self, params, final):
        nums = [int(p) if p.isdigit() else 0 for p in params.split(";") if p != ""]

        def arg(index, default=1):
            return nums[index] if len(nums) > index and nums[index] else default

        if final == "H" or final == "f":            # cursor position (1-based)
            self.cy = min(max(arg(0) - 1, 0), self.rows - 1)
            self.cx = min(max(arg(1) - 1, 0), self.cols - 1)
        elif final == "A":
            self.cy = max(0, self.cy - arg(0))
        elif final == "B":
            self.cy = min(self.rows - 1, self.cy + arg(0))
        elif final == "C":
            self.cx = min(self.cols - 1, self.cx + arg(0))
        elif final == "D":
            self.cx = max(0, self.cx - arg(0))
        elif final == "G":
            self.cx = min(max(arg(0) - 1, 0), self.cols - 1)
        elif final == "J":                          # erase in display
            mode = nums[0] if nums else 0
            if mode == 2:
                self.grid = [[" "] * self.cols for _ in range(self.rows)]
            elif mode == 0:
                for x in range(self.cx, self.cols):
                    self.grid[self.cy][x] = " "
                for y in range(self.cy + 1, self.rows):
                    self.grid[y] = [" "] * self.cols
        elif final == "K":                          # erase in line
            mode = nums[0] if nums else 0
            if mode == 0:
                for x in range(self.cx, self.cols):
                    self.grid[self.cy][x] = " "
            elif mode == 1:
                for x in range(0, self.cx + 1):
                    self.grid[self.cy][x] = " "
            else:
                self.grid[self.cy] = [" "] * self.cols
        # SGR (m), cursor visibility (h/l) and the rest do not affect content.

    def text(self):
        return "\n".join("".join(row).rstrip() for row in self.grid)


class Session:
    """A running toolc process attached to a PTY."""

    def __init__(self, workdir):
        self.master, slave = pty.openpty()
        # Tell the child how big the window is, so ratatui lays out normally.
        import fcntl
        import struct
        import termios

        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        env = dict(os.environ, TERM="xterm-256color")
        self.proc = subprocess.Popen(
            [BIN, str(workdir)],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=env,
            close_fds=True,
        )
        os.close(slave)
        self.screen_state = Screen(COLS, ROWS)
        # Decodes incrementally, so a UTF-8 sequence split across two reads
        # is reassembled instead of turning into replacement characters.
        import codecs

        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.drain(0.7)

    def drain(self, timeout=0.35):
        """Feed whatever the app has drawn into the emulator."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            ready, _, _ = select.select([self.master], [], [], 0.05)
            if not ready:
                continue
            try:
                chunk = os.read(self.master, 65536)
            except OSError:
                break
            if not chunk:
                break
            self.screen_state.feed(self.decoder.decode(chunk))

    def send(self, keys, settle=0.3):
        os.write(self.master, keys.encode())
        self.drain(settle)

    def screen(self):
        """The current frame as the user would see it."""
        return self.screen_state.text()

    def close(self):
        try:
            self.send("q", 0.3)
            self.proc.wait(timeout=3)
        except Exception:
            self.proc.kill()
            self.proc.wait(timeout=3)
        finally:
            try:
                os.close(self.master)
            except OSError:
                pass


PASSED = 0
FAILED = []


def check(name, condition, detail=""):
    global PASSED
    if condition:
        PASSED += 1
        print(f"  PASS  {name}")
    else:
        FAILED.append(name)
        print(f"  FAIL  {name}")
        if detail:
            print("        " + detail.replace("\n", "\n        ")[:2000])


def fixture():
    root = Path(tempfile.mkdtemp(prefix="toolc-e2e-"))
    src = root / "src"
    src.mkdir()
    (src / "config.yml").write_text("server:\n  port: 8080\ntls: true\n")
    (src / "notes.txt").write_text("remember to rotate the logs\n")
    (src / "app.log").write_text("INFO started\nWARN disk slow\n")
    return root, src


def test_browse_and_preview():
    print("\n[browse + preview on one screen]")
    root, src = fixture()
    try:
        s = Session(src)
        out = s.screen()
        check("file list shows every entry",
              all(n in out for n in ("config.yml", "notes.txt", "app.log")), out)
        check("preview shows the selected file's content",
              "INFO started" in out, out)
        check("preview names the selected file", "app.log" in out, out)
        check("working directory is displayed", str(src) in out, out)
        check("key hints are visible",
              "copy" in out and "rename" in out and "delete" in out, out)
        s.close()
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_preview_updates_on_move():
    print("\n[preview follows the selection]")
    root, src = fixture()
    try:
        s = Session(src)
        s.send("\x1b[B")  # Down -> config.yml
        out = s.screen()
        check("second file's content is shown", "port: 8080" in out, out)
        s.close()
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_copy():
    print("\n[copy]")
    root, src = fixture()
    try:
        s = Session(src)
        # app.log is selected first (alphabetical). Copy it to a new name.
        s.send("c")
        s.send("app.log.bak")
        s.send("\r", 0.5)
        s.close()
        dst = src / "app.log.bak"
        check("destination file exists", dst.exists())
        check("source file still exists", (src / "app.log").exists())
        check("copy is byte-identical",
              dst.exists() and dst.read_bytes() == (src / "app.log").read_bytes())
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_mkdir_and_move():
    print("\n[mkdir + move]")
    root, src = fixture()
    try:
        s = Session(src)
        s.send("n")
        s.send("archive")
        s.send("\r", 0.5)
        check("directory was created", (src / "archive").is_dir())

        # Select app.log by filtering, then move it into archive/.
        s.send("/")
        s.send("app.log")
        s.send("\r", 0.4)
        s.send("m")
        s.send("archive/")
        s.send("\r", 0.6)
        s.close()

        moved = src / "archive" / "app.log"
        check("file exists at the new path", moved.exists())
        check("original path is gone", not (src / "app.log").exists())
        check("content survived the move",
              moved.exists() and moved.read_text() == "INFO started\nWARN disk slow\n")
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_rename():
    print("\n[rename]")
    root, src = fixture()
    try:
        original = (src / "notes.txt").read_text()
        s = Session(src)
        s.send("/")
        s.send("notes")
        s.send("\r", 0.4)
        s.send("r")
        # The prompt pre-fills the old name; clear it with Ctrl-U first.
        s.send("\x15")
        s.send("todo.md")
        s.send("\r", 0.6)
        s.close()
        check("new name exists", (src / "todo.md").exists())
        check("old name is gone", not (src / "notes.txt").exists())
        check("content unchanged",
              (src / "todo.md").exists() and (src / "todo.md").read_text() == original)
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_delete_requires_confirmation():
    print("\n[delete]")
    root, src = fixture()
    try:
        s = Session(src)
        s.send("/")
        s.send("app.log")
        s.send("\r", 0.4)
        s.send("d", 0.4)
        out = s.screen()
        check("confirmation is requested", "Confirm" in out or "y/n" in out, out)
        s.send("n", 0.4)
        check("answering no keeps the file", (src / "app.log").exists())

        s.send("d", 0.4)
        s.send("y", 0.6)
        check("answering yes deletes the file", not (src / "app.log").exists())
        s.close()
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_directory_navigation():
    print("\n[hierarchical navigation]")
    root, src = fixture()
    try:
        (src / "deep").mkdir()
        (src / "deep" / "buried.txt").write_text("found me\n")
        s = Session(src)
        s.send("\x1b[A" * 6)          # go to the top of the list
        s.send("\r", 0.5)             # enter `deep`
        out = s.screen()
        check("entered the subdirectory", "buried.txt" in out, out)
        check("subdirectory content is previewed", "found me" in out, out)
        s.send("\x1b[D", 0.5)         # Left -> parent
        out = s.screen()
        check("returned to the parent", "config.yml" in out, out)
        s.close()
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_long_file_scrolls_to_the_end():
    print("\n[full content reachable by scrolling]")
    root, src = fixture()
    try:
        # Many short lines: the tail must be reachable.
        (src / "a_rows.txt").write_text("".join(f"row {i}\n" for i in range(1, 301)))
        # Long lines that soft-wrap: the tail must still be reachable, which is
        # only true if the scroll limit counts wrapped rows, not logical lines.
        (src / "b_wide.txt").write_text(
            "".join(f"L{i:02d} " + "w" * 400 + "\n" for i in range(1, 41)) + "FINAL-LINE\n"
        )
        s = Session(src)
        # Select by filtering, so the assertions do not depend on list order.
        s.send("/")
        s.send("a_rows")
        s.send("\r", 0.4)
        s.send("\t")                    # focus the preview
        s.send("\x1b[4~", 0.6)          # End -> bottom
        check("last line of a 300-line file is reachable", "row 300" in s.screen(),
              s.screen())

        s.send("\t")                    # back to the list
        s.send("/")
        s.send("\x15")                  # Ctrl-U: clear the previous filter
        s.send("b_wide")
        s.send("\r", 0.4)
        s.send("\t")                    # focus the preview again
        s.send("\x1b[4~", 0.7)          # End -> bottom
        check("last line of a soft-wrapped file is reachable",
              "FINAL-LINE" in s.screen(), s.screen())

        s.send("\x1b[1~", 0.5)          # Home -> top
        check("Home returns to the first line", "L01" in s.screen(), s.screen())
        s.close()
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_help_is_discoverable():
    print("\n[help]")
    root, src = fixture()
    try:
        s = Session(src)
        s.send("?", 0.5)
        out = s.screen()
        check("help lists key bindings", "Navigate" in out and "File operations" in out, out)
        check("help keeps the file list visible", "config.yml" in out, out)
        s.close()
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_overwrite_confirmation():
    print("\n[overwrite guard]")
    root, src = fixture()
    try:
        s = Session(src)
        # Copy app.log over notes.txt; that must ask before clobbering.
        s.send("c")
        s.send("notes.txt")
        s.send("\r", 0.5)
        out = s.screen()
        check("overwrite is confirmed first", "verwrite" in out, out)
        check("target untouched before answering",
              (src / "notes.txt").read_text() == "remember to rotate the logs\n")
        s.send("y", 0.6)
        s.close()
        check("target replaced after confirming",
              (src / "notes.txt").read_text() == "INFO started\nWARN disk slow\n")
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_absolute_destination():
    print("\n[absolute destination outside cwd]")
    root, src = fixture()
    try:
        outside = root / "elsewhere"
        outside.mkdir()
        s = Session(src)
        s.send("c")
        s.send(str(outside / "exported.log"))
        s.send("\r", 0.6)
        s.close()
        target = outside / "exported.log"
        check("file written outside the working directory", target.exists())
        check("content matches the source",
              target.exists() and target.read_text() == "INFO started\nWARN disk slow\n")
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_error_is_reported():
    print("\n[error handling]")
    root, src = fixture()
    try:
        s = Session(src)
        s.send("c")
        s.send("/nonexistent-parent-dir-zz/out.txt")
        s.send("\r", 0.5)
        out = s.screen()
        check("missing directory is reported", "does not exist" in out, out)
        check("app is still running and drawing", "Files" in out, out)
        s.close()
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_cli_surface():
    print("\n[command line]")
    help_out = subprocess.run([BIN, "--help"], capture_output=True, text=True)
    check("--help succeeds", help_out.returncode == 0)
    check("--help documents the default directory",
          "/bench/data/src" in help_out.stdout, help_out.stdout)
    ver = subprocess.run([BIN, "--version"], capture_output=True, text=True)
    check("--version prints the name", ver.stdout.startswith("toolc "), ver.stdout)
    bad = subprocess.run([BIN, "/no/such/dir/at/all"], capture_output=True, text=True)
    check("missing directory exits nonzero", bad.returncode != 0)
    check("missing directory explains itself", "cannot open" in bad.stderr, bad.stderr)


def main():
    print(f"driving: {BIN}")
    test_browse_and_preview()
    test_preview_updates_on_move()
    test_copy()
    test_mkdir_and_move()
    test_rename()
    test_delete_requires_confirmation()
    test_directory_navigation()
    test_long_file_scrolls_to_the_end()
    test_help_is_discoverable()
    test_overwrite_confirmation()
    test_absolute_destination()
    test_error_is_reported()
    test_cli_surface()

    print(f"\n{PASSED} passed, {len(FAILED)} failed")
    if FAILED:
        for name in FAILED:
            print(f"  - {name}")
        sys.exit(1)


if __name__ == "__main__":
    main()
