import re
import os
import shlex

# Regex patterns for strace output
# Match lines like: [pid 12345] execve("/bin/ls", ["ls", "-l"], 0x...) = 0
# or without pid: execve("/bin/ls", ["ls", "-l"], 0x...) = 0
EXECVE_RE = re.compile(r'execve\([^,]+,\s*\[(.*?)\]')
UNLINK_RE = re.compile(r'unlink\("([^"]+)"\)')
RENAME_RE = re.compile(r'rename\("([^"]+)",\s*"([^"]+)"\)')
UNLINKAT_RE = re.compile(r'unlinkat\([^,]+,\s*"([^"]+)"')
RENAMEAT_RE = re.compile(r'renameat\([^,]+,\s*"([^"]+)",\s*[^,]+,\s*"([^"]+)"\)')
KILL_RE = re.compile(r'kill\([^,]+,\s*([A-Z0-9_]+)\)')
CONNECT_RE = re.compile(r'connect\([^,]+,\s*\{(.+?)\}')

def parse_strace_line(line):
    line = line.strip()
    # We only care about successful or in-progress calls, but for simplicity,
    # we'll just extract the information if the pattern matches.
    
    # process (execve)
    m = EXECVE_RE.search(line)
    if m:
        # Extract string arguments
        args_str = m.group(1)
        # Find all strings enclosed in quotes
        args = re.findall(r'"((?:\\.|[^"\\])*)"', args_str)
        # Unescape and format command
        formatted_args = []
        for arg in args:
            # simple unescape for \"
            arg = arg.replace('\\"', '"')
            formatted_args.append(arg)
        
        parsed_command = shlex.join(formatted_args)
        return "process", {"raw_log": line, "parsed_command": parsed_command}

    # file (unlink, rename, unlinkat, renameat)
    m = UNLINK_RE.search(line)
    if m:
        return "file", {"raw_log": line, "parsed_target": m.group(1)}
    
    m = UNLINKAT_RE.search(line)
    if m:
        return "file", {"raw_log": line, "parsed_target": m.group(1)}
        
    m = RENAME_RE.search(line)
    if m:
        return "file", {"raw_log": line, "parsed_target": f"{m.group(1)} -> {m.group(2)}"}
        
    m = RENAMEAT_RE.search(line)
    if m:
        return "file", {"raw_log": line, "parsed_target": f"{m.group(1)} -> {m.group(2)}"}

    # signal (kill)
    m = KILL_RE.search(line)
    if m:
        return "signal", {"raw_log": line, "parsed_target": m.group(1)}

    # network (connect)
    m = CONNECT_RE.search(line)
    if m:
        # For connect, the target might be the struct sockaddr representation
        # e.g., sa_family=AF_INET, sin_port=htons(80), sin_addr=inet_addr("1.1.1.1")
        return "network", {"raw_log": line, "parsed_target": m.group(1)}

    return None, None

def read_incremental_log(filepath, offset):
    """
    Reads the strace log file from the given byte offset.
    Returns the parsed mutation object and the new offset.
    """
    mutations = {
        "process": {"changed": False, "details": []},
        "file": {"changed": False, "details": []},
        "network": {"changed": False, "details": []},
        "signal": {"changed": False, "details": []}
    }
    
    if not os.path.exists(filepath):
        return {"has_mutation": False, **mutations}, offset

    try:
        with open(filepath, 'rb') as f:
            f.seek(offset)
            # Read all new lines
            data = f.read()
            new_offset = f.tell()
            
            if not data:
                return {"has_mutation": False, **mutations}, new_offset
                
            # Decode ignoring errors to avoid crash on partial writes
            text = data.decode('utf-8', errors='ignore')
            lines = text.splitlines()
            
            has_mutation = False
            for line in lines:
                category, detail = parse_strace_line(line)
                if category:
                    mutations[category]["changed"] = True
                    mutations[category]["details"].append(detail)
                    has_mutation = True
                    
            return {"has_mutation": has_mutation, **mutations}, new_offset
            
    except Exception as e:
        # In case of any read error, return safe defaults
        return {"has_mutation": False, **mutations}, offset
