#!/usr/bin/env python3
"""Robustness checks: awkward inputs, arbitrary paths, and stress cases.

The benchmark requires the tool to work for *any* valid input, so these exercise
regexes with special characters, quoting, large output, and unusual save targets.
"""
import os, pty, select, sys, time, shutil, fcntl, termios, struct
import pyte

BIN = sys.argv[1] if len(sys.argv) > 1 else "/tmp/toolf-install-test/bin/toolf"
WORK = "/tmp/toolf-robust"
LOG = f"{WORK}/app-2026.log"
ROWS, COLS = 40, 150

shutil.rmtree(WORK, ignore_errors=True)
os.makedirs(WORK, exist_ok=True)
rows = [
    'INFO  GET /a?x=1&y=2 200 "ua|pipe" 12ms',
    'ERROR POST /b 500 "boom (fatal)" 99ms',
    "WARN  GET /c 404 'quoted' 5ms",
    'ERROR GET /d 503 "tab\there" 42ms',
    'INFO  GET /e 200 "$HOME and `tick`" 7ms',
]
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
    print(f"  {'PASS' if cond else 'FAIL'} {label}" + ("" if cond else f" {extra}"))
    if not cond:
        fails.append(label)

pump(1.5)

print("\n[A] regex with special characters (awk field extraction)")
send("\x0c")
send(f"""grep -E '^(ERROR|WARN)' {LOG} | awk '{{print $2, $3}}'""", 0.5)
send("\r", 1.6)
t = text()
check("exit 0" in t, "special-char regex ran", t[:200])
check("POST" in t and "/b" in t, "awk extracted fields")

print("\n[B] pipe character inside a quoted pattern")
send("\x0c")
send(f"""grep 'ua|pipe' {LOG} | wc -l""", 0.5)
t = text()
# The quoted pipe must not be treated as a stage separator.
check("stage 2/2" in t, "quoted | not counted as separator", [l for l in screen.display if "stage" in l])
send("\r", 1.5)
t = text()
check("exit 0" in t, "quoted-pipe pipeline ran")

print("\n[C] sed with slashes and a substitution")
send("\x0c")
send(f"""cat {LOG} | sed -n 's#.*\\(ERROR\\).*#\\1#p' | sort | uniq -c""", 0.5)
send("\r", 1.6)
t = text()
check("exit 0" in t, "sed substitution ran", t[:200])
check("2" in t and "ERROR" in t, "uniq -c counted 2 ERROR")

print("\n[D] unterminated quote is refused, not executed")
send("\x0c")
send("grep 'oops", 0.5)
t = text()
check("unterminated" in t, "validation warns on open quote")
send("\r", 0.9)
t = text()
check("cannot run" in t, "refused to run invalid pipeline")

print("\n[E] large output is handled and scrolls")
send("\x0c")
send("seq 1 5000", 0.4)
send("\r", 2.2)
t = text()
check("5000 line(s)" in t, "5000 lines reported", [l for l in screen.display if "line(s)" in l])
send("\x1b[6~", 0.5)   # PageDown
t = text()
check(" 1 │" not in t, "PageDown scrolled away from line 1")
send("\x1b[1;5F", 0.6)  # Ctrl+End
t = text()
check("5000" in t, "Ctrl+End reaches last line")

print("\n[F] save to a path with spaces and new directories")
target = f"{WORK}/out dir/deep/my result.txt"
send("\x0c")
send(f"grep ERROR {LOG}", 0.4)
send("\r", 1.5)
send("\x13", 0.6)
send("\x15")
send(target, 0.4)
send("\r", 1.3)
t = text()
check("wrote" in t, "saved to path with spaces", [l for l in screen.display if "save" in l.lower() or "wrote" in l])
if os.path.exists(target):
    body = open(target).read()
    expected = [r for r in rows if r.startswith("ERROR")]
    check(body.splitlines() == expected, "file content matches grep output",
          f"got {body!r}")
else:
    check(False, "file with spaces created")

print("\n[G] overwriting an existing file works")
send("\x0c")
send(f"grep WARN {LOG}", 0.4)
send("\r", 1.5)
send("\x13", 0.6)
send("\x15")
send(target, 0.4)
send("\r", 1.3)
if os.path.exists(target):
    body = open(target).read()
    check("WARN" in body and "ERROR" not in body, "overwrite replaced contents",
          f"got {body!r}")
else:
    check(False, "overwrite kept file")

print("\n[H] a failing middle stage still reports clearly")
send("\x0c")
send(f"cat {LOG} | nosuchcommand_xyz | wc -l", 0.4)
send("\r", 1.6)
t = text()
check("stderr" in t, "stderr surfaced for missing command")
check("not found" in t.lower(), "shell reported command not found")

print("\n[I] empty result set is reported, not mistaken for an error")
send("\x0c")
send(f"grep ZZZNOMATCH {LOG}", 0.4)
send("\r", 1.5)
t = text()
check("0 line(s)" in t or "no output" in t, "empty output reported",
      [l for l in screen.display if "line(s)" in l or "output" in l][:3])

print("\n[J] history recall after many runs")
send("\x1b[A", 0.5)
t = text()
check("grep ZZZNOMATCH" in t or "grep" in t, "Up recalls previous command")

send("\x1b[21~", 0.7)
try:
    os.close(fd)
except OSError:
    pass
os.waitpid(pid, 0)

print("\n" + "=" * 52)
if fails:
    print(f"FAILURES ({len(fails)}): {fails}")
    sys.exit(1)
print("ALL ROBUSTNESS CHECKS PASSED")
