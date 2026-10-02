#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_screen "wget"
check_screen "libc6"
check_screen "libgnutls30"
check_screen "libidn2-0"
check_screen "libnettle8"
check_screen "libpcre2-8-0"
check_screen "libpsl5"
check_screen "libuuid1"
check_screen "zlib1g"

oracle_msg "T03 PASS"
