from __future__ import annotations

import shlex
import time
from pathlib import Path
from unittest.mock import MagicMock

from keyboard_agent.human.key_classifier import (
    US_SHIFT_SYMBOLS,
    is_press_turn_key,
    is_type_key,
)
from keyboard_agent.human.key_parser import (
    KeyParseState,
    key_name_to_raw_bytes,
    keystrokes_to_actions,
)
from keyboard_agent.human.session import HumanSession
from keyboard_agent.human.turn_recorder import HumanTurnRecorder


def test_type_vs_press_classification():
    assert is_type_key("a")
    assert is_type_key("5")
    assert is_type_key("/")
    assert is_type_key("?")
    assert is_type_key("!")
    assert is_type_key("@")
    assert not is_type_key("enter")
    assert is_press_turn_key("enter")
    assert is_press_turn_key("arrow_down")
    assert not is_press_turn_key("a")
    assert not is_press_turn_key("?")


def test_question_mark_parsed():
    state = KeyParseState()
    events = state.feed(b"?")
    assert len(events) == 1
    assert events[0].key_name == "?"
    assert events[0].raw_bytes == b"?"


def test_ctrl_slash_parsed_as_ctrl_underscore():
    state = KeyParseState()
    events = state.feed(b"\x1f")
    assert len(events) == 1
    assert events[0].key_name == "ctrl+_"
    assert events[0].raw_bytes == b"\x1f"


def test_turn_recorder_records_shift_punctuation():
    driver = MagicMock()
    driver.observe.return_value = MagicMock(
        mode="plain",
        text="screen",
        plain_text="screen",
        svg_file="x.svg",
        image_relative=None,
    )
    rec = HumanTurnRecorder(driver, run_dir=MagicMock(), type_idle_ms=1000.0)
    rec.handle_key("?")
    rec.handle_key("!")
    rec.flush()
    assert len(rec.operations) == 1
    assert rec.operations[0].kind == "type"
    assert rec.operations[0].to_action_dict()["text"] == "?!"


def test_printable_keys():
    state = KeyParseState()
    events = state.feed(b"abc")
    assert [e.key_name for e in events] == ["a", "b", "c"]
    assert all(e.is_printable for e in events)


def test_enter_and_arrow():
    state = KeyParseState()
    events = state.feed(b"\r\x1b[B")
    assert events[0].key_name == "enter"
    assert events[1].key_name == "arrow_down"


def test_application_cursor_arrows():
    state = KeyParseState()
    events = state.feed(b"\x1bOA\x1bOB\x1bOC\x1bOD")
    assert [event.key_name for event in events] == [
        "arrow_up",
        "arrow_down",
        "arrow_right",
        "arrow_left",
    ]


def test_f10_submit_sequence():
    state = KeyParseState()
    events = state.feed(b"\x1b[21~")
    assert len(events) == 1
    assert events[0].key_name == "f10"


def test_alt_backslash_and_pipe():
    state = KeyParseState()
    events = state.feed(b"\x1b\\\x1b|")
    assert [e.key_name for e in events] == ["alt+\\", "alt+|"]
    assert events[0].raw_bytes == b"\x1b\\"
    assert events[1].raw_bytes == b"\x1b|"


def test_alt_letter_meta_prefix():
    state = KeyParseState()
    events = state.feed(b"\x1bi")
    assert len(events) == 1
    assert events[0].key_name == "alt+i"
    assert events[0].raw_bytes == b"\x1bi"


def test_alt_key_raw_bytes():
    assert key_name_to_raw_bytes("alt+\\") == b"\x1b\\"
    assert key_name_to_raw_bytes("alt+|") == b"\x1b|"
    assert key_name_to_raw_bytes("alt+i") == b"\x1bi"


def test_turn_recorder_alt_press_turn():
    driver = MagicMock()
    driver.observe.return_value = MagicMock(
        mode="plain",
        text="screen",
        plain_text="screen",
        svg_file="x.svg",
        image_relative=None,
    )
    rec = HumanTurnRecorder(driver, run_dir=MagicMock(), type_idle_ms=1000.0)
    rec.handle_key("alt+\\", raw_bytes=b"\x1b\\")
    rec.flush()
    assert len(rec.operations) == 1
    assert rec.operations[0].kind == "press"
    assert rec.operations[0].to_action_dict()["key"] == "alt+\\"


def test_browser_alt_backslash():
    from keyboard_agent.human.browser_keys import browser_key_to_agent

    assert browser_key_to_agent({"key": "\\", "altKey": True}) == ("alt+\\", None)
    assert browser_key_to_agent({"key": "|", "altKey": True}) == ("alt+|", None)
    assert browser_key_to_agent({"key": "i", "altKey": True}) == ("alt+i", None)


