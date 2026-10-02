import os
import re
import json
import subprocess
import tempfile
from pathlib import Path
from typing import Optional
from ansi2html import Ansi2HTMLConverter
import imgkit

# We use a comprehensive regex to parse all SGR codes including 256 colors and TrueColor
# \x1b\[ (parameter;) m
ANSI_ESCAPE_RE = re.compile(r'\x1b\[([0-9;]*)([A-Za-z])')

ANSI_COLORS = {
    '30': 'black', '31': 'red', '32': 'green', '33': 'yellow',
    '34': 'blue', '35': 'magenta', '36': 'cyan', '37': 'white',
    '90': 'gray', '91': 'light_red', '92': 'light_green', '93': 'light_yellow',
    '94': 'light_blue', '95': 'light_magenta', '96': 'light_cyan', '97': 'light_white'
}
ANSI_BG_COLORS = {
    '40': 'black', '41': 'red', '42': 'green', '43': 'yellow',
    '44': 'blue', '45': 'magenta', '46': 'cyan', '47': 'white',
    '100': 'gray', '101': 'light_red', '102': 'light_green', '103': 'light_yellow',
    '104': 'light_blue', '105': 'light_magenta', '106': 'light_cyan', '107': 'light_white'
}

def strip_ansi(text: str) -> str:
    return ANSI_ESCAPE_RE.sub('', text)

def to_plain(ansi_text: str) -> str:
    return strip_ansi(ansi_text)


CURSOR_MARKER = "\u2588"  # FULL BLOCK — marks the cell under the text cursor


def _mark_cursor_in_line(line: str, cursor_x: int) -> str:
    """Replace the cell under the cursor with a block character (█).

    Same rule for plain, semantic, and json line fields. End-of-line empty
    cell appends █ after the line content.
    """
    if cursor_x < 0:
        return line
    if cursor_x >= len(line):
        return line + CURSOR_MARKER
    return line[:cursor_x] + CURSOR_MARKER + line[cursor_x + 1 :]


def annotate_cursor_plain(plain_text: str, cursor_x: int, cursor_y: int) -> str:
    lines = plain_text.splitlines()
    if 0 <= cursor_y < len(lines):
        lines[cursor_y] = _mark_cursor_in_line(lines[cursor_y], cursor_x)
    return "\n".join(lines)


def annotate_cursor_semantic(semantic_text: str, cursor_x: int, cursor_y: int) -> str:
    lines = semantic_text.splitlines()
    if 0 <= cursor_y < len(lines):
        lines[cursor_y] = _mark_cursor_in_semantic_line(lines[cursor_y], cursor_x)
    return "\n".join(lines)


def _mark_cursor_in_semantic_line(line: str, cursor_x: int) -> str:
    """Mark cursor column in one semantic line (plain chars + XML tags)."""
    visible: list[tuple[int, int]] = []
    i = 0
    while i < len(line):
        if line.startswith("<fg:", i) or line.startswith("<bg:", i):
            end = line.find(">", i)
            if end == -1:
                break
            i = end + 1
            continue
        if line.startswith("</fg:", i) or line.startswith("</bg:", i):
            end = line.find(">", i)
            if end == -1:
                break
            i = end + 1
            continue
        visible.append((i, i + 1))
        i += 1

    if cursor_x < 0:
        return line
    if cursor_x >= len(visible):
        return line + CURSOR_MARKER
    start, end = visible[cursor_x]
    return line[:start] + CURSOR_MARKER + line[end:]


