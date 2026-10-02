from __future__ import annotations

import fcntl
import os
import pty
import select
import shlex
import signal
import struct
import subprocess
import sys
import termios
import tempfile
import threading
import time
import tty
import unicodedata
from pathlib import Path
from typing import Callable

from ..keyboard_driver import KeyboardDriver
from .key_parser import KeyParseState, key_name_to_raw_bytes
from .turn_recorder import HumanOperation, HumanTurnRecorder, KeystrokeRecord


def _display_char_width(ch: str) -> int:
    if not ch or ord(ch) < 0x20:
        return 0
    if unicodedata.east_asian_width(ch) in ("F", "W"):
        return 2
    return 1


def _display_width(text: str) -> int:
    return sum(_display_char_width(ch) for ch in text)


def _wrap_display_text(text: str, *, width: int) -> list[str]:
    if width < 1:
        return [text]

    lines: list[str] = []
    line = ""
    line_width = 0

    for word in text.split():
        word_width = _display_width(word)
        sep_width = 1 if line else 0
        if line and line_width + sep_width + word_width <= width:
            line += " " + word
            line_width += sep_width + word_width
            continue

        if line:
            lines.append(line)
            line = ""
            line_width = 0

        if word_width <= width:
            line = word
            line_width = word_width
            continue

        chunk = ""
        chunk_width = 0
        for ch in word:
            ch_width = _display_char_width(ch)
            if chunk and chunk_width + ch_width > width:
                lines.append(chunk)
                chunk = ch
                chunk_width = ch_width
            else:
                chunk += ch
                chunk_width += ch_width
        line = chunk
        line_width = chunk_width

    if line:
        lines.append(line)
    return lines or [""]


def _truncate_display_text(text: str, *, width: int, suffix: str = "...") -> str:
    if _display_width(text) <= width:
        return text

    suffix_width = _display_width(suffix)
    budget = max(0, width - suffix_width)
    out: list[str] = []
    used = 0
    for ch in text:
        ch_width = _display_char_width(ch)
        if used + ch_width > budget:
            break
        out.append(ch)
        used += ch_width
    return "".join(out) + suffix


