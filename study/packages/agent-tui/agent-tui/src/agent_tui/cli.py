import click
import json
import sys
from . import tmux_backend

@click.group()
def cli():
    """Agent TUI: Bridge between LLM Agents and TUI via Tmux."""
    pass

@cli.command()
@click.option('--cwd', type=click.Path(), help='Start in specific directory')
@click.option('--label', type=str, help='Start with label (session id)')
@click.option('--cols', type=int, default=120, help='Custom terminal width')
@click.option('--rows', type=int, default=30, help='Custom terminal height')
@click.argument('cmd', nargs=-1, required=True)
def start(cwd, label, cols, rows, cmd):
    """Start a program in a new tmux session."""
    cmd_str = " ".join(cmd)
    try:
        session_id = tmux_backend.start(cmd_str, cwd=cwd, label=label, cols=cols, rows=rows)
        click.echo(f"Started session: {session_id}")
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
@click.argument('session_id')
def use(session_id):
    """Switch to a session (save to local state)."""
    try:
        tmux_backend.use(session_id)
        click.echo(f"Using session: {session_id}")
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command(name="type")
@click.argument('text')
def type_cmd(text):
    """Type literal text."""
    # Click might unescape \n in arguments depending on shell, but assuming literal \n is passed
    # we can process it directly. But if user passes "\n" as actual characters from bash, 
    # we need to handle it. Actually python gets actual newline if bash interprets it.
    try:
        tmux_backend.type_text(text)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
@click.argument('text')
def paste(text):
    """Multi-line paste."""
    try:
        tmux_backend.paste(text)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
@click.argument('key')
def press(key):
    """Press a special key."""
    try:
        tmux_backend.press(key)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

FORMAT_CHOICES = ['plain', 'json', 'semantic', 'png', 'jpg', 'jpeg', 'pdf', 'svg']

@cli.command()
@click.option('--format', type=click.Choice(FORMAT_CHOICES), default='plain', help='Output format')
@click.option('--output', type=str, help='Output file path for images')
def snapshot(format, output):
    """Get current screen."""
    try:
        result = tmux_backend.snapshot(format=format, output_path=output)
        if format in ['png', 'jpg', 'jpeg', 'pdf', 'svg']:
            click.echo(f"Saved snapshot to {result}")
        else:
            click.echo(result)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
@click.argument('n', type=int)
def scrollup(n):
    """Scroll up to older content."""
    try:
        tmux_backend.scrollup(n)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
@click.argument('n', type=int)
def scrolldown(n):
    """Scroll down to newer content."""
    try:
        tmux_backend.scrolldown(n)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
@click.argument('pattern')
def find(pattern):
    """Search in screen (regex)."""
    try:
        found = tmux_backend.find(pattern)
        if found:
            click.echo("Pattern found")
            sys.exit(0)
        else:
            click.echo("Pattern not found")
            sys.exit(1)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
@click.argument('ms', type=int, required=False, default=3000)
@click.option('--text', type=str, help='Wait until screen contains pattern')
@click.option('--debounce', type=int, default=100, help='Idle time after last change before resolving')
@click.option('--format', type=click.Choice(FORMAT_CHOICES), help='Output after waiting')
@click.option('--output', type=str, help='Output file path for images')
def wait(ms, text, debounce, format, output):
    """Wait for screen change."""
    try:
        tmux_backend.wait(timeout=ms, debounce=debounce, text=text)
        if format:
            result = tmux_backend.snapshot(format=format, output_path=output)
            if format in ['png', 'jpg', 'jpeg', 'pdf', 'svg']:
                click.echo(f"Saved snapshot to {result}")
            else:
                click.echo(result)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command(name="list")
def list_cmd():
    """List all sessions."""
    try:
        click.echo(tmux_backend.list_sessions())
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
def info():
    """Show session details."""
    try:
        click.echo(tmux_backend.info())
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
@click.argument('label')
def rename(label):
    """Rename session."""
    try:
        tmux_backend.rename(label)
        click.echo(f"Renamed to: {label}")
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.command()
def kill():
    """Kill current session."""
    try:
        tmux_backend.kill()
        click.echo("Killed current session")
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)

@cli.group()
def daemon():
    """Daemon management commands."""
    pass

@daemon.command()
def status():
    """Check if daemon (tmux server) is running."""
    is_running = tmux_backend.daemon_status()
    click.echo(f"Daemon running: {is_running}")

@daemon.command()
def stop():
    """Stop the daemon (tmux kill-server)."""
    tmux_backend.daemon_stop()
    click.echo("Daemon stopped.")

@daemon.command()
def restart():
    """Restart the daemon."""
    tmux_backend.daemon_restart()
    click.echo("Daemon restarted.")

if __name__ == '__main__':
    cli()