#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

# Pinned + fingerprint streams (seed/init.sh); 18 additional streams from generated XADD block
check_screen "activity:part-professional"
check_screen "activity:sort-until-hundred"
check_screen "audit:foreign-box"
check_screen "audit:pick-street-present"
check_screen "audit:trouble-soldier"
check_screen "audit:upon-little-either"
check_screen "events:agreement-because"
check_screen "events:alone-each-fight"
check_screen "events:bench-stream"
check_screen "events:bench-$FP_SCREEN"
check_screen "events:development"
check_screen "events:off-down-raise"
check_screen "events:same-spring-fish-to"
check_screen "events:similar-nearly"
check_screen "events:structure-adult"
check_screen "events:those-again-pretty"
check_screen "stream:during-camera-set"
check_screen "stream:move-raise-growth"
check_screen "stream:science-up-fact"
check_screen "stream:there-daughter-fine"

oracle_msg "T05 PASS"
