from keyboard_agent.human.session import (
    _display_width,
    _truncate_display_text,
    _wrap_display_text,
)


def test_wrap_respects_cjk_display_width():
    text = "English-only textsnippet：English-only text（title）English-only text git-log-pretty"
    lines = _wrap_display_text(text, width=40)
    assert lines
    assert all(_display_width(line) <= 40 for line in lines)


def test_p08_t02_three_lines_fit_terminal_width():
    text = (
        "English-only textsnippet：English-only text（title）English-only text git-log-pretty，English-only text（language）English-only text bash，"
        "English-only text（description）English-only text pretty git log-{{file_content}}，English-only text（tags）English-only text git，"
        "English-only text（code）English-only text git log --oneline --graph，English-only text。English-only textsnippet："
        "English-only text（title）English-only text batch-install-dependency，English-only text（language）English-only text python，"
        "English-only text（description）English-only text download and install all dependencies-{{file_content}}，"
        "English-only text（tags）English-only text environment，English-only text（code）English-only text pip install -r requirements.txt，English-only text。"
        "English-only text git-log-pretty English-only text batch-install-python，English-only text，English-only text "
        "batch-install-python，English-only text，English-only text（title）、English-only text（language）、"
        "English-only text（description）、English-only text（tags）、English-only text（code）。"
    )
    text = " ".join(text.split())
    width = 78
    lines = _wrap_display_text(text, width=width)[:3]
    if len(_wrap_display_text(text, width=width)) > 3:
        last = lines[-1]
        if _display_width(last) + _display_width("...") <= width:
            lines[-1] = last + "..."
        else:
            lines[-1] = _truncate_display_text(lines[-1], width=width, suffix="...")

    assert len(lines) == 3
    assert all(_display_width(line) <= width for line in lines)
    assert lines[-1].endswith("...")
