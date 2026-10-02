#!/usr/bin/env python3
"""Print the rendered toolf screen after a scripted interaction, for visual review."""
import os, pty, select, sys, time, shutil, fcntl, termios, struct
import pyte

BIN = sys.argv[1] if len(sys.argv) > 1 else "/tmp/toolf-install-test/bin/toolf"
SCENE = sys.argv[2] if len(sys.argv) > 2 else "partial"
WORK = "/tmp/toolf-shot"
LOG = f"{WORK}/server.log"
ROWS, COLS = 34, 128

shutil.rmtree(WORK, ignore_errors=True)
os.makedirs(WORK, exist_ok=True)
rows = []
for i in range(1, 41):
    lvl = "ERROR" if i % 7 == 0 else ("WARN" if i % 5 == 0 else "INFO")
    rows.append(f"2026-08-13T10:{i:02d}:00 {lvl} request id={i} path=/api/v{i%3} ms={i*3}")
open(LOG, "w").write("\n".join(rows) + "\n")

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

def send(s, t=0.35):
    os.write(fd, s.encode())
    pump(t)

pump(1.5)

if SCENE == "startup":
    pass
elif SCENE == "full":
    send("\x0c")
    send(f"cat {LOG} | grep ERROR | tail -n 5", 0.4)
    send("\r", 1.6)
elif SCENE == "partial":
    send("\x0c")
    send(f"cat {LOG} | grep ERROR | awk '{{print $2, $4}}' | sort -u", 0.4)
    send("\x1b[H")
    send("\x1b[1;3C")
    send("\x1b[1;3C", 0.3)
    send("\x1b\\", 1.6)
elif SCENE == "save":
    send("\x0c")
    send(f"cat {LOG} | grep WARN", 0.4)
    send("\r", 1.5)
    send("\x13", 0.7)
elif SCENE == "complete":
    send("\x0c")
    send(f"cat {LOG} | ", 0.3)
    send("so", 0.3)
    send("\t", 0.9)
elif SCENE == "search":
    send("\x0c")
    send(f"cat {LOG}", 0.3)
    send("\r", 1.4)
    send("\x06", 0.4)
    send("ERROR", 0.3)
    send("\r", 1.0)

print(f"=== scene: {SCENE} ===")
for i, line in enumerate(screen.display):
    print(f"{line}")
print(f"[cursor row={screen.cursor.y} col={screen.cursor.x} hidden={screen.cursor.hidden}]")
os.write(fd, b"\x1b[21~")
time.sleep(0.4)
os.close(fd)
os.waitpid(pid, 0)
