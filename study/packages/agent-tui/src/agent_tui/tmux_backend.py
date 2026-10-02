import os
import re
import time
import uuid
import json
import shutil
import hashlib
import subprocess
from pathlib import Path
from typing import Optional, List, Dict, Any

from .ansi_parser import (
    annotate_cursor_plain,
    annotate_cursor_semantic,
    to_plain,
    to_json,
    to_semantic,
    to_image,
)
from .syscall_parser import read_incremental_log

def _agent_tui_dir() -> Path:
    override = (os.environ.get("AGENT_TUI_HOME") or "").strip()
    if override:
        path = Path(override).expanduser()
    else:
        path = Path.home() / ".agent-tui"
    try:
        path.mkdir(parents=True, exist_ok=True)
        (path / "sessions").mkdir(parents=True, exist_ok=True)
        return path
    except Exception:
        fallback = Path.cwd() / ".agent-tui"
        fallback.mkdir(parents=True, exist_ok=True)
        (fallback / "sessions").mkdir(parents=True, exist_ok=True)
        return fallback


def _active_session_file() -> Path:
    return _agent_tui_dir() / "active_session"


def _sessions_dir() -> Path:
    return _agent_tui_dir() / "sessions"


# Back-compat for imports; resolved at call time via the helpers above.
ACTIVE_SESSION_FILE = Path.home() / ".agent-tui" / "active_session"
SESSIONS_DIR = ACTIVE_SESSION_FILE.parent / "sessions"

def get_session_state_file(session_id: str) -> Path:
    return _sessions_dir() / f"{session_id}.state.json"

def get_strace_log_file(session_id: str) -> Path:
    return _sessions_dir() / f"{session_id}.strace.log"

def update_session_state(session_id: str, updates: dict):
    state_file = get_session_state_file(session_id)
    state = {}
    if state_file.exists():
        try:
            state = json.loads(state_file.read_text())
        except Exception:
            pass
    state.update(updates)
    state_file.write_text(json.dumps(state, indent=2))

def get_session_state(session_id: str) -> dict:
    state_file = get_session_state_file(session_id)
    if state_file.exists():
        try:
            return json.loads(state_file.read_text())
        except Exception:
            pass
    return {}

VALID_KEYS = {
    # Single-letter / digit hotkeys (e.g. kairo: n=new, q=quit)
    **{chr(i): chr(i) for i in range(ord('a'), ord('z') + 1)},
    # Unshifted punctuation hotkeys (e.g. aptui: /=search)
    **{ch: ch for ch in ("/", ".", ",", "-", "=", "[", "]", "\\", ";", "'", "`")},
    **{f"ctrl+{chr(i)}": f"C-{chr(i)}" for i in range(ord('a'), ord('z')+1)},
    **{f"alt+{chr(i)}": f"M-{chr(i)}" for i in range(ord('a'), ord('z')+1)},
    "alt+\\": "M-\\",
    "alt+|": "M-|",
    # Arrow keys
    "arrow_up": "Up",
    "arrow_down": "Down",
    "arrow_left": "Left",
    "arrow_right": "Right",
    # Navigation
    "page_up": "PageUp",
    "page_down": "PageDown",
    "home": "Home",
    "end": "End",
    # Edit/Controls
    "enter": "Enter",
    "tab": "Tab",
    "space": "Space",
    "escape": "Escape",
    "backspace": "BSpace",
    "delete": "DC",
    # Function keys
    **{f"f{i}": f"F{i}" for i in range(1, 11)}
}

def tmux_socket_name() -> str:
    """Dedicated socket so bench traffic does not share the user's default tmux server."""
    name = (os.environ.get("TUI_BENCH_TMUX_SOCKET") or "tui-bench").strip()
    return name or "tui-bench"


def tmux_cli_prefix() -> List[str]:
    return ["tmux", "-L", tmux_socket_name()]


