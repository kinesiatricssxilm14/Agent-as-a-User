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

def test_press_invalid_key():
    # Should raise ValueError for invalid key
    with pytest.raises(ValueError, match="Invalid key"):
        tmux_backend.press("invalid_key")
        
    with pytest.raises(ValueError, match="Invalid key"):
        tmux_backend.press("ctrl+1")
