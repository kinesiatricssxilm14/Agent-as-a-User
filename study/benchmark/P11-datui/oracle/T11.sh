#!/usr/bin/env bash
set -euo pipefail

: "${ORACLE_FP_FILE:?run bench-oracle gen-fingerprints first}"

source "${BENCH_ORACLE_LIB:?set BENCH_ORACLE_LIB to evaluation/scripts/oracle_lib.sh}"

load_fingerprints

export ORACLE_SEARCH_ROOTS="/bench/data/Henry.csv"
check_filesystem_content "employee_id,full_name,age,salary,score,department,country,email,is_active,fingerprint_payload
EMP-4870,Henry Roach,50,6207,50.06,Legal,JP,davidmccall@example.com,true,normal_data_stream
EMP-0226,Casey Henry,24,15343,52.93,Design,UK,ejefferson@example.net,true,normal_data_stream
EMP-4856,Carolyn Henry,24,18858,56.05,Legal,UK,amay@example.org,true,normal_data_stream
EMP-2351,Brandon Henry,63,19810,58.49,Engineering,UK,paulgreen@example.net,true,normal_data_stream
EMP-1841,Alicia Henry MD,38,10419,65.92,Marketing,UK,dnewton@example.com,true,normal_data_stream
EMP-0797,Henry Aguilar,24,14698,69.62,HR,DE,ncaldwell@example.net,false,normal_data_stream
EMP-0170,Dr. Caroline Henry DDS,46,18940,77.0,Product,CN,opope@example.net,true,normal_data_stream
EMP-0655,Henry Aguilar,52,14228,77.27,Sales,FR,ortizmark@example.org,true,normal_data_stream
EMP-3487,Henry Simmons,61,4634,77.8,Product,UK,jesse38@example.com,true,normal_data_stream
EMP-0550,Amber Henry,59,16875,86.32,Legal,FR,nclark@example.net,false,normal_data_stream
EMP-1707,Henry Huffman,39,18474,87.58,Engineering,CN,davisjoseph@example.net,false,normal_data_stream
EMP-4712,Henry Sutton,24,16647,95.49,HR,US,jeremytran@example.com,true,normal_data_stream"

oracle_msg "T11 PASS"
