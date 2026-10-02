#!/bin/bash
# P01 aptui seed — wget/curl installed at Docker build time; do not apt-get here (slow on each start)
set -e
# Tasks need: wget installed, tree not installed (for T01 install)

# apt-get update -qq 2>/dev/null
apt-get install -y wget curl 2>/dev/null

# T04 oracle: record upgradable package count before upgrade
# mkdir -p /bench/data
# apt list --upgradable 2>/dev/null | wc -l > /bench/data/.upgradable_baseline

echo "deb http://deb.debian.org/debian bullseye main" > /etc/apt/sources.list.d/test-old.list
apt-get update
apt-cache madison hello
apt-get install -y hello=2.10-2
