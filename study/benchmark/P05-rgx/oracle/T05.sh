#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "[USER]@example.com"
check_screen "bad-email-here"
check_screen "test@domain.org"
check_screen "2026-01-15"
check_screen "foo@bar.co"
check_screen "[USER]@EXAMPLE.COM"
check_screen "admin@company.net"

oracle_msg "T05 PASS"