def test_keystrokes_to_actions():
    ks = [
        {"key": "t", "pressed_at": "t", "interval_ms": None, "raw_bytes": "74"},
        {"key": "r", "pressed_at": "t", "interval_ms": 10, "raw_bytes": "72"},
        {"key": "enter", "pressed_at": "t", "interval_ms": 20, "raw_bytes": "0d"},
    ]
    actions = keystrokes_to_actions(ks)
    assert actions == [{"action": "type", "text": "tr\n"}]


def test_turn_recorder_type_and_press():
    driver = MagicMock()
    driver.observe.return_value = MagicMock(
        mode="plain",
        text="screen",
        plain_text="screen",
        svg_file="T01/turns/turn_001.svg",
        image_relative=None,
    )
    rec = HumanTurnRecorder(driver, run_dir=MagicMock(), type_idle_ms=1000.0)
    assert not rec.handle_key("h")
    assert not rec.handle_key("i")
    assert not rec.handle_key("enter")
    rec.flush()
    assert len(rec.operations) == 2
    assert rec.operations[0].kind == "type"
    assert rec.operations[0].to_action_dict()["action"] == "type"
    assert rec.operations[0].to_action_dict()["text"] == "hi"
    assert rec.operations[1].kind == "press"
    assert rec.operations[1].to_action_dict() == {
        "action": "press",
        "key": "enter",
        "human_keystrokes": rec.operations[1].to_action_dict()["human_keystrokes"],
        "gap_before_ms": rec.operations[1].to_action_dict()["gap_before_ms"],
    }


def test_turn_recorder_type_gap_splits_turns():
    driver = MagicMock()
    driver.observe.return_value = MagicMock(
        mode="plain",
        text="screen",
        plain_text="screen",
        svg_file="x.svg",
        image_relative=None,
    )
    rec = HumanTurnRecorder(driver, run_dir=MagicMock(), type_idle_ms=50.0)
    rec.handle_key("a")
    time.sleep(0.08)
    rec.handle_key("b")
    rec.flush()
    assert len(rec.operations) == 2
    assert all(op.kind == "type" for op in rec.operations)


def test_turn_recorder_submit_sequence_consumes_prefix():
    driver = MagicMock()
    logged: list[str] = []
    rec = HumanTurnRecorder(
        driver,
        run_dir=MagicMock(),
        type_idle_ms=1000.0,
        submit_key="ctrl+g ctrl+g ctrl+g",
        on_keystroke=lambda record: logged.append(record.key),
    )

    submitted, consumed = rec.handle_submit_candidate("ctrl+g", raw_bytes=b"\x07")
    assert not submitted
    assert consumed
    assert not rec.submitted
    assert logged == ["ctrl+g"]

    submitted, consumed = rec.handle_submit_candidate("ctrl+g", raw_bytes=b"\x07")
    assert not submitted
    assert consumed
    assert logged == ["ctrl+g", "ctrl+g"]

    submitted, consumed = rec.handle_submit_candidate("ctrl+g", raw_bytes=b"\x07")
    assert submitted
    assert consumed
    assert rec.submitted
    assert logged == ["ctrl+g", "ctrl+g", "ctrl+g"]


def test_ctrl_g_raw_bytes():
    assert key_name_to_raw_bytes("ctrl+g") == b"\x07"


def test_all_us_shift_symbols_parsed_and_recorded():
    """Every US Shift+punctuation symbol is a type key with raw bytes."""
    payload = "".join(US_SHIFT_SYMBOLS).encode("ascii")
    state = KeyParseState()
    events = state.feed(payload)
    assert [e.key_name for e in events] == list(US_SHIFT_SYMBOLS)
    for event, ch in zip(events, US_SHIFT_SYMBOLS, strict=True):
        assert event.raw_bytes == ch.encode("ascii")
        assert is_type_key(ch)
        assert not is_press_turn_key(ch)
        assert key_name_to_raw_bytes(ch) == ch.encode("ascii")

    driver = MagicMock()
    driver.observe.return_value = MagicMock(
        mode="plain",
        text="screen",
        plain_text="screen",
        svg_file="x.svg",
        image_relative=None,
    )
    rec = HumanTurnRecorder(driver, run_dir=MagicMock(), type_idle_ms=1000.0)
    for ch in US_SHIFT_SYMBOLS:
        rec.handle_key(ch, raw_bytes=ch.encode("ascii"))
    rec.flush()
    assert len(rec.operations) == 1
    assert rec.operations[0].kind == "type"
    assert rec.operations[0].to_action_dict()["text"] == "".join(US_SHIFT_SYMBOLS)
    for ks in rec.operations[0].keystrokes:
        assert ks.raw_bytes == ks.key.encode("ascii").hex()


