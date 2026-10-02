#!/bin/bash
# P11 · datui seed
set -e

mkdir -p /bench/data

# bench.sh seed injection writes expanded fingerprints to /bench/employees.csv;
# datui reads /bench/data/employees.csv (see Dockerfile CMD).
if [ -f /bench/employees.csv ]; then
  cp /bench/employees.csv /bench/data/employees.csv
fi
