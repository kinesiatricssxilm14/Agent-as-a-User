#!/bin/bash
# P05 · rgx seed
set -e

mkdir -p /root/.config/rgx /bench/data

cat > /bench/data/input.txt << 'EOF'
user@example.com
bad-email-here
test@domain.org
2026-01-15
foo@bar.co
USER@EXAMPLE.COM
admin@company.net
2025-12-31
not-an-email
13812345678
15999998888
https://example.com/path?q=1
hello world
foo@bar
2026/06/19
13900001111
EOF