def inject_cursor_into_svg(svg_path: str, cursor_x: int, cursor_y: int) -> None:
    """Overlay a block cursor on a Rich terminal SVG export."""
    if cursor_x < 0 or cursor_y < 0:
        return

    content = Path(svg_path).read_text(encoding="utf-8")
    baseline_y = None
    line_y = None
    line_h = None

    text_match = re.search(
        rf'<text[^>]*clip-path="url\(#([^"]+-line-{cursor_y})\)"[^>]*y="([0-9.]+)"',
        content,
    )
    if text_match:
        baseline_y = float(text_match.group(2))
    else:
        text_match = re.search(
            rf'<text[^>]*y="([0-9.]+)"[^>]*clip-path="url\(#([^"]+-line-{cursor_y})\)"',
            content,
        )
        if text_match:
            baseline_y = float(text_match.group(1))

    line_match = re.search(
        rf'<clipPath id="[^"]+-line-{cursor_y}">\s*<rect x="0" y="([0-9.]+)" width="[0-9.]+" height="([0-9.]+)"',
        content,
    )
    if line_match:
        line_y = float(line_match.group(1))
        line_h = float(line_match.group(2))

    if baseline_y is None and line_y is None:
        return

    cell_w = 12.2
    cell_w_match = re.search(r'textLength="12\.2"', content)
    if not cell_w_match:
        width_match = re.search(
            r'<clipPath id="[^"]+-clip-terminal">\s*<rect x="0" y="0" width="([0-9.]+)"',
            content,
        )
        if width_match:
            cell_w = float(width_match.group(1)) / max(1, 120)

    font_size_match = re.search(r"font-size:\s*([0-9.]+)px", content)
    font_size = float(font_size_match.group(1)) if font_size_match else 20.0

    # Rich places text inside translate(...); the cursor rect is inserted there too.
    cx_px = cursor_x * cell_w
    if baseline_y is not None:
        cy_px = baseline_y - font_size
    else:
        cy_px = line_y + max(0.0, (line_h - font_size) / 2)

    cursor_rect = (
        f'<rect class="agent-tui-cursor" x="{cx_px:.1f}" y="{cy_px:.1f}" '
        f'width="{cell_w:.1f}" height="{font_size:.1f}" '
        f'fill="#cccccc" fill-opacity="0.45" stroke="#cccccc" stroke-width="1"/>'
    )

    insert_point = content.rfind("    </g>\n    </g>")
    if insert_point == -1:
        insert_point = content.rfind("</g>\n</svg>")
        if insert_point == -1:
            return
        cursor_rect = cursor_rect + "\n    "
    Path(svg_path).write_text(
        content[:insert_point] + cursor_rect + content[insert_point:],
        encoding="utf-8",
    )


def to_json(ansi_text: str, cursor_x: int, cursor_y: int, title: str = "") -> str:
    plain_text = to_plain(ansi_text)
    lines = plain_text.splitlines()
    if 0 <= cursor_y < len(lines):
        lines[cursor_y] = _mark_cursor_in_line(lines[cursor_y], cursor_x)
    data = {
        "title": title,
        "cursor": {"x": cursor_x, "y": cursor_y},
        "lines": lines
    }
    return json.dumps(data, indent=2)

