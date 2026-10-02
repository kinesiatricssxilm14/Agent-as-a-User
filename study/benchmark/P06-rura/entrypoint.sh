#!/bin/sh
# ============================================================
# TUI Benchmark — standard container entry point
# ============================================================
set -e

# Run project-specific seed / init if present
if [ -f /bench/init.sh ]; then
    sh /bench/init.sh
fi

# Execute the TUI (first arg = binary, rest = extra args)
exec "$@"
