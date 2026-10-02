import click
import json
import sys
from . import tmux_backend

@click.group()
def cli():
    """Agent TUI: Bridge between LLM Agents and TUI via Tmux."""
    pass

def output_json(success: bool, has_changed: bool, data: any = None, error: str = None, system_mutation: any = None):
    """Helper to consistently output JSON responses."""
    result = {
        "success": success,
        "has_changed": has_changed
    }
    if system_mutation is not None:
        result["system_mutation"] = system_mutation
    if data is not None:
        result["data"] = data
    if error is not None:
        result["error"] = error
        
    click.echo(json.dumps(result, indent=2, ensure_ascii=False))
    
    if not success:
        sys.exit(1)

@cli.command()
@click.option('--cwd', type=click.Path(), help='Start in specific directory')
@click.option('--label', type=str, help='Start with label (session id)')
@click.option('--cols', type=int, default=160, help='Custom terminal width')
@click.option('--rows', type=int, default=40, help='Custom terminal height')
@click.argument('cmd', nargs=-1, required=True)
def start(cwd, label, cols, rows, cmd):
    """Start a program in a new tmux session."""
    cmd_str = " ".join(cmd)
    try:
        session_id = tmux_backend.start(cmd_str, cwd=cwd, label=label, cols=cols, rows=rows)
        output_json(True, True, {"session_id": session_id})
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
@click.argument('session_id')
def use(session_id):
    """Switch to a session (save to local state)."""
    try:
        tmux_backend.use(session_id)
        output_json(True, False, {"session_id": session_id})
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command(name="type")
@click.argument('text')
def type_cmd(text):
    """Type literal text."""
    try:
        res = tmux_backend.type_text(text)
        output_json(res["success"], res["has_changed"], error=res.get("error"), system_mutation=res.get("system_mutation"))
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
@click.argument('text')
def paste(text):
    """Multi-line paste."""
    try:
        res = tmux_backend.paste(text)
        output_json(res["success"], res["has_changed"], error=res.get("error"), system_mutation=res.get("system_mutation"))
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
@click.argument('key')
def press(key):
    """Press a special key."""
    try:
        res = tmux_backend.press(key)
        output_json(res["success"], res["has_changed"], error=res.get("error"), system_mutation=res.get("system_mutation"))
    except Exception as e:
        output_json(False, False, error=str(e))

FORMAT_CHOICES = ['plain', 'json', 'semantic', 'png', 'jpg', 'jpeg', 'pdf', 'svg']

@cli.command()
@click.option('--format', type=click.Choice(FORMAT_CHOICES), default='plain', help='Output format')
@click.option('--output', type=str, help='Output file path for images')
def snapshot(format, output):
    """Get current screen."""
    try:
        result = tmux_backend.snapshot(format=format, output_path=output)
        output_json(True, False, {"snapshot": result})
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
@click.argument('n', type=int)
def scrollup(n):
    """Scroll up to older content."""
    try:
        res = tmux_backend.scrollup(n)
        output_json(res["success"], res["has_changed"], error=res.get("error"))
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
@click.argument('n', type=int)
def scrolldown(n):
    """Scroll down to newer content."""
    try:
        res = tmux_backend.scrolldown(n)
        output_json(res["success"], res["has_changed"], error=res.get("error"))
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
@click.argument('pattern')
def find(pattern):
    """Search in screen (regex)."""
    try:
        found = tmux_backend.find(pattern)
        output_json(True, False, {"found": found})
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
@click.argument('ms', type=int, required=False, default=3000)
@click.option('--text', type=str, help='Wait until screen contains pattern')
@click.option('--debounce', type=int, default=100, help='Idle time after last change before resolving')
@click.option('--format', type=click.Choice(FORMAT_CHOICES), help='Output after waiting')
@click.option('--output', type=str, help='Output file path for images')
def wait(ms, text, debounce, format, output):
    """Wait for screen change."""
    try:
        success = tmux_backend.wait(timeout=ms, debounce=debounce, text=text)
        data = {}
        if format:
            result = tmux_backend.snapshot(format=format, output_path=output)
            data["snapshot"] = result
        output_json(True, True, data)
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command(name="list")
def list_cmd():
    """List all sessions."""
    try:
        sessions = tmux_backend.list_sessions()
        output_json(True, False, {"sessions": sessions})
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
def info():
    """Show session details."""
    try:
        info_data = tmux_backend.info()
        output_json(True, False, {"info": info_data})
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
@click.argument('label')
def rename(label):
    """Rename session."""
    try:
        tmux_backend.rename(label)
        output_json(True, False, {"new_label": label})
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.command()
def kill():
    """Kill current session."""
    try:
        tmux_backend.kill()
        output_json(True, False)
    except Exception as e:
        output_json(False, False, error=str(e))

@cli.group()
def daemon():
    """Daemon management commands."""
    pass

@daemon.command()
def status():
    """Check if daemon (tmux server) is running."""
    is_running = tmux_backend.daemon_status()
    output_json(True, False, {"is_running": is_running})

@daemon.command()
def stop():
    """Stop the daemon (tmux kill-server)."""
    try:
        tmux_backend.daemon_stop()
        output_json(True, False)
    except Exception as e:
        output_json(False, False, error=str(e))

@daemon.command()
def restart():
    """Restart the daemon."""
    try:
        tmux_backend.daemon_restart()
        output_json(True, False)
    except Exception as e:
        output_json(False, False, error=str(e))

if __name__ == '__main__':
    cli()