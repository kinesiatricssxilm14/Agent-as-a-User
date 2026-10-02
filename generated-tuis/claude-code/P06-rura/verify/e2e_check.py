#!/usr/bin/env python3
"""Drive the real toolf binary in a PTY and verify the benchmark workflow.

Uses pyte as a real terminal emulator so assertions run against the actual rendered
screen (including cursor position), the way a human or a screenshot-based grader sees it.
"""
import os, pty, select, sys, time, shutil, fcntl, termios, struct
import pyte

BIN = sys.argv[1] if len(sys.argv) > 1 else "/tmp/toolf-install-test/bin/toolf"
WORK = "/tmp/toolf-e2e"
LOG = f"{WORK}/server.log"
OUT = f"{WORK}/data/result.txt"
ROWS, COLS = 40, 140

shutil.rmtree(WORK, ignore_errors=True)
os.makedirs(WORK, exist_ok=True)

lines = []
for i in range(1, 61):
    lvl = "ERROR" if i % 7 == 0 else ("WARN" if i % 5 == 0 else "INFO")
    lines.append(f"2026-08-13T10:{i:02d}:00 {lvl} request id={i} path=/api/v{i%3}")
open(LOG, "w").write("\n".join(lines) + "\n")
ERRORS = [l for l in lines if " ERROR " in l]
WARNS = [l for l in lines if " WARN " in l]
print(f"log: {len(lines)} lines, {len(ERRORS)} ERROR, {len(WARNS)} WARN")

screen = pyte.Screen(COLS, ROWS)
stream = pyte.ByteStream(screen)

pid, fd = pty.fork()
if pid == 0:
    os.environ.update(TERM="xterm-256color")
    os.chdir(WORK)
    os.execv(BIN, [BIN, "--file", LOG])

# The child inherits the pty size, which defaults to 0x0; ratatui needs a real area.
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

def pump(t=0.6):
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                data = os.read(fd, 65536)
            except OSError:
                break
            if not data:
                break
            stream.feed(data)

def send(s, t=0.35):
    os.write(fd, s.encode())
    pump(t)

def text():
    return "\n".join(screen.display)

failures = []
def check(cond, label, extra=""):
    if cond:
        print(f"  PASS {label}")
    else:
        print(f"  FAIL {label} {extra}")
        failures.append(label)

def has(needle, label):
    check(needle in text(), label, f"({needle!r} missing)")

pump(1.5)
print("\n[1] startup screen")
has("toolf", "banner")
has("server.log", "log path in header")
has("pipeline", "pipeline panel")
has("output", "output panel")
has("F10", "quit hint")
has("F1", "help hint")

print("\n[2] full pipeline: count ERROR lines")
send("\x0c")
send(f"cat {LOG} | grep ERROR | wc -l", 0.5)
send("\r", 1.6)
t = text()
has("exit 0", "full run exit 0")
check(str(len(ERRORS)) in t, "real ERROR count in output")
has("full", "run kind = full")

print("\n[3] partial execution (Alt+\\)")
send("\x0c")
send(f"cat {LOG} | grep ERROR | tail -n 3", 0.5)
send("\x1b[H")                 # Home
send("\x1b[1;3C")              # Alt+Right -> boundary 1
send("\x1b[1;3C", 0.4)         # Alt+Right -> stage 2
t = text()
check("stage 2/3" in t, "cursor reports stage 2/3", f"got: {[l for l in screen.display if 'stage' in l]}")
send("\x1b\\", 1.6)            # Alt+\
t = text()
has("partial 2/3", "partial run marker")
# Same-screen requirement: the command line must still be visible with the output.
check("tail -n 3" in t, "command line visible alongside output")
check("grep ERROR" in t, "pipeline text visible")
# Partial stops after grep, so all ERROR lines show, not just tail's last 3.
shown = sum(1 for l in screen.display if "ERROR request" in l)
check(shown > 3, f"partial shows grep output ({shown} ERROR lines, >3)")
# The cursor must be visible on the command line.
cy, cx = screen.cursor.y, screen.cursor.x
check(not screen.cursor.hidden, "cursor visible")
check(cy <= 5, f"cursor on command line row (row {cy}, col {cx})")

print("\n[4] save output with Ctrl+S")
send("\x0c")
send(f"cat {LOG} | grep ERROR", 0.5)
send("\r", 1.6)
send("\x13", 0.6)              # Ctrl+S
t = text()
has("save output", "save prompt")
check("ERROR request" in t, "output still visible while prompt is open")
send("\x15")                   # Ctrl+U clears prefilled default
send(OUT, 0.4)
send("\r", 1.3)
has("wrote", "save confirmation")

if os.path.exists(OUT):
    body = open(OUT).read()
    got = [l for l in body.splitlines() if l.strip()]
    check(got == ERRORS, f"saved file matches real grep output ({len(got)} lines)")
else:
    check(False, "save file created")

print("\n[5] tab completion (real filesystem)")
send("\x0c")
send(f"cat {WORK}/ser", 0.4)
send("\t", 0.9)
has("server.log", "tab completed real filename")

print("\n[6] tab completion for commands")
send("\x0c")
send("gre", 0.3)
send("\t", 0.9)
t = text()
check("grep" in t, "command completion offers grep")

print("\n[7] search the output (Ctrl+F)")
send("\x1b", 0.2)              # dismiss completion
send("\x0c")
send(f"cat {LOG}", 0.4)
send("\r", 1.4)
send("\x06", 0.5)              # Ctrl+F
has("search output", "search prompt")
send("WARN", 0.3)
send("\r", 1.0)
t = text()
check("match" in t.lower(), "search reports matches")
check(str(len(WARNS)) in t, f"search hit count {len(WARNS)}")

print("\n[8] help panel (F1)")
send("\x1bOP", 0.7)            # F1 toggle
send("\x1bOP", 0.9)            # F1 back on
t = text()
check("keys" in t, "help panel lists keys")
check("Alt+" in t, "help documents Alt bindings")

print("\n[9] error handling: nonexistent file")
send("\x0c")
send("cat /no/such/file/xyz", 0.4)
send("\r", 1.5)
t = text()
check("stderr" in t, "stderr shown in output pane")
check("exit 0" not in t.split("output")[1][:200] if "output" in t else True,
      "nonzero exit reported")

print("\n[10] quit with F10")
send("\x1b[21~", 0.8)
os.close(fd)
_, status = os.waitpid(pid, 0)
check(status == 0, f"clean exit (status {status})")

print("\n" + "=" * 52)
if failures:
    print(f"FAILURES ({len(failures)}): {failures}")
    print("\n--- final screen ---")
    print(text())
    sys.exit(1)
print("ALL E2E CHECKS PASSED")
