from agent_tui import ansi_parser

def test_to_plain():
    ansi_text = "\x1b[31mError\x1b[0m message"
    plain = ansi_parser.to_plain(ansi_text)
    assert plain == "Error message"

def test_to_semantic_simple_color():
    ansi_text = "\x1b[31mError\x1b[0m"
    semantic = ansi_parser.to_semantic(ansi_text)
    assert semantic == "<fg:red>Error</fg:red>"

def test_to_semantic_bg_and_fg():
    # 31 is red fg, 44 is blue bg
    ansi_text = "\x1b[31;44mError\x1b[0m"
    semantic = ansi_parser.to_semantic(ansi_text)
    assert semantic == "<fg:red><bg:blue>Error</bg:blue></fg:red>"

def test_to_semantic_nested_reset():
    # A bit more complex
    ansi_text = "\x1b[32mSuccess \x1b[31mError\x1b[0m"
    semantic = ansi_parser.to_semantic(ansi_text)
    assert semantic == "<fg:green>Success </fg:green><fg:red>Error</fg:red>"

def test_to_semantic_multiple_text():
    ansi_text = "Start \x1b[31mRed\x1b[0m End"
    semantic = ansi_parser.to_semantic(ansi_text)
    assert semantic == "Start <fg:red>Red</fg:red> End"

def test_to_semantic_no_reset():
    ansi_text = "\x1b[31mRedText"
    semantic = ansi_parser.to_semantic(ansi_text)
    assert semantic == "<fg:red>RedText</fg:red>"