class HumanSession:
    """Attach to bench tmux session and record human turns via terminal input."""

    def __init__(
        self,
        session_id: str,
        driver: KeyboardDriver,
        *,
        run_dir: Path,
        task_id: str = "",
        description: str = "",
        type_idle_ms: float = 1000.0,
        submit_key: str = "ctrl+g ctrl+g ctrl+g",
        on_keystroke: Callable[[KeystrokeRecord], None] | None = None,
        on_keystroke_snapshot: Callable[..., None] | None = None,
        on_operation: Callable[[HumanOperation], None] | None = None,
        on_operation_complete: Callable[[HumanOperation], None] | None = None,
        container_alive: Callable[[], bool] | None = None,
        log: Callable[[str], None] | None = None,
        task_pane_percent: int = -1,
    ) -> None:
        self.session_id = session_id
        self.driver = driver
        self.run_dir = run_dir
        self.task_id = task_id
        self.description = description
        self._tui_target = session_id
        self._status_saved: dict[str, str] | None = None
        self._status_active = False
        self._desc_pane: str | None = None
        self._desc_pane_height = 0
        self._desc_strip_path: Path | None = None
        self._container_alive = container_alive
        self.recorder = HumanTurnRecorder(
            driver,
            run_dir=run_dir,
            type_idle_ms=type_idle_ms,
            submit_key=submit_key,
            on_keystroke=on_keystroke,
            on_keystroke_snapshot=on_keystroke_snapshot,
            on_operation=on_operation,
            on_operation_complete=on_operation_complete,
            log=log,
        )
        self._log = log or (lambda _msg: None)
        self._task_pane_percent = int(task_pane_percent)
        # submit | tui_exit | attach_lost | None (still running / unknown)
        self.end_reason: str | None = None

    @property
    def operations(self) -> list[HumanOperation]:
        return self.recorder.operations

    @property
    def submitted(self) -> bool:
        return self.recorder.submitted

    def run(self) -> list[HumanOperation]:
        # Native tmux attach — reliable keyboard on macOS. Keys logged via tmux binds.
        return self._run_tmux_native()

    def _process_terminal_input(
        self,
        data: bytes,
        parser: KeyParseState,
        done: threading.Event,
    ) -> bytes:
        """Parse stdin bytes for logging; return bytes that should reach the TUI."""
        forward = bytearray()
        for event in parser.feed(data):
            if event.key_name == "ctrl+c":
                raise KeyboardInterrupt
            submitted, consumed = self.recorder.handle_submit_candidate(event.key_name)
            if consumed:
                if submitted:
                    done.set()
                continue
            if self.recorder.handle_key(event.key_name, raw_bytes=event.raw_bytes):
                done.set()
                continue
            forward.extend(event.raw_bytes)
        return bytes(forward)

    def _run_terminal(self) -> list[HumanOperation]:
        """PTY attach: record every keystroke from raw terminal bytes (no tmux bind whitelist)."""
        self._print_banner()
        self._setup_task_sidebar()
        self._send_tui_mouse_off_once()
        self._disable_mouse()
        done = threading.Event()
        master, proc = self._open_attach()
        self._refresh_pty_size(master)
        old_tty = termios.tcgetattr(sys.stdin)
        old_winch = signal.getsignal(signal.SIGWINCH)
        parser = KeyParseState()
        stdin_error: list[BaseException] = []

        def on_winch(_signum: int, _frame: object | None) -> None:
            self._refresh_pty_size(master)

        def stdin_worker() -> None:
            try:
                while not done.is_set() and proc.poll() is None:
                    try:
                        data = os.read(sys.stdin.fileno(), 4096)
                    except OSError:
                        break
                    if not data:
                        break
                    forward = self._process_terminal_input(data, parser, done)
                    if forward:
                        os.write(master, forward)
            except KeyboardInterrupt:
                done.set()
            except BaseException as exc:
                stdin_error.append(exc)
                done.set()

        def master_worker() -> None:
            try:
                while not done.is_set() and proc.poll() is None:
                    try:
                        out = os.read(master, 4096)
                    except OSError:
                        break
                    if not out:
                        break
                    os.write(sys.stdout.fileno(), out)
            except OSError:
                pass
            finally:
                done.set()

        def monitor_worker() -> None:
            while not done.is_set():
                if self.recorder.submitted:
                    self.end_reason = "submit"
                    self._log("Submit received — running verify…")
                    done.set()
                    return
                if self._should_end_session():
                    self.end_reason = "tui_exit"
                    self._log("TUI exited — stopping session…")
                    done.set()
                    return
                time.sleep(0.5)

        monitor = threading.Thread(target=monitor_worker, daemon=True)
        monitor.start()
        stdin_thread = threading.Thread(target=stdin_worker, daemon=True)
        master_thread = threading.Thread(target=master_worker, daemon=True)
        try:
            signal.signal(signal.SIGWINCH, on_winch)
            tty.setraw(sys.stdin.fileno())
            self._disable_terminal_mouse()
            stdin_thread.start()
            master_thread.start()
            while (
                proc.poll() is None
                and not self.recorder.submitted
                and not done.is_set()
            ):
                time.sleep(0.05)
        finally:
            done.set()
            if self.end_reason is None:
                if self.recorder.submitted:
                    self.end_reason = "submit"
                elif self._should_end_session():
                    self.end_reason = "tui_exit"
                else:
                    self.end_reason = "attach_lost"
            signal.signal(signal.SIGWINCH, old_winch)
            self.recorder.flush()
            termios.tcsetattr(sys.stdin, termios.TCSADRAIN, old_tty)
            self.recorder.join_snapshots()
            self._detach_clients()
            stdin_thread.join(timeout=1)
            master_thread.join(timeout=1)
            monitor.join(timeout=1)
            try:
                os.close(master)
            except OSError:
                pass
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    proc.kill()
            self._remove_task_sidebar()
            if stdin_error:
                raise stdin_error[0]
        return self.operations

    def _run_tmux_native(self) -> list[HumanOperation]:
        """Use a real tmux client for display and tmux key bindings for logging."""
        self._harden_tmux_server()
        self._print_banner()
        self._setup_task_sidebar()
        self._send_tui_mouse_off_once()
        done = threading.Event()
        fifo_path = Path(tempfile.mkdtemp(prefix="keyboard-agent-")) / "keys.fifo"
        os.mkfifo(fifo_path)
        reader = threading.Thread(
            target=self._read_key_fifo,
            args=(fifo_path, done),
            daemon=True,
        )
        reader.start()
        installed: list[tuple[str, str]] = []
        mouse_blockers: list[tuple[str, str]] = []
        attach_proc: subprocess.Popen[bytes] | None = None
        try:
            self._configure_mouse_for_human()
            installed = self._install_tmux_key_bindings(fifo_path)
            mouse_blockers = self._install_mouse_blockers()

            def attach_worker() -> None:
                nonlocal attach_proc
                env = os.environ.copy()
                env.pop("TMUX", None)
                self._disable_terminal_mouse()
                attach_proc = subprocess.Popen(
                    [*_tmux_cli_prefix(), "attach", "-t", self.session_id],
                    env=env,
                )
                attach_proc.wait()
                done.set()

            def monitor_worker() -> None:
                while not done.is_set():
                    if self.recorder.submitted:
                        self.end_reason = "submit"
                        self._log("Submit received — running verify…")
                        self._end_attach()
                        done.set()
                        return
                    if self._should_end_session():
                        self.end_reason = "tui_exit"
                        self._log("TUI exited — stopping session…")
                        self._end_attach()
                        done.set()
                        return
                    time.sleep(0.5)

            threading.Thread(target=monitor_worker, daemon=True).start()

            def layout_worker() -> None:
                # One soft refresh only — repeated respawn-pane during the first
                # seconds raced with warm-pool session churn and killed the tmux server.
                if done.wait(timeout=0.8):
                    return
                self._refresh_task_sidebar(respawn_desc=False)

            threading.Thread(target=layout_worker, daemon=True).start()
            attach_thread = threading.Thread(target=attach_worker, daemon=True)
            attach_thread.start()
            done.wait()
            if self.end_reason is None:
                if self.recorder.submitted:
                    self.end_reason = "submit"
                elif self._should_end_session():
                    self.end_reason = "tui_exit"
                else:
                    # tmux attach returned without submit — often
                    # "[server exited unexpectedly]" or client detach.
                    self.end_reason = "attach_lost"
                    self._log(
                        "tmux attach ended unexpectedly "
                        "(server/session lost) — not treating as submit"
                    )
            self._end_attach(attach_proc)
            attach_thread.join(timeout=3)
        finally:
            done.set()
            self._remove_mouse_blockers(mouse_blockers)
            self._restore_mouse_for_human()
            self._remove_tmux_key_bindings(installed)
            self._remove_task_sidebar()
            self.recorder.flush()
            self.recorder.join_snapshots()
            try:
                fifo_path.unlink()
                fifo_path.parent.rmdir()
            except OSError:
                pass
        return self.operations

    def _read_key_fifo(self, fifo_path: Path, done: threading.Event) -> None:
        fd = os.open(fifo_path, os.O_RDWR | os.O_NONBLOCK)
        try:
            buffer = ""
            while not done.is_set() and not self.recorder.submitted:
                rlist, _, _ = select.select([fd], [], [], 0.1)
                if fd not in rlist:
                    continue
                try:
                    chunk = os.read(fd, 4096).decode("utf-8", errors="replace")
                except BlockingIOError:
                    continue
                if not chunk:
                    continue
                buffer += chunk
                while "\n" in buffer:
                    line, buffer = buffer.split("\n", 1)
                    key = line.strip()
                    if not key:
                        continue
                    if key == "__SUBMIT__":
                        self.recorder.submit()
                        done.set()
                        return
                    if self.recorder.handle_key(
                        key,
                        raw_bytes=key_name_to_raw_bytes(key),
                    ):
                        done.set()
                        return
        finally:
            os.close(fd)

    def _install_tmux_key_bindings(self, fifo_path: Path) -> list[tuple[str, str]]:
        """Install all key loggers in one tmux source-file (fast vs ~90 subprocess calls)."""
        installed: list[tuple[str, str]] = []
        lines: list[str] = []
        tui = self._tui_target

        def bind_literal(agent_key: str, tmux_key: str | None = None) -> None:
            key = tmux_key or agent_key
            shell = (
                f"{_fifo_log_shell(fifo_path, agent_key)}; "
                f"{_tmux_send_literal(tui, agent_key)}"
            )
            lines.append(f"bind-key -n {_tmux_conf_key(key)} run-shell -b {shlex.quote(shell)}")
            installed.append(("root", key))

        def bind_press(agent_key: str, tmux_key: str) -> None:
            shell = (
                f"{_fifo_log_shell(fifo_path, agent_key)}; "
                f"{_tmux_cli_shell()} send-keys -t {shlex.quote(tui)} {shlex.quote(tmux_key)}"
            )
            lines.append(
                f"bind-key -n {_tmux_conf_key(tmux_key)} run-shell -b {shlex.quote(shell)}"
            )
            installed.append(("root", tmux_key))

        for code in range(33, 127):
            ch = chr(code)
            bind_literal(ch, _tmux_bind_key(ch))
        bind_press("space", "Space")
        bind_press("enter", "Enter")
        bind_press("tab", "Tab")
        bind_press("escape", "Escape")
        bind_press("backspace", "BSpace")
        bind_press("delete", "DC")
        bind_press("arrow_up", "Up")
        bind_press("arrow_down", "Down")
        bind_press("arrow_left", "Left")
        bind_press("arrow_right", "Right")
        bind_press("page_up", "PageUp")
        bind_press("page_down", "PageDown")
        bind_press("home", "Home")
        bind_press("end", "End")
        for i in range(1, 11):
            bind_press(f"f{i}", f"F{i}")
        for ch in "abcdefhijklmnopqrstuvwxyz":
            if ch == "g":
                continue  # submit sequence owns C-g
            bind_press(f"ctrl+{ch}", f"C-{ch}")
        for ch in "abcdefghijklmnopqrstuvwxyz":
            bind_press(f"alt+{ch}", f"M-{ch}")
        bind_press("alt+\\", "M-\\")
        bind_press("alt+|", "M-|")

        submit_key = self.recorder.submit_sequence[0] if self.recorder.submit_sequence else "ctrl+g"
        submit_log = _fifo_log_shell(fifo_path, submit_key)
        # Only log to fifo — never send C-g (0x07 bell/^G) to the TUI. Python tracks
        # the submit sequence count; no switch-client / detach-client side effects.
        lines.append(
            f"bind-key -n C-g run-shell -b {shlex.quote(submit_log)}"
        )
        installed.append(("root", "C-g"))

        conf_path = Path(tempfile.mkstemp(prefix="keyboard-agent-", suffix=".conf")[1])
        try:
            conf_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
            proc = self._tmux("source-file", str(conf_path), check=False)
            if proc.returncode != 0:
                detail = (proc.stderr or proc.stdout or "").strip()
                raise RuntimeError(
                    f"tmux source-file failed ({proc.returncode}): {detail or 'unknown error'}"
                )
        finally:
            conf_path.unlink(missing_ok=True)
        return installed

    def _remove_tmux_key_bindings(self, installed: list[tuple[str, str]]) -> None:
        for table, key in reversed(installed):
            args = ["unbind-key"]
            if table == "root":
                args.append("-n")
            else:
                args.extend(["-T", table])
            args.append(key)
            self._tmux(*args, check=False)

    def _harden_tmux_server(self) -> None:
        self._tmux("set-option", "-g", "exit-empty", "off", check=False)
        self._tmux("set-option", "-g", "exit-unattached", "off", check=False)

    @staticmethod
    def _tmux(*args: str | list[str], check: bool = True) -> subprocess.CompletedProcess:
        if len(args) == 1 and isinstance(args[0], list):
            cmd = [*_tmux_cli_prefix(), *args[0]]
        else:
            cmd = [*_tmux_cli_prefix(), *args]
        return subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=check,
        )

    def _print_banner(self) -> None:
        if self.description and self._task_pane_percent != 0:
            return
        submit = self.recorder.submit_key.upper()
        idle_sec = self.recorder.type_idle_ms / 1000.0
        msg = (
            f"\n{'=' * 60}\n"
            f"  Human TUI session — mouse disabled\n"
            f"  Task: {self.task_id or '?'}\n"
            f"  Logs: {self.run_dir}\n"
            f"  Submit sequence: {submit}\n"
            f"  Printable keys: grouped into type turns; gap ≥ {idle_sec:.1f}s → new turn\n"
            f"  Press keys (arrows, enter, ctrl+*, …): one key = one turn\n"
            f"{'=' * 60}\n"
        )
        self._log(msg.rstrip("\n"))

    def _setup_task_sidebar(self) -> None:
        if not self.description or self._task_pane_percent == 0:
            return

        cols, rows = self._pane_size()
        sid = self.session_id
        self._resize_session_to_terminal(cols, rows)

        self._status_saved = self._capture_status_options(sid)
        self._status_active = True
        self._apply_task_status(sid, self._format_submit_hint())
        self._setup_description_pane(cols)

    def _setup_description_pane(self, cols: int) -> None:
        sid = self.session_id
        lines = self._description_lines(cols)
        if not lines:
            return

        strip_path = self.run_dir / ".task_desc.txt"
        strip_path.write_text("\n".join(lines), encoding="utf-8")
        self._desc_strip_path = strip_path
        shell = (
            f"cat {shlex.quote(str(strip_path))}; exec tail -f /dev/null"
        )
        pane_lines = len(lines)
        pane_height = pane_lines + 1

        proc = self._tmux(
            "split-window",
            "-v",
            "-b",
            "-l",
            str(pane_lines),
            "-t",
            sid,
            "sh",
            "-c",
            shell,
            check=False,
        )
        if proc.returncode != 0:
            self._log(f"  description pane unavailable: {proc.stderr.strip()}")
            return

        panes = self._list_pane_ids(sid)
        if len(panes) < 2:
            return

        self._desc_pane = panes[0]
        self._tui_target = panes[-1]
        self._desc_pane_height = pane_height
        self._tmux(
            "set-option",
            "-t",
            self._desc_pane,
            "remain-on-exit",
            "on",
            check=False,
        )
        self._refresh_description_pane(cols)

    def _list_pane_ids(self, sid: str) -> list[str]:
        pane_info = self._tmux(
            "list-panes",
            "-t",
            sid,
            "-F",
            "#{pane_index}:#{pane_id}",
            check=False,
        )
        if pane_info.returncode != 0 or not pane_info.stdout.strip():
            return []
        panes = [line.split(":", 1) for line in pane_info.stdout.strip().splitlines()]
        return [pane_id for _, pane_id in sorted(panes, key=lambda item: int(item[0]))]

    def _description_lines(self, cols: int, *, max_lines: int = 3) -> list[str]:
        text = " ".join(self.description.split())
        width = max(24, cols - 2)
        lines = _wrap_display_text(text, width=width)
        if len(lines) > max_lines:
            lines = lines[:max_lines]
            last = lines[-1]
            if _display_width(last) + _display_width("...") <= width:
                lines[-1] = last + "..."
            else:
                lines[-1] = _truncate_display_text(last, width=width, suffix="...")
        return lines

    def _capture_status_options(self, sid: str) -> dict[str, str]:
        keys = (
            "status",
            "status-position",
            "status-left",
            "status-right",
            "status-left-length",
            "status-right-length",
            "status-interval",
            "status-format[0]",
            "status-format[1]",
            "window-status-format",
            "window-status-current-format",
        )
        saved: dict[str, str] = {}
        for key in keys:
            proc = self._tmux("show-options", "-t", sid, "-v", key, check=False)
            if proc.returncode == 0:
                saved[key] = proc.stdout.rstrip("\n")
        return saved

    def _apply_task_status(self, sid: str, banner: str) -> None:
        self._tmux("set-option", "-t", sid, "status", "on", check=False)
        self._tmux("set-option", "-t", sid, "status-position", "top", check=False)
        self._tmux("set-option", "-t", sid, "status-left-length", "500", check=False)
        self._tmux("set-option", "-t", sid, "status-right-length", "0", check=False)
        self._tmux("set-option", "-t", sid, "status-right", "", check=False)
        self._tmux("set-option", "-t", sid, "status-left", banner, check=False)
        self._tmux("set-option", "-t", sid, "status-interval", "0", check=False)
        # tmux 3.4+ renders window names (e.g. 0:docker*) via status-format, not status-right.
        self._tmux("set-option", "-t", sid, "window-status-format", "", check=False)
        self._tmux("set-option", "-t", sid, "window-status-current-format", "", check=False)
        self._tmux(
            "set-option",
            "-t",
            sid,
            "status-format[0]",
            "#[align=left]#{T:status-left}",
            check=False,
        )
        self._tmux("set-option", "-t", sid, "status-format[1]", "", check=False)

    def _format_submit_hint(self) -> str:
        return "Submit：English-only text Ctrl+G"

    def _refresh_description_pane(self, cols: int, *, respawn: bool = True) -> None:
        if not self._desc_pane or self._desc_strip_path is None:
            return
        lines = self._description_lines(cols)
        self._desc_strip_path.write_text("\n".join(lines), encoding="utf-8")
        self._desc_pane_height = len(lines) + 1
        if respawn:
            shell = (
                f"cat {shlex.quote(str(self._desc_strip_path))}; exec tail -f /dev/null"
            )
            self._tmux(
                "respawn-pane",
                "-k",
                "-t",
                self._desc_pane,
                "sh",
                "-c",
                shell,
                check=False,
            )
        self._tmux(
            "resize-pane",
            "-t",
            self._desc_pane,
            "-y",
            str(self._desc_pane_height),
            check=False,
        )
        self._tmux("select-pane", "-t", self._tui_target, check=False)

    def _refresh_task_sidebar(self, *, respawn_desc: bool = True) -> None:
        if not self._status_active:
            return
        cols, _rows = self._pane_size()
        self._apply_task_status(self.session_id, self._format_submit_hint())
        self._refresh_description_pane(cols, respawn=respawn_desc)

    def _remove_task_sidebar(self) -> None:
        sid = self.session_id
        if self._desc_pane:
            self._tmux("kill-pane", "-t", self._desc_pane, check=False)
            self._desc_pane = None
        self._desc_strip_path = None
        if self._status_saved is not None:
            for key, value in self._status_saved.items():
                self._tmux("set-option", "-t", sid, key, value, check=False)
            self._status_saved = None
        self._status_active = False
        self._tmux(
            "set-option",
            "-t",
            sid,
            "window-size",
            "largest",
            check=False,
        )

    def _inner_rows(self, rows: int) -> int:
        """Usable pane rows (tmux client status line occupies one row when attached)."""
        return max(10, rows - 1)

    def _resize_session_to_terminal(self, cols: int, rows: int) -> None:
        inner = self._inner_rows(rows)
        sid = self.session_id
        self._tmux("set-option", "-t", sid, "window-size", "manual", check=False)
        self._tmux(
            "resize-window",
            "-t",
            sid,
            "-x",
            str(cols),
            "-y",
            str(inner),
            check=False,
        )

    def _configure_mouse_for_human(self) -> None:
        """Enable tmux mouse handling so root-table blockers swallow clicks."""
        subprocess.run(
            [*_tmux_cli_prefix(), "set-option", "-t", self.session_id, "mouse", "on"],
            check=False,
            capture_output=True,
        )

    def _restore_mouse_for_human(self) -> None:
        subprocess.run(
            [*_tmux_cli_prefix(), "set-option", "-t", self.session_id, "mouse", "off"],
            check=False,
            capture_output=True,
        )

    def _send_tui_mouse_off_once(self) -> None:
        """Tell the TUI app not to track mouse (once at session start)."""
        for hex_seq in (
            "1b 5b 3f 31 30 30 30 6c",  # \033[?1000l
            "1b 5b 3f 31 30 30 32 6c",  # \033[?1002l
            "1b 5b 3f 31 30 30 33 6c",  # \033[?1003l
            "1b 5b 3f 31 30 30 36 6c",  # \033[?1006l
        ):
            self._tmux(
                "send-keys",
                "-t",
                self._tui_target,
                "-H",
                hex_seq,
                check=False,
            )

    def _disable_mouse(self) -> None:
        subprocess.run(
            [*_tmux_cli_prefix(), "set-option", "-t", self.session_id, "mouse", "off"],
            check=False,
            capture_output=True,
        )

    def _should_end_session(self) -> bool:
        if self._container_alive is not None and not self._container_alive():
            return True
        return self._is_tui_pane_dead()

    def _is_tui_pane_dead(self) -> bool:
        if self._tui_target.startswith("%"):
            return self._pane_dead(self._tui_target)

        proc = self._tmux(
            "list-panes",
            "-t",
            self.session_id,
            "-F",
            "#{pane_id}:#{pane_dead}",
            check=False,
        )
        if proc.returncode != 0:
            return False
        for line in proc.stdout.splitlines():
            pane_id, dead = line.split(":", 1)
            if self._desc_pane and pane_id == self._desc_pane:
                continue
            if dead.strip() == "1":
                return True
        return False

    def _pane_dead(self, pane_target: str) -> bool:
        proc = self._tmux(
            "list-panes",
            "-t",
            pane_target,
            "-F",
            "#{pane_dead}",
            check=False,
        )
        if proc.returncode != 0:
            return False
        return any(line.strip() == "1" for line in proc.stdout.splitlines())

    def _end_attach(
        self,
        attach_proc: subprocess.Popen[bytes] | None = None,
    ) -> None:
        self._detach_clients()
        if attach_proc is None or attach_proc.poll() is not None:
            return
        try:
            attach_proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            attach_proc.terminate()
            try:
                attach_proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                attach_proc.kill()

    def _detach_clients(self) -> None:
        self._tmux(
            "detach-client",
            "-s",
            self.session_id,
            "-a",
            check=False,
        )

    _MOUSE_BLOCKER_KEYS: tuple[str, ...] = (
        "MouseDown1Pane",
        "MouseDown2Pane",
        "MouseDown3Pane",
        "MouseUp1Pane",
        "WheelUpPane",
        "WheelDownPane",
        "DoubleClick1Pane",
        "TripleClick1Pane",
        "M-MouseDown3Pane",
    )

    def _install_mouse_blockers(self) -> list[tuple[str, str]]:
        installed: list[tuple[str, str]] = []
        for key in self._MOUSE_BLOCKER_KEYS:
            self._tmux("bind-key", "-T", "root", key, "run-shell", "-b", "true", check=False)
            installed.append(("root", key))
        return installed

    def _remove_mouse_blockers(self, installed: list[tuple[str, str]]) -> None:
        for table, key in reversed(installed):
            self._tmux("unbind-key", "-T", table, key, check=False)

    @staticmethod
    def _disable_terminal_mouse() -> None:
        sys.stdout.write("\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l")
        sys.stdout.flush()

    def _open_attach(self) -> tuple[int, subprocess.Popen]:
        master, slave = pty.openpty()
        cols, rows = self._pane_size()
        HumanSession._set_winsize(master, rows, cols)
        env = os.environ.copy()
        # The operator may launch the harness from inside another tmux session.
        # Removing TMUX lets the child client attach to the bench session instead
        # of trying to nest inside the outer tmux client.
        env.pop("TMUX", None)
        proc = subprocess.Popen(
            [*_tmux_cli_prefix(), "attach", "-t", self.session_id],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            preexec_fn=os.setsid,
            env=env,
        )
        os.close(slave)
        return master, proc

    def _refresh_pty_size(self, master: int) -> None:
        cols, rows = self._pane_size()
        HumanSession._set_winsize(master, rows, cols)

    def _pane_size(self) -> tuple[int, int]:
        """Operator terminal size (PTY attach must match the real terminal, not tmux pane)."""
        try:
            size = os.get_terminal_size()
            return size.columns, size.lines
        except OSError:
            pass
        return 120, 30

    @staticmethod
    def _set_winsize(fd: int, rows: int, cols: int) -> None:
        winsize = struct.pack("HHHH", rows, cols, 0, 0)
        fcntl.ioctl(fd, termios.TIOCSWINSZ, winsize)


