import re
import json
import subprocess
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

def to_json(ansi_text: str, cursor_x: int, cursor_y: int, title: str = "") -> str:
    plain_text = to_plain(ansi_text)
    lines = plain_text.splitlines()
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

def to_image(ansi_text: str, img_format: str = "png", output_path: str = None, width: int = 120, palette_overrides: dict = None) -> str:
    palette_overrides = palette_overrides or {}
    if not output_path:
        output_path = f"snapshot.{img_format}"
        
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
        force_terminal=True, 
        color_system="truecolor",
        no_color=False
    )
    console.print(Text.from_ansi(ansi_text))
        
    if img_format == "svg":
        console.save_svg(output_path, title="Terminal Snapshot", theme=vscode_dark_theme)
        return output_path

    # For other formats, we use rich to generate high-fidelity HTML, then convert via imgkit
    html = console.export_html(inline_styles=True, theme=vscode_dark_theme)
    
    # Use imgkit to convert HTML to target format
    options = {
        'format': img_format,
        'quiet': ''
    }
    try:
        imgkit.from_string(html, output_path, options=options)
        return output_path
    except Exception as e:
        error_str = str(e)
        if "No wkhtmltoimage executable found" in error_str or "command not found" in error_str:
            raise RuntimeError(
                f"Failed to generate {img_format.upper()}. The underlying system dependency 'wkhtmltopdf' is missing.\n"
                "Please install it via your system package manager:\n"
                "  Ubuntu/Debian: sudo apt-get install wkhtmltopdf\n"
                "  CentOS/RHEL:   sudo yum install wkhtmltopdf\n"
                "  macOS:         brew install wkhtmltopdf\n"
                "Alternatively, try '--format svg' which requires no system dependencies."
            )
        raise RuntimeError(f"Failed to generate image. Error: {e}")
