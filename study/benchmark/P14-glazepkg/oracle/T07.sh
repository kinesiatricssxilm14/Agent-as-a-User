#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

check_shell "pip3 show requests >/dev/null 2>&1 && python3 -c \"import requests; print('1' if requests.__version__ != '2.34.1' else 'same')\"" "1"

oracle_msg "T07 PASS"