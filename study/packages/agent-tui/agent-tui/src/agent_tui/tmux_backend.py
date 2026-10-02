import os
import re
import time
import uuid
import hashlib
import subprocess
from pathlib import Path
from typing import Optional, List, Dict, Any

from .ansi_parser import to_plain, to_json, to_semantic, to_image

ACTIVE_SESSION_FILE = Path.home() / ".agent-tui" / "active_session"
try:
    ACTIVE_SESSION_FILE.parent.mkdir(parents=True, exist_ok=True)
except Exception:
    ACTIVE_SESSION_FILE = Path.cwd() / ".agent-tui" / "active_session"
    ACTIVE_SESSION_FILE.parent.mkdir(parents=True, exist_ok=True)

VALID_KEYS = {
    # Single-letter / digit hotkeys (e.g. kairo: n=new, q=quit)
    **{chr(i): chr(i) for i in range(ord('a'), ord('z') + 1)},
    **{chr(i): chr(i) for i in range(ord('0'), ord('9') + 1)},
    # Unshifted punctuation hotkeys (e.g. aptui: /=search)
    **{ch: ch for ch in ("/", ".", ",", "-", "=", "[", "]", "\\", ";", "'", "`")},
    # Ctrl keys
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

def _run_tmux(args: List[str], check: bool = True) -> subprocess.CompletedProcess:
    """Run a tmux command."""
    cmd = ["tmux"] + args
    return subprocess.run(cmd, capture_output=True, text=True, check=check)

def get_active_session() -> str:
    if not ACTIVE_SESSION_FILE.exists():
        raise RuntimeError("No active session. Please run `agent-tui start` or `agent-tui use` first.")
    return ACTIVE_SESSION_FILE.read_text().strip()

def set_active_session(session_id: str):
    ACTIVE_SESSION_FILE.parent.mkdir(parents=True, exist_ok=True)
    ACTIVE_SESSION_FILE.write_text(session_id)

def start(cmd: str, cwd: Optional[str] = None, label: Optional[str] = None, cols: int = 160, rows: int = 40) -> str:
    session_id = label if label else str(uuid.uuid4())[:8]
    
    # We use env command instead of tmux -e to set environment variables.
    # Older tmux versions (like 2.x on some Linux distros) do not support the -e flag for new-session.
    # By prefixing the command with `env TERM=xterm-256color COLORTERM=truecolor`, we achieve the same 
    # effect universally.
    env_prefix = "env TERM=xterm-256color COLORTERM=truecolor "
    full_cmd = f"{env_prefix}{cmd}"
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
        
    # Attach pipe-pane to capture OSC 4 palette redefinitions
    log_file = ACTIVE_SESSION_FILE.parent / f"{session_id}.log"
    _run_tmux(["pipe-pane", "-t", session_id, "-o", f"cat > {log_file}"])
    
    # Also tell tmux to support truecolor in this session
    _run_tmux(["set-option", "-t", session_id, "-g", "default-terminal", "tmux-256color"])
    _run_tmux(["set-option", "-t", session_id, "-ga", "terminal-overrides", ",xterm-256color:Tc"])
    
    set_active_session(session_id)
    return session_id

def use(session_id: str):
    # Verify session exists
    result = _run_tmux(["has-session", "-t", session_id], check=False)
    if result.returncode != 0:
        raise ValueError(f"Session '{session_id}' does not exist.")
    set_active_session(session_id)

def type_text(text: str):
    session_id = get_active_session()
    
    # Check for trailing \n or \t
    ends_with_enter = text.endswith('\n')
    ends_with_tab = text.endswith('\t')
    
    if ends_with_enter or ends_with_tab:
        text = text[:-1]
        
    if text:
        # -l for literal text
        _run_tmux(["send-keys", "-t", session_id, "-l", text])
        
    if ends_with_enter:
        _run_tmux(["send-keys", "-t", session_id, "Enter"])
    elif ends_with_tab:
        _run_tmux(["send-keys", "-t", session_id, "Tab"])

def send_key(key: str):
    session_id = get_active_session()
    _run_tmux(["send-keys", "-t", session_id, key])

def paste(text: str):
    session_id = get_active_session()
    # To paste multiple lines correctly, we can use set-buffer and paste-buffer
    _run_tmux(["set-buffer", text])
    _run_tmux(["paste-buffer", "-t", session_id])

def press(key: str):
    key_lower = key.lower()
    if key_lower not in VALID_KEYS:
        raise ValueError(f"Invalid key: '{key}'. Valid keys are: {', '.join(VALID_KEYS.keys())}")
    
    tmux_key = VALID_KEYS[key_lower]
    send_key(tmux_key)

def _get_cursor_pos(session_id: str) -> tuple:
    result = _run_tmux(["display-message", "-t", session_id, "-p", "#{cursor_x},#{cursor_y}"])
    try:
        x, y = map(int, result.stdout.strip().split(','))
        return x, y
    except Exception:
        return 0, 0

def _get_size(session_id: str) -> tuple:
    result = _run_tmux(["display-message", "-t", session_id, "-p", "#{pane_width},#{pane_height}"])
    try:
        w, h = map(int, result.stdout.strip().split(','))
        return w, h
    except Exception:
        return 120, 30

def snapshot(format: str = "plain", output_path: Optional[str] = None) -> Any:
    session_id = get_active_session()
    result = _run_tmux(["capture-pane", "-t", session_id, "-p", "-e"])
    ansi_text = result.stdout
    
    log_file = ACTIVE_SESSION_FILE.parent / f"{session_id}.log"
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
    
    if format == "plain":
        return to_plain(ansi_text)
    elif format == "json":
        cx, cy = _get_cursor_pos(session_id)
        return to_json(ansi_text, cx, cy, title=session_id)
    elif format == "semantic":
        return to_semantic(ansi_text, palette_overrides=palette_overrides)
    elif format in ["png", "jpg", "jpeg", "pdf", "svg"]:
        w, h = _get_size(session_id)
        return to_image(ansi_text, img_format=format, output_path=output_path, width=w, palette_overrides=palette_overrides)
    else:
        raise ValueError(f"Unknown format: {format}")

def scrollup(n: int):
    session_id = get_active_session()
    _run_tmux(["copy-mode", "-t", session_id])
    for _ in range(n):
        _run_tmux(["send-keys", "-t", session_id, "Up"])

def scrolldown(n: int):
    session_id = get_active_session()
    for _ in range(n):
        _run_tmux(["send-keys", "-t", session_id, "Down"])

def find(pattern: str) -> bool:
    content = snapshot("plain")
    return bool(re.search(pattern, content))

def wait(timeout: int = 3000, debounce: int = 100, text: Optional[str] = None) -> bool:
    session_id = get_active_session()
    start_time = time.time()
    timeout_sec = timeout / 1000.0
    debounce_sec = debounce / 1000.0
    
    last_hash = None
    
    while True:
        elapsed = time.time() - start_time
        if elapsed > timeout_sec:
            raise TimeoutError(f"Wait timeout after {timeout}ms")
        
        result = _run_tmux(["capture-pane", "-t", session_id, "-p", "-e"])
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
    if ACTIVE_SESSION_FILE.exists():
        ACTIVE_SESSION_FILE.unlink()

def daemon_status() -> bool:
    result = _run_tmux(["info"], check=False)
    return result.returncode == 0

def daemon_stop():
    _run_tmux(["kill-server"], check=False)

def daemon_restart():
    daemon_stop()
    _run_tmux(["start-server"])
