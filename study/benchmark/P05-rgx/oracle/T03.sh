#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "13812345678"
check_screen "15999998888"
check_screen "13900001111"
check_screen_highlight --text "13812345678" --highlighted
check_screen_highlight --text "15999998888" --highlighted
check_screen_highlight --text "13900001111" --highlighted

oracle_msg "T03 PASS"