def to_semantic(ansi_text: str, palette_overrides: dict = None) -> str:
    palette_overrides = palette_overrides or {}
    result = ""
    current_fg = None
    current_bg = None
    
    last_end = 0
    for match in ANSI_ESCAPE_RE.finditer(ansi_text):
        result += ansi_text[last_end:match.start()]
        
        params = match.group(1)
        command = match.group(2)
        
        if command == 'm':
            codes = params.split(';') if params else ['0']
            i = 0
            while i < len(codes):
                code = codes[i]
                if not code: code = '0'
                if code == '0':
                    if current_bg:
                        result += f"</bg:{current_bg}>"
                        current_bg = None
                    if current_fg:
                        result += f"</fg:{current_fg}>"
                        current_fg = None
                elif code in ANSI_COLORS:
                    if current_fg:
                        result += f"</fg:{current_fg}>"
                    current_fg = ANSI_COLORS[code]
                    result += f"<fg:{current_fg}>"
                elif code in ANSI_BG_COLORS:
                    if current_bg:
                        result += f"</bg:{current_bg}>"
                    current_bg = ANSI_BG_COLORS[code]
                    result += f"<bg:{current_bg}>"
                elif code == '38': # Extended fg color
                    if i + 2 < len(codes) and codes[i+1] == '5': # 256 color
                        idx = int(codes[i+2])
                        if current_fg:
                            result += f"</fg:{current_fg}>"
                        if idx in palette_overrides:
                            r, g, b = palette_overrides[idx]
                            current_fg = f"rgb_{r}_{g}_{b}"
                        else:
                            current_fg = f"color_{idx}"
                        result += f"<fg:{current_fg}>"
                        i += 2
                    elif i + 4 < len(codes) and codes[i+1] == '2': # truecolor
                        if current_fg:
                            result += f"</fg:{current_fg}>"
                        current_fg = f"rgb_{codes[i+2]}_{codes[i+3]}_{codes[i+4]}"
                        result += f"<fg:{current_fg}>"
                        i += 4
                elif code == '48': # Extended bg color
                    if i + 2 < len(codes) and codes[i+1] == '5':
                        idx = int(codes[i+2])
                        if current_bg:
                            result += f"</bg:{current_bg}>"
                        if idx in palette_overrides:
                            r, g, b = palette_overrides[idx]
                            current_bg = f"rgb_{r}_{g}_{b}"
                        else:
                            current_bg = f"color_{idx}"
                        result += f"<bg:{current_bg}>"
                        i += 2
                    elif i + 4 < len(codes) and codes[i+1] == '2':
                        if current_bg:
                            result += f"</bg:{current_bg}>"
                        current_bg = f"rgb_{codes[i+2]}_{codes[i+3]}_{codes[i+4]}"
                        result += f"<bg:{current_bg}>"
                        i += 4
                i += 1
        
        last_end = match.end()
        
    result += ansi_text[last_end:]
    
    if current_bg:
        result += f"</bg:{current_bg}>"
    if current_fg:
        result += f"</fg:{current_fg}>"
        
    return result

def _convert_svg_to_raster(svg_path: str, output_path: str):
    """Convert SVG to png/jpg/pdf using whichever system tool is available."""
    import shutil

    ext = os.path.splitext(output_path)[1].lower().lstrip(".") or "png"
    if shutil.which("rsvg-convert"):
        cmd = ["rsvg-convert", "-o", output_path, svg_path]
        if ext != "png":
            cmd = ["rsvg-convert", "-f", ext, "-o", output_path, svg_path]
        subprocess.run(cmd, check=True, capture_output=True)
        return "rsvg-convert"
    if shutil.which("magick"):
        subprocess.run(["magick", svg_path, output_path], check=True, capture_output=True)
        return "magick"
    if shutil.which("wkhtmltoimage"):
        subprocess.run(
            ["wkhtmltoimage", svg_path, output_path],
            check=True,
            capture_output=True,
        )
        return "wkhtmltoimage"
    return None


def _raster_via_svg(
    console,
    output_path: str,
    img_format: str,
    theme,
    cursor_x: Optional[int] = None,
    cursor_y: Optional[int] = None,
) -> str:
    """Fallback: Rich SVG export → system rasterizer (no wkhtmltopdf HTML path)."""
    with tempfile.NamedTemporaryFile(suffix=".svg", delete=False) as tmp:
        svg_path = tmp.name
    try:
        console.save_svg(svg_path, title="Terminal Snapshot", theme=theme)
        if cursor_x is not None and cursor_y is not None:
            inject_cursor_into_svg(svg_path, cursor_x, cursor_y)
        tool = _convert_svg_to_raster(svg_path, output_path)
        if tool:
            return output_path
        raise RuntimeError("no rasterizer")
    finally:
        try:
            os.unlink(svg_path)
        except OSError:
            pass


def _missing_rasterizer_error(img_format: str) -> RuntimeError:
    return RuntimeError(
        f"Failed to generate {img_format.upper()}. No rasterizer found.\n"
        "Install one of:\n"
        "  macOS (recommended):  brew install librsvg\n"
        "  macOS (alternative):  brew install imagemagick\n"
        "  Ubuntu/Debian:        sudo apt-get install librsvg2-bin\n"
        "  Legacy HTML path:     install wkhtmltopdf (discontinued; not in Homebrew)\n"
        "Or use '--format svg' which needs no extra system dependencies."
    )

