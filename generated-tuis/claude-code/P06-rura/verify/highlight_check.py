#!/usr/bin/env python3
"""Verify syntax highlighting really colours the command line differently per token."""
import os, pty, select, sys, time, shutil, fcntl, termios, struct
import pyte

BIN = sys.argv[1] if len(sys.argv) > 1 else "/tmp/toolf-install-test/bin/toolf"
WORK = "/tmp/toolf-hl"
LOG = f"{WORK}/server.log"
ROWS, COLS = 30, 120
shutil.rmtree(WORK, ignore_errors=True)
os.makedirs(WORK, exist_ok=True)
open(LOG, "w").write("2026-08-13 ERROR boom\n")

screen = pyte.Screen(COLS, ROWS)
stream = pyte.ByteStream(screen)
pid, fd = pty.fork()
if pid == 0:
    os.environ.update(TERM="xterm-256color")
    os.chdir(WORK)
    os.execv(BIN, [BIN, "--file", LOG])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

def pump(t=0.6):
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                d = os.read(fd, 65536)
            except OSError:
                break
            if not d:
                break
            stream.feed(d)

def send(s, t=0.4):
    os.write(fd, s.encode())
    pump(t)

pump(1.4)
send("\x0c")
CMD = "grep -i 'ERROR' /tmp/toolf-hl/server.log | tail -n 20"
send(CMD, 0.9)

# Locate the command row.
row = next(i for i, l in enumerate(screen.display) if CMD[:12] in l)
line = screen.buffer[row]
text = screen.display[row]
start = text.index("grep")

def color_of(substr, offset=0):
    """Foreground colour of the first cell of `substr` on the command row."""
    i = text.index(substr, offset)
    return line[i].fg, i

results = {}
for label, sub in [
    ("command  grep", "grep"),
    ("flag     -i", "-i"),
    ("string   'ERROR'", "'ERROR'"),
    ("path     /tmp", "/tmp/toolf-hl"),
    ("pipe     |", "|"),
    ("command  tail", "tail"),
    ("number   20", "20"),
]:
    fg, idx = color_of(sub)
    results[label] = fg
    print(f"  {label:<18} col {idx:>3}  fg={fg}")

distinct = set(results.values())
print(f"\ndistinct colours used: {len(distinct)} -> {sorted(distinct)}")
ok = len(distinct) >= 5
print("PASS: syntax highlighting uses distinct colours" if ok
      else "FAIL: too few distinct colours")

os.write(fd, b"\x1b[21~")
time.sleep(0.3)
os.close(fd)
os.waitpid(pid, 0)
sys.exit(0 if ok else 1)