def _ensure_server_hardening() -> None:
    """Keep the bench server alive when sessions churn (warm pool / stop)."""
    subprocess.run(
        tmux_cli_prefix() + ["set-option", "-g", "exit-empty", "off"],
        capture_output=True,
        text=True,
        check=False,
    )
    subprocess.run(
        tmux_cli_prefix() + ["set-option", "-g", "exit-unattached", "off"],
        capture_output=True,
        text=True,
        check=False,
    )


def _run_tmux(args: List[str], check: bool = True) -> subprocess.CompletedProcess:
    """Run a tmux command on the bench socket."""
    cmd = tmux_cli_prefix() + args
    return subprocess.run(cmd, capture_output=True, text=True, check=check)

def get_active_session() -> str:
    path = _active_session_file()
    if not path.exists():
        raise RuntimeError("No active session. Please run `agent-tui start` or `agent-tui use` first.")
    return path.read_text().strip()

def set_active_session(session_id: str):
    path = _active_session_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(session_id)

def start(cmd: str, cwd: Optional[str] = None, label: Optional[str] = None, cols: int = 160, rows: int = 40) -> str:
    session_id = label if label else str(uuid.uuid4())[:8]
    
    # We use env command instead of tmux -e to set environment variables.
    # Older tmux versions (like 2.x on some Linux distros) do not support the -e flag for new-session.
    # By prefixing the command with `env TERM=xterm-256color COLORTERM=truecolor`, we achieve the same 
    # effect universally.
    env_prefix = "env TERM=xterm-256color COLORTERM=truecolor "
    
    # Check if strace is available
    if shutil.which("strace"):
        strace_log = get_strace_log_file(session_id)
        strace_cmd = f"strace -f -s 65535 -e trace=execve,unlink,unlinkat,rename,renameat,kill,connect -o {strace_log} "
        full_cmd = f"{env_prefix}{strace_cmd}{cmd}"
        # Initialize state offset
        update_session_state(session_id, {"strace_log_offset": 0})
    else:
        import sys
        print("Warning: strace not found. System mutation detection will be disabled.", file=sys.stderr)
        full_cmd = f"{env_prefix}{cmd}"
        update_session_state(session_id, {"strace_log_offset": -1}) # Indicate disabled

    safe_cmd = f"sleep 0.1; exec {full_cmd}"
    
    # Force tmux to assume the terminal supports 256 colors with the `-2` flag globally.
    tmux_args = [
        "-2", "new-session", "-d", "-s", session_id, "-x", str(cols), "-y", str(rows)
    ]
    if cwd:
        tmux_args.extend(["-c", cwd])
        
    tmux_args.extend(["sh", "-c", safe_cmd])
    
    try:
        _run_tmux(tmux_args)
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"Failed to start tmux session. Error: {e.stderr or e.stdout or str(e)}")

    _ensure_server_hardening()

    # Attach pipe-pane to capture OSC 4 palette redefinitions
    log_file = _agent_tui_dir() / f"{session_id}.log"
    _run_tmux(["pipe-pane", "-t", session_id, "-o", f"cat > {log_file}"])

    # Session-local terminal options (avoid -g races with concurrent warm-pool starts)
    _run_tmux(["set-option", "-t", session_id, "default-terminal", "tmux-256color"])
    _run_tmux(
        ["set-option", "-t", session_id, "-a", "terminal-overrides", ",xterm-256color:Tc"]
    )

    set_active_session(session_id)
    return session_id

def use(session_id: str):
    # Verify session exists
    result = _run_tmux(["has-session", "-t", session_id], check=False)
    if result.returncode != 0:
        raise ValueError(f"Session '{session_id}' does not exist.")
    set_active_session(session_id)

def _get_screen_hash(session_id: str) -> str:
    """Helper to get a hash of the current screen to detect changes."""
    try:
        result = _run_tmux(["capture-pane", "-t", session_id, "-p", "-e"])
        return hashlib.md5(result.stdout.encode('utf-8')).hexdigest()
    except Exception:
        return ""

