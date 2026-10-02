import pytest
from agent_tui import tmux_backend

def test_press_valid_keys(monkeypatch):
    # Mock send_key so it doesn't actually call tmux
    called_keys = []
    def mock_send_key(key):
        called_keys.append(key)
        
    monkeypatch.setattr(tmux_backend, "send_key", mock_send_key)
    
    # Test a few valid keys
    tmux_backend.press("enter")
    assert called_keys[-1] == "Enter"
    
    tmux_backend.press("ctrl+c")
    assert called_keys[-1] == "C-c"
    
    tmux_backend.press("alt+i")
    assert called_keys[-1] == "M-i"

    tmux_backend.press("alt+\\")
    assert called_keys[-1] == "M-\\"

    tmux_backend.press("alt+|")
    assert called_keys[-1] == "M-|"
    
    tmux_backend.press("arrow_up")
    assert called_keys[-1] == "Up"
    
    tmux_backend.press("escape")
    assert called_keys[-1] == "Escape"

    tmux_backend.press("space")
    assert called_keys[-1] == "Space"

    tmux_backend.press("n")
    assert called_keys[-1] == "n"

    tmux_backend.press("7")
    assert called_keys[-1] == "7"

    tmux_backend.press("/")
    assert called_keys[-1] == "/"

def test_press_invalid_key():
    # Should return success: False and an error message for invalid key
    res1 = tmux_backend.press("invalid_key")
    assert res1["success"] is False
    assert "Invalid key" in res1["error"]
        
    res2 = tmux_backend.press("ctrl+1")
    assert res2["success"] is False
    assert "Invalid key" in res2["error"]

    res3 = tmux_backend.press("?")
    assert res3["success"] is False


def test_has_changed_when_cursor_moves(monkeypatch):
    cursor = iter([(24, 30), (23, 30), (23, 30)])

    def mock_hash(_session_id):
        return "same-screen"

    monkeypatch.setattr(tmux_backend, "_get_screen_hash", mock_hash)
    monkeypatch.setattr(tmux_backend, "_get_cursor_pos", lambda _sid: next(cursor))
    monkeypatch.setattr(tmux_backend, "get_active_session", lambda: "test")
    monkeypatch.setattr(tmux_backend, "send_key", lambda _key: None)

    res = tmux_backend.press("arrow_left")
    assert res["success"] is True
    assert res["has_changed"] is True


def test_get_cursor_pos_hidden(monkeypatch):
    class Result:
        stdout = "12,5,0"

    monkeypatch.setattr(tmux_backend, "_run_tmux", lambda _args: Result())
    assert tmux_backend._get_cursor_pos("sess") == (-1, -1)


def test_has_changed_false_when_nothing_moves(monkeypatch):
    monkeypatch.setattr(tmux_backend, "_get_screen_hash", lambda _sid: "same")
    monkeypatch.setattr(tmux_backend, "_get_cursor_pos", lambda _sid: (10, 5))
    monkeypatch.setattr(tmux_backend, "get_active_session", lambda: "test")
    monkeypatch.setattr(tmux_backend, "send_key", lambda _key: None)

    res = tmux_backend.press("arrow_left")
    assert res["success"] is True
    assert res["has_changed"] is False
