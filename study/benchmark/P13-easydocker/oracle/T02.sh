#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "2026-06-21T06:00:02.000000Z 0"
check_screen "[System]"
check_screen "[MY-010931]"
check_screen "[Server]"
check_screen "/usr/sbin/mysqld: ready for connections."
check_screen "[MY-011323]"
check_screen "[Server]"
# check_screen "X Plugin ready for connections."

oracle_msg "T02 PASS"