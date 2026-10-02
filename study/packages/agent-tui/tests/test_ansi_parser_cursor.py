import json
from pathlib import Path

from agent_tui import ansi_parser


def test_mark_cursor_in_line():
    assert ansi_parser._mark_cursor_in_line("hello", 1) == "h█llo"
    assert ansi_parser._mark_cursor_in_line("ab", 2) == "ab█"


def test_annotate_cursor_plain():
    plain = "line0\nline1"
    marked = ansi_parser.annotate_cursor_plain(plain, 2, 1)
    assert marked == "line0\nli█e1"


def test_annotate_cursor_semantic():
    semantic = "<fg:red>ab</fg:red>c"
    marked = ansi_parser.annotate_cursor_semantic(semantic, 1, 0)
    assert "<cursor" not in marked
    assert "a█" in marked


def test_to_json_marks_cursor_line():
    json_str = ansi_parser.to_json("abc\ndef", 1, 1)
    data = json.loads(json_str)
    assert data["cursor"] == {"x": 1, "y": 1}
    assert data["lines"][1] == "d█f"


def test_inject_cursor_into_svg(tmp_path):
    svg = """<svg xmlns="http://www.w3.org/2000/svg">
    <style>.terminal-1-matrix { font-size: 20px; line-height: 24.4px; }</style>
    <defs>
    <clipPath id="terminal-1-line-1">
    <rect x="0" y="25.9" width="1464" height="24.65"/>
    </clipPath>
    </defs>
    <g transform="translate(9, 41)" clip-path="url(#terminal-1-clip-terminal)">
    <g class="terminal-1-matrix">
    <text x="0" y="44.4" textLength="12.2" clip-path="url(#terminal-1-line-1)">x</text>
    </g>
    </g>
</svg>"""
    svg_path = tmp_path / "snap.svg"
    svg_path.write_text(svg, encoding="utf-8")
    ansi_parser.inject_cursor_into_svg(str(svg_path), 3, 1)
    out = svg_path.read_text(encoding="utf-8")
    assert 'class="agent-tui-cursor"' in out
    assert 'x="36.6"' in out
    assert 'y="24.4"' in out


def test_to_image_svg_includes_cursor(tmp_path):
    ansi = "hello\nworld"
    out = tmp_path / "snap.svg"
    ansi_parser.to_image(ansi, img_format="svg", output_path=str(out), width=10, cursor_x=2, cursor_y=1)
    content = out.read_text(encoding="utf-8")
    assert 'class="agent-tui-cursor"' in content
