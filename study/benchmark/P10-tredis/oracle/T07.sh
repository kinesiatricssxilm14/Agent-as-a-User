#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

# Pinned + generated ACL users from seed/init.sh (faker seed=42); fingerprint user bench-<token>
check_screen "default"
check_screen "bench-acl-user"
check_screen "bench-${FP_SCREEN}"
check_screen "join-grow-world"
check_screen "identify-east-total"
check_screen "whether-loss"
check_screen "prepare-authority"
check_screen "others-rest"

oracle_msg "T07 PASS"