def _execute_with_change_detection(
    action_func,
    session_id: Optional[str] = None,
) -> Dict[str, Any]:
    """Execute an action and return a dict with success and has_changed status."""
    try:
        sid = session_id or get_active_session()
        hash_before = _get_screen_hash(sid)
        cursor_before = _get_cursor_pos(sid)

        # Execute the actual action
        action_func()

        # Slight delay to allow TUI to render changes
        time.sleep(0.1)

        hash_after = _get_screen_hash(sid)
        cursor_after = _get_cursor_pos(sid)
        has_changed = (hash_before != hash_after) or (cursor_before != cursor_after)
        
        # System mutation detection
        state = get_session_state(sid)
        offset = state.get("strace_log_offset", -1)
        system_mutation = None
        
        if offset >= 0:
            log_file = get_strace_log_file(sid)
            system_mutation, new_offset = read_incremental_log(log_file, offset)
            if new_offset != offset:
                update_session_state(sid, {"strace_log_offset": new_offset})
        
        result = {
            "success": True,
            "has_changed": has_changed,
            "error": None
        }
        if system_mutation is not None:
            result["system_mutation"] = system_mutation
            
        return result
    except Exception as e:
        return {
            "success": False,
            "has_changed": False,
            "error": str(e)
        }

def type_text(text: str, session_id: Optional[str] = None) -> Dict[str, Any]:
    def action():
        sid = session_id or get_active_session()
        ends_with_enter = text.endswith('\n')
        ends_with_tab = text.endswith('\t')
        
        clean_text = text[:-1] if (ends_with_enter or ends_with_tab) else text
            
        if clean_text:
            _run_tmux(["send-keys", "-t", sid, "-l", clean_text])
            
        if ends_with_enter:
            _run_tmux(["send-keys", "-t", sid, "Enter"])
        elif ends_with_tab:
            _run_tmux(["send-keys", "-t", sid, "Tab"])
            
    return _execute_with_change_detection(action, session_id=session_id)

def send_key(key: str, session_id: Optional[str] = None):
    sid = session_id or get_active_session()
    _run_tmux(["send-keys", "-t", sid, key])

def paste(text: str, session_id: Optional[str] = None) -> Dict[str, Any]:
    def action():
        sid = session_id or get_active_session()
        _run_tmux(["set-buffer", text])
        _run_tmux(["paste-buffer", "-t", sid])
    return _execute_with_change_detection(action, session_id=session_id)

def press(key: str, session_id: Optional[str] = None) -> Dict[str, Any]:
    key_lower = key.lower()
    if key_lower not in VALID_KEYS:
        return {
            "success": False,
            "has_changed": False,
            "error": f"Invalid key: '{key}'. Valid keys are: {', '.join(VALID_KEYS.keys())}"
        }
    
    def action():
        tmux_key = VALID_KEYS[key_lower]
        send_key(tmux_key, session_id=session_id)
    return _execute_with_change_detection(action, session_id=session_id)

def _get_cursor_pos(session_id: str) -> tuple:
    result = _run_tmux(
        [
            "display-message",
            "-t",
            session_id,
            "-p",
            "#{cursor_x},#{cursor_y},#{cursor_flag}",
        ]
    )
    try:
        parts = result.stdout.strip().split(",")
        x, y = int(parts[0]), int(parts[1])
        visible = int(parts[2]) if len(parts) > 2 else 1
        if visible == 0:
            return -1, -1
        return x, y
    except Exception:
        return 0, 0

def _get_size(session_id: str) -> tuple:
    result = _run_tmux(["display-message", "-t", session_id, "-p", "#{pane_width},#{pane_height}"])
    try:
        w, h = map(int, result.stdout.strip().split(','))
        return w, h
    except Exception:
        return 160, 40

