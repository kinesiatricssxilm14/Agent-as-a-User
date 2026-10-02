from src.agent_tui.syscall_parser import parse_strace_line

log_lines = [
    '[pid 12345] execve("/usr/bin/git", ["git", "commit", "-m", "fix bug"], 0x...) = 0',
    '[pid 12346] unlink("/workspace/old_file.txt") = 0',
    '[pid 12347] rename("old", "new") = 0',
    '[pid 12348] unlinkat(AT_FDCWD, "file.txt", 0) = 0',
    '[pid 12349] kill(1234, SIGKILL) = 0',
    '[pid 12350] connect(3, {sa_family=AF_INET, sin_port=htons(80), sin_addr=inet_addr("1.1.1.1")}, 16) = 0',
    '[pid 12351] connect(3, {sa_family=AF_UNSPEC, ...}, 16) = 0',
    '[pid 12352] renameat(AT_FDCWD, "old.txt", AT_FDCWD, "new.txt") = 0'
]

for line in log_lines:
    cat, detail = parse_strace_line(line)
    if cat:
        print(f"[{cat}] {detail}")
    else:
        print(f"[NONE] {line}")
