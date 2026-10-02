#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_shell "dpkg -l hello 2>/dev/null | grep -c '^ii'" "1"
check_shell "apt list --upgradable 2>/dev/null | grep -cE '^hello/'" "0"

oracle_msg "T04 PASS"
