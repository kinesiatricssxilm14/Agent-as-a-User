#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "batch-install-python"
check_screen "bash"
check_screen "git"
check_screen "pretty git log"
check_screen "git log --oneline --graph"
check_screen "$FP_FILE_CONTENT"

oracle_msg "T02 PASS"