def _tmux_cli_prefix() -> list[str]:
    """Match agent-tui's dedicated bench socket (avoid the user's default tmux server)."""
    try:
        from agent_tui.tmux_backend import tmux_cli_prefix

        return list(tmux_cli_prefix())
    except ImportError:
        sock = (os.environ.get("TUI_BENCH_TMUX_SOCKET") or "tui-bench").strip() or "tui-bench"
        return ["tmux", "-L", sock]


def _tmux_cli_shell() -> str:
    return " ".join(shlex.quote(p) for p in _tmux_cli_prefix())


def _fifo_log_shell(fifo_path: Path, agent_key: str) -> str:
    """Append one key line to the fifo using octal escapes (safe for \\ ' \" % etc.)."""
    path = shlex.quote(str(fifo_path))
    parts = "".join(f"\\{ord(c):03o}" for c in agent_key + "\n")
    return f"printf '{parts}' >> {path}"


def _tmux_send_literal(tui_target: str, text: str) -> str:
    """Send one literal character via hex (-l - is parsed as tmux flags)."""
    hex_seq = text.encode("utf-8").hex(" ")
    return (
        f"{_tmux_cli_shell()} send-keys -t {shlex.quote(tui_target)} -H {hex_seq}"
    )


def _tmux_bind_key(ch: str) -> str:
    """Map a literal character to tmux bind-key syntax."""
    if ch == ";":
        return r"\;"
    return ch


def _tmux_conf_key(key: str) -> str:
    """Format a tmux key for source-file (# and { } need quoting; ~ expands to $HOME)."""
    if key == "M-\\":
        # tmux config treats \ as escape; M-\ before space becomes key "M- run-shell".
        return "M-\\\\"
    if key.startswith(("C-", "M-", "F")) or key in {
        "Space", "Enter", "Tab", "Escape", "BSpace", "DC",
        "Up", "Down", "Left", "Right", "PageUp", "PageDown", "Home", "End",
    } or (len(key) > 1 and key.startswith("F") and key[1:].isdigit()):
        return key
    if key == r"\;":
        return r"\;"
    if len(key) != 1:
        return key
    if key.isalnum():
        return key
    escaped = {
        '"': r"\"",
        "'": r"\'",
        "\\": r"\\",
        "~": r"\~",
        ";": r"\;",
        "-": '"-"',
        "%": '"%"',
        "#": '"#"',
        "{": '"{"',
        "}": '"}"',
    }
    return escaped.get(key, key)