def test_process_terminal_forwards_all_shift_symbols():
    import threading

    driver = MagicMock()
    session = HumanSession("bench-s-test", driver, run_dir=MagicMock())
    parser = KeyParseState()
    done = threading.Event()
    payload = "".join(US_SHIFT_SYMBOLS).encode("ascii")
    forwarded = session._process_terminal_input(payload, parser, done)
    assert forwarded == payload
    assert len(session.recorder.operations) == 0  # not finalized until idle/flush
    assert [ks.key for ks in session.recorder._type_buffer] == list(US_SHIFT_SYMBOLS)


def test_keystrokes_to_actions_shift_symbols():
    ks = [
        {"key": ch, "pressed_at": "t", "interval_ms": None, "raw_bytes": f"{ord(ch):02x}"}
        for ch in "!@#"
    ]
    assert keystrokes_to_actions(ks) == [{"action": "type", "text": "!@#"}]


def test_tmux_conf_key_escapes_special_chars():
    from keyboard_agent.human.session import _tmux_conf_key

    assert _tmux_conf_key('"') == r"\""
    assert _tmux_conf_key("#") == '"#"'
    assert _tmux_conf_key("-") == '"-"'
    assert _tmux_conf_key("%") == '"%"'
    assert _tmux_conf_key("{") == '"{"'
    assert _tmux_conf_key("~") == r"\~"
    assert _tmux_conf_key(";") == r"\;"
    assert _tmux_conf_key("M-\\") == "M-\\\\"
    assert _tmux_conf_key("M-|") == "M-|"


def test_literal_bind_shell_uses_safe_printf_and_hex_send():
    from pathlib import Path

    from keyboard_agent.human.session import _fifo_log_shell, _tmux_send_literal

    fifo = Path("/tmp/keys.fifo")
    for ch in "-%!=+\\":
        log = _fifo_log_shell(fifo, ch)
        send = _tmux_send_literal("%40", ch)
        assert "printf '" in log, ch
        assert "-l -" not in send, ch
        assert "-H" in send, ch
        assert ch.encode("utf-8").hex(" ") in send, ch
    assert "\\134" in _fifo_log_shell(fifo, "\\")


def test_normalize_key_preserves_uppercase():
    from keyboard_agent.human.turn_recorder import _normalize_key

    assert _normalize_key("A") == "A"
    assert _normalize_key("a") == "a"
    assert _normalize_key("ctrl+g") == "ctrl+g"


def test_uppercase_recorded_as_type_turn():
    driver = MagicMock()
    driver.observe.return_value = MagicMock(
        mode="plain",
        text="screen",
        plain_text="screen",
        svg_file="x.svg",
        image_relative=None,
    )
    logged: list[str] = []
    rec = HumanTurnRecorder(
        driver,
        run_dir=MagicMock(),
        on_keystroke=lambda r: logged.append(r.key),
    )
    rec.handle_key("A", raw_bytes=b"A")
    rec.handle_key("a", raw_bytes=b"a")
    rec.flush()
    assert logged == ["A", "a"]
    assert rec.operations[0].to_action_dict()["text"] == "Aa"


def test_submit_bind_logs_only_no_tmux_side_effects():
    from keyboard_agent.human.session import _fifo_log_shell

    log = _fifo_log_shell(Path("/tmp/f"), "ctrl+g")
    line = f"bind-key -n C-g run-shell -b {shlex.quote(log)}"
    assert "switch-client" not in line
    assert "detach-client" not in line
    assert "send-keys" not in line
    assert "__SUBMIT__" not in line


def test_all_us_keyboard_symbols_have_conf_escape():
    from keyboard_agent.human.session import _tmux_bind_key, _tmux_conf_key

    for ch in US_SHIFT_SYMBOLS:
        conf_key = _tmux_conf_key(_tmux_bind_key(ch))
        assert conf_key, ch
    for ch in "-=[]\\;',./":
        conf_key = _tmux_conf_key(_tmux_bind_key(ch))
        assert conf_key, ch


def test_turn_recorder_single_submit_key_still_supported():
    driver = MagicMock()
    rec = HumanTurnRecorder(driver, run_dir=MagicMock(), submit_key="f10")

    assert rec.handle_key("f10")
    assert rec.submitted
