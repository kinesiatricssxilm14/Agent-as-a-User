#!/usr/bin/env python3
"""In-container verification: run the documented benchmark workflow end to end.

Launches `toolf --file /bench/server.log` exactly as the benchmark does, then uses only
keyboard input to run a pipeline, do a partial execution, and save to /bench/data/result.txt.
"""
import os, pty, select, sys, time, fcntl, termios, struct
import pyte

LOG = "/bench/server.log"
OUT = "/bench/data/result.txt"
ROWS, COLS = 40, 150

expected_errors = [l for l in open(LOG).read().splitlines() if " ERROR " in l]
print(f"log has {len(expected_errors)} ERROR lines")

screen = pyte.Screen(COLS, ROWS)
stream = pyte.ByteStream(screen)
pid, fd = pty.fork()
if pid == 0:
    os.environ.update(TERM="xterm-256color")
    os.environ.pop("NO_COLOR", None)
    # The documented launch command, verbatim.
    os.execvp("toolf", ["toolf", "--file", LOG])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

def pump(t=0.6):
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                d = os.read(fd, 65536)
            except OSError:
                return
            if not d:
                return
            stream.feed(d)

def send(s, t=0.4):
    os.write(fd, s.encode())
    pump(t)

def text():
    return "\n".join(screen.display)

fails = []
def check(cond, label, extra=""):
    print(f"  {'PASS' if cond else 'FAIL'} {label}" + ("" if cond else f"  {extra}"))
    if not cond:
        fails.append(label)

pump(1.6)
print("\n[1] launches with the documented command")
check("toolf" in text(), "banner")
check("/bench/server.log" in text(), "default log path shown")

print("\n[2] full pipeline: tail of ERROR lines")
send("\x0c")
send(f"cat {LOG} | grep ERROR | tail -n 3", 0.5)
send("\r", 1.8)
t = text()
check("exit 0" in t, "ran successfully", t[:300])
last3 = expected_errors[-3:]
check(all(l.split()[0] in t for l in last3), "shows the real last 3 ERROR lines")

print("\n[3] partial execution with Alt+backslash")
send("\x1b[H")
send("\x1b[1;3C")
send("\x1b[1;3C", 0.4)
check("stage 2/3" in text(), "cursor at stage 2/3",
      [l for l in screen.display if "stage" in l])
send("\x1b\\", 1.8)
t = text()
check("partial 2/3" in t, "partial marker shown")
shown = sum(1 for l in screen.display if "ERROR svc=api" in l)
check(shown == len(expected_errors),
      f"partial shows all {len(expected_errors)} ERROR lines (got {shown})")
check("tail -n 3" in t, "command line visible on same screen as output")
check(not screen.cursor.hidden and screen.cursor.y <= 5, "cursor visible on command line")

print("\n[4] save to /bench/data/result.txt")
send("\x0c")
send(f"cat {LOG} | grep ERROR", 0.5)
send("\r", 1.8)
send("\x13", 0.7)
check("save output" in text(), "save prompt open")
send("\x15")
send(OUT, 0.4)
send("\r", 1.5)
check("wrote" in text(), "save confirmed",
      [l for l in screen.display if "wrote" in l or "save" in l.lower()])

if os.path.exists(OUT):
    got = open(OUT).read().splitlines()
    check(got == expected_errors, f"result.txt matches real grep ({len(got)} lines)")
else:
    check(False, f"{OUT} created")

print("\n[5] quit")
send("\x1b[21~", 0.8)
try:
    os.close(fd)
except OSError:
    pass
_, status = os.waitpid(pid, 0)
check(status == 0, f"clean exit (status {status})")

print("\n" + "=" * 50)
if fails:
    print(f"FAILURES ({len(fails)}): {fails}")
    print("\n--- screen ---")
    print(text())
    sys.exit(1)
print("ALL CONTAINER CHECKS PASSED")
