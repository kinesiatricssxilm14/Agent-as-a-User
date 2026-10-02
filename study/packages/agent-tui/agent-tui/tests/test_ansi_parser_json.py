from agent_tui import ansi_parser
import json

def test_to_json():
    ansi_text = "\x1b[31mLine 1\x1b[0m\nLine 2"
    json_str = ansi_parser.to_json(ansi_text, 5, 10, title="Test")
    data = json.loads(json_str)
    
    assert data["title"] == "Test"
    assert data["cursor"]["x"] == 5
    assert data["cursor"]["y"] == 10
    assert data["lines"] == ["Line 1", "Line 2"]
