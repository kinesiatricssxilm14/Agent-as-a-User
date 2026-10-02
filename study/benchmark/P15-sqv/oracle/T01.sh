#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "Alice"
check_screen "alice@ex.com"
check_screen "Bob"
check_screen "bob@ex.com"
check_screen "Carol"
check_screen "carol@ex.com"
check_screen "Dave"
check_screen "dave@ex.com"
check_screen "Allison Hill"
check_screen "donaldgarcia@example.net"
check_screen "30"
check_screen "Engineering"
check_screen "25"
check_screen "Design"
check_screen "35"
check_screen "28"
check_screen "Product"
check_screen "62"

oracle_msg "T01 PASS"
