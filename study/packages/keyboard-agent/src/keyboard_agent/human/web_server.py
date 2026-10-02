from __future__ import annotations

import json
import subprocess
import threading
import webbrowser
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Callable
from urllib.parse import urlparse

from ..keyboard_driver import KeyboardDriver
from .browser_keys import browser_key_to_agent
from .key_classifier import is_type_key
from .startup_status import PHASE_ACTIVE, PHASE_SUBMITTED, StartupStatus
from .turn_recorder import HumanOperation, HumanTurnRecorder, KeystrokeRecord

STATIC_DIR = Path(__file__).resolve().parent / "static"


@dataclass
class WebSessionState:
    task_id: str
    description: str
    submit_key: str
    recorder: HumanTurnRecorder
    driver: KeyboardDriver
    screen_svg: str = ""
    screen_text: str = ""
    done: threading.Event = threading.Event()


@dataclass
class StartupWebServer:
    status: StartupStatus
    server: ThreadingHTTPServer
    url: str
    _thread: threading.Thread

    def shutdown(self) -> None:
        self.server.shutdown()
        self._thread.join(timeout=2.0)


class _HandlerState:
    startup: StartupStatus | None = None
    session: WebSessionState | None = None


class _WebHandler(BaseHTTPRequestHandler):
    state: _HandlerState = _HandlerState()

    def log_message(self, format: str, *args) -> None:
        return

    def _send_json(self, code: int, payload: dict) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _send_bytes(self, code: int, body: bytes, content_type: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        handler = self.state
        if path in ("/", "/index.html"):
            html = (STATIC_DIR / "index.html").read_bytes()
            self._send_bytes(200, html, "text/html; charset=utf-8")
            return
        if path == "/api/state":
            session = handler.session
            if session is not None:
                self._send_json(
                    200,
                    {
                        "phase": PHASE_ACTIVE,
                        "message": "✅ English-only text！",
                        "ready": True,
                        "active": True,
                        "task_id": session.task_id,
                        "description": session.description,
                        "submit_key": session.submit_key,
                        "submitted": session.recorder.submitted,
                        "turn_count": len(session.recorder.operations),
                        "screen_text": session.screen_text,
                    },
                )
                return
            startup = handler.startup
            if startup is not None:
                payload = startup.snapshot()
                payload.update(
                    {
                        "submit_key": "",
                        "submitted": False,
                        "turn_count": 0,
                        "screen_text": "",
                        "active": False,
                    }
                )
                self._send_json(200, payload)
                return
            self._send_json(503, {"error": "session not ready"})
            return
        session = handler.session
        if session is None:
            self._send_json(503, {"error": "session not ready"})
            return
        if path == "/api/screen.svg":
            svg = session.screen_svg.encode("utf-8") if session.screen_svg else b""
            self._send_bytes(200, svg, "image/svg+xml; charset=utf-8")
            return
        self._send_json(404, {"error": "not found"})

    def do_POST(self) -> None:
        path = urlparse(self.path).path
        session = self.state.session
        if session is None:
            self._send_json(503, {"error": "session not ready"})
            return
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length else b"{}"
        try:
            payload = json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError:
            self._send_json(400, {"error": "invalid json"})
            return

        if path == "/api/key":
            agent_key, type_char = browser_key_to_agent(payload)
            self._forward_key(session, agent_key, type_char)
            self._send_json(200, {"ok": True, "submitted": session.recorder.submitted})
            if session.recorder.submitted:
                session.done.set()
            return
        if path == "/api/submit":
            session.recorder.submit()
            self._send_json(200, {"ok": True, "submitted": True})
            session.done.set()
            return
        self._send_json(404, {"error": "not found"})

    @staticmethod
    def _forward_key(state: WebSessionState, agent_key: str, type_char: str | None) -> None:
        tmux = state.driver._tmux
        done, consumed = state.recorder.handle_submit_candidate(agent_key)
        if consumed:
            return
        done = state.recorder.handle_key(agent_key)
        if done:
            return
        if type_char is not None and is_type_key(agent_key):
            tmux.type_text(type_char)
        else:
            tmux.press(agent_key)


def _poll_screen(state: WebSessionState, interval: float = 0.25) -> None:
    while not state.done.is_set():
        try:
            state.screen_text = str(state.driver._tmux.snapshot(format="plain"))
            svg_path = state.driver.turns_dir / "_live.svg"
            state.driver._tmux.snapshot(format="svg", output_path=str(svg_path))
            state.screen_svg = svg_path.read_text(encoding="utf-8")
        except Exception:
            pass
        state.done.wait(interval)


def run_startup_web_server(
    *,
    status: StartupStatus,
    host: str,
    port: int,
    open_browser: bool,
    log: Callable[[str], None] | None = None,
) -> StartupWebServer:
    """Start HTTP UI early so operators see ⏳/✅ during container startup."""
    emit = log or (lambda _msg: None)
    handler_state = _HandlerState()
    handler_state.startup = status
    _WebHandler.state = handler_state

    server = ThreadingHTTPServer((host, port), _WebHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    url = f"http://{host}:{port}/"
    emit(f"Human web UI: {url}")
    emit("⏳ English-only text，English-only text ✅ English-only text…")
    if open_browser:
        webbrowser.open(url)

    return StartupWebServer(status=status, server=server, url=url, _thread=thread)


def run_web_session(
    *,
    session_id: str,
    driver: KeyboardDriver,
    run_dir: Path,
    task_id: str,
    description: str,
    type_idle_ms: float,
    submit_key: str,
    host: str,
    port: int,
    open_browser: bool,
    on_keystroke: Callable[[KeystrokeRecord], None] | None = None,
    on_keystroke_snapshot: Callable[..., None] | None = None,
    on_operation: Callable[[HumanOperation], None] | None = None,
    on_operation_complete: Callable[[HumanOperation], None] | None = None,
    log: Callable[[str], None] | None = None,
    startup_server: StartupWebServer | None = None,
) -> HumanTurnRecorder:
    from .session import _tmux_cli_prefix

    subprocess.run(
        [*_tmux_cli_prefix(), "set-option", "-t", session_id, "mouse", "off"],
        check=False,
        capture_output=True,
    )
    recorder = HumanTurnRecorder(
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
    state = WebSessionState(
        task_id=task_id,
        description=description,
        submit_key=submit_key,
        recorder=recorder,
        driver=driver,
    )
    poll_thread = threading.Thread(target=_poll_screen, args=(state,), daemon=True)
    poll_thread.start()

    emit = log or (lambda _msg: None)

    if startup_server is not None:
        startup_server.status.set(PHASE_ACTIVE, "✅ English-only text！English-only text。")
        _WebHandler.state.session = state
        emit(f"Submit sequence: {submit_key.upper()} or click Submit in browser")
        state.done.wait()
        recorder.flush()
        recorder.join_snapshots()
        startup_server.status.set(PHASE_SUBMITTED, "✅ English-only text，English-only text。")
        startup_server.shutdown()
        return recorder

    server = ThreadingHTTPServer((host, port), _WebHandler)
    handler_state = _HandlerState()
    handler_state.session = state
    _WebHandler.state = handler_state
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    url = f"http://{host}:{port}/"
    emit(f"Human web UI: {url}")
    emit(f"Submit sequence: {submit_key.upper()} or click Submit in browser")
    if open_browser:
        webbrowser.open(url)

    state.done.wait()
    recorder.flush()
    recorder.join_snapshots()
    server.shutdown()
    return recorder
