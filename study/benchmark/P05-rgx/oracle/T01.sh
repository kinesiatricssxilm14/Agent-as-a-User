#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "user@example.com"
check_screen "test@domain.org"
check_screen "foo@bar.co"
check_screen "USER@EXAMPLE.COM"
check_screen "admin@company.net"
check_screen_highlight --text "user@example.com" --highlighted
check_screen_highlight --text "test@domain.org" --highlighted
check_screen_highlight --text "foo@bar.co" --highlighted
check_screen_highlight --text "USER@EXAMPLE.COM" --highlighted
check_screen_highlight --text "admin@company.net" --highlighted

oracle_msg "T01 PASS"
