#!/bin/sh
set -eu

test_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
c++ -std=c++11 -Wall -Wextra -Werror \
    -I"$test_dir/.." \
    "$test_dir/test_tx_packet.cpp" \
    -o /tmp/ruview-csi-tx-packet-test
/tmp/ruview-csi-tx-packet-test
