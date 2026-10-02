#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "git-log-pretty"
check_screen "bash"
check_screen "git"
check_screen "git log --oneline --graph"
check_screen "$FP_FILE_CONTENT"

oracle_msg "T01 PASS"