def snapshot(
    format: str = "plain",
    output_path: Optional[str] = None,
    session_id: Optional[str] = None,
) -> Any:
    sid = session_id or get_active_session()
    result = _run_tmux(["capture-pane", "-t", sid, "-p", "-e"])
    ansi_text = result.stdout
    
    log_file = _agent_tui_dir() / f"{sid}.log"
    palette_overrides = {}
    if log_file.exists():
        content = log_file.read_text(encoding='utf-8', errors='ignore')
        matches = re.findall(r'\x1b\]4;(\d+);rgb:([0-9a-fA-F]{1,4})/([0-9a-fA-F]{1,4})/([0-9a-fA-F]{1,4})(?:\x1b\\|\x07)', content)
        for idx_str, r, g, b in matches:
            idx = int(idx_str)
            r_val = int(r[:2], 16) if len(r) >= 2 else int(r[0]*2, 16)
            g_val = int(g[:2], 16) if len(g) >= 2 else int(g[0]*2, 16)
            b_val = int(b[:2], 16) if len(b) >= 2 else int(b[0]*2, 16)
            palette_overrides[idx] = (r_val, g_val, b_val)
    
    cx, cy = _get_cursor_pos(sid)

    if format == "plain":
        return annotate_cursor_plain(to_plain(ansi_text), cx, cy)
    elif format == "json":
        return to_json(ansi_text, cx, cy, title=sid)
    elif format == "semantic":
        return annotate_cursor_semantic(
            to_semantic(ansi_text, palette_overrides=palette_overrides), cx, cy
        )
    elif format in ["png", "jpg", "jpeg", "pdf", "svg"]:
        w, h = _get_size(sid)
        return to_image(
            ansi_text,
            img_format=format,
            output_path=output_path,
            width=w,
            height=h,
            palette_overrides=palette_overrides,
            cursor_x=cx,
            cursor_y=cy,
        )
    else:
        raise ValueError(f"Unknown format: {format}")

def scrollup(n: int) -> Dict[str, Any]:
    def action():
        session_id = get_active_session()
        _run_tmux(["copy-mode", "-t", session_id])
        for _ in range(n):
            _run_tmux(["send-keys", "-t", session_id, "Up"])
    return _execute_with_change_detection(action)

def scrolldown(n: int) -> Dict[str, Any]:
    def action():
        session_id = get_active_session()
        for _ in range(n):
            _run_tmux(["send-keys", "-t", session_id, "Down"])
    return _execute_with_change_detection(action)

def find(pattern: str) -> bool:
    content = snapshot("plain")
    return bool(re.search(pattern, content))

def wait(
    timeout: int = 3000,
    debounce: int = 100,
    text: Optional[str] = None,
    session_id: Optional[str] = None,
) -> bool:
    sid = session_id or get_active_session()
    start_time = time.time()
    timeout_sec = timeout / 1000.0
    debounce_sec = debounce / 1000.0
    
    last_hash = None
    
    while True:
        elapsed = time.time() - start_time
        if elapsed > timeout_sec:
            raise TimeoutError(f"Wait timeout after {timeout}ms")
        
        result = _run_tmux(["capture-pane", "-t", sid, "-p", "-e"])
        current_ansi = result.stdout
        
        if text:
            plain_text = to_plain(current_ansi)
            if re.search(text, plain_text):
                return True
        
        current_hash = hashlib.md5(current_ansi.encode('utf-8')).hexdigest()
        
        if last_hash == current_hash:
            # Screen is stable
            if not text:
                return True
        else:
            last_hash = current_hash
            
        time.sleep(debounce_sec)

def list_sessions() -> str:
    result = _run_tmux(["ls"], check=False)
    if result.returncode != 0:
        return "No active sessions."
    return result.stdout.strip()

def info() -> str:
    session_id = get_active_session()
    result = _run_tmux(["display-message", "-t", session_id, "-p", 
                       "Session: #{session_name}\nWindow: #{window_name}\nPane: #{pane_index}\nSize: #{pane_width}x#{pane_height}\nCursor: #{cursor_x},#{cursor_y}"])
    return result.stdout.strip()

def rename(label: str):
    session_id = get_active_session()
    _run_tmux(["rename-session", "-t", session_id, label])
    set_active_session(label)

def kill():
    session_id = get_active_session()
    _run_tmux(["kill-session", "-t", session_id])
    path = _active_session_file()
    if path.exists():
        path.unlink()

def daemon_status() -> bool:
    result = _run_tmux(["info"], check=False)
    return result.returncode == 0

def daemon_stop():
    _run_tmux(["kill-server"], check=False)

def daemon_restart():
    daemon_stop()
    _run_tmux(["start-server"])
