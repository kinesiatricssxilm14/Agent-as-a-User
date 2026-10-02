#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen_absent "git-log-pretty"
check_screen_absent "$FP_FILE_CONTENT"
check_screen_absent "git log --oneline --graph"

oracle_msg "T03 PASS"