def to_image(
    ansi_text: str,
    img_format: str = "png",
    output_path: str = None,
    width: int = 120,
    height: int = 30,
    palette_overrides: dict = None,
    cursor_x: Optional[int] = None,
    cursor_y: Optional[int] = None,
) -> str:
    palette_overrides = palette_overrides or {}
    if not output_path:
        output_path = f"snapshot.{img_format}"
        
    from io import StringIO
    from rich.console import Console
    from rich.text import Text
    import rich.terminal_theme as rt
    
    # Custom VS Code Dark Theme for better visual fidelity
    vscode_dark_theme = rt.TerminalTheme(
        background=(30, 30, 30),        # #1e1e1e
        foreground=(204, 204, 204),     # #cccccc
        normal=[
            (0, 0, 0),         # black
            (205, 49, 49),     # red
            (13, 188, 121),    # green
            (229, 229, 16),    # yellow
            (36, 114, 200),    # blue
            (188, 63, 188),    # magenta
            (17, 168, 205),    # cyan
            (229, 229, 229),   # white
        ],
        bright=[
            (102, 102, 102),   # bright black
            (241, 76, 76),     # bright red
            (35, 209, 139),    # bright green
            (245, 245, 67),    # bright yellow
            (59, 142, 234),    # bright blue
            (214, 112, 214),   # bright magenta
            (41, 184, 219),    # bright cyan
            (229, 229, 229),   # bright white
        ]
    )
    
    # Pre-process ANSI text to replace dynamically overridden 256 colors with direct TrueColor RGB.
    # This ensures `rich` doesn't fall back to the standard palette for overridden colors.
    for idx, (r, g, b) in palette_overrides.items():
        # Replace foreground color
        ansi_text = re.sub(fr'\x1b\[38;5;{idx}m', f'\x1b[38;2;{r};{g};{b}m', ansi_text)
        # Replace background color
        ansi_text = re.sub(fr'\x1b\[48;5;{idx}m', f'\x1b[48;2;{r};{g};{b}m', ansi_text)

    # We use a dark theme which is much more pleasant than default ansi2html
    # record=True enables exporting, force_terminal=True ensures ANSI gets parsed correctly
    # IMPORTANT: We explicitly force `color_system="truecolor"` and set `no_color=False`
    # to prevent `rich` from downgrading colors when running in a headless CI/CD or Agent environment.
    console = Console(
        record=True, 
        width=width, 
        height=height,
        force_terminal=True, 
        color_system="truecolor",
        no_color=False,
        file=StringIO(),
    )
    console.print(Text.from_ansi(ansi_text))
        
    if img_format == "svg":
        console.save_svg(output_path, title="Terminal Snapshot", theme=vscode_dark_theme)
        if cursor_x is not None and cursor_y is not None:
            inject_cursor_into_svg(output_path, cursor_x, cursor_y)
        return output_path

    # Prefer SVG → rsvg-convert (works without discontinued wkhtmltopdf).
    # Must run before export_html(), which clears Rich's record buffer.
    try:
        return _raster_via_svg(
            console, output_path, img_format, vscode_dark_theme, cursor_x, cursor_y
        )
    except Exception:
        pass

    # Legacy HTML → wkhtmltoimage path
    html = console.export_html(inline_styles=True, theme=vscode_dark_theme)
    options = {
        'format': img_format,
        'quiet': ''
    }
    try:
        imgkit.from_string(html, output_path, options=options)
        return output_path
    except Exception as e:
        error_str = str(e)
        wkhtml_missing = (
            "No wkhtmltoimage executable found" in error_str
            or "command not found" in error_str
            or "wkhtmltoimage" in error_str.lower()
        )
        if wkhtml_missing:
            raise _missing_rasterizer_error(img_format) from e
        raise RuntimeError(f"Failed to generate image. Error: {e}")
