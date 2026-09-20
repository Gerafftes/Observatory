#!/bin/sh
set -eu

test_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cc -std=c11 -Wall -Wextra -Werror \
    "$test_dir/test_ld2450_parser.c" \
    "$test_dir/../main/ld2450_parser.c" \
    "$test_dir/../main/coordinate_transform.c" \
    -lm \
    -o /tmp/ruview-mmwave-parser-test
/tmp/ruview-mmwave-parser-test

cc -std=c11 -Wall -Wextra -Werror \
    -I"$test_dir/../main" \
    "$test_dir/test_identity_indicator.c" \
    "$test_dir/../main/identity_indicator.c" \
    -o /tmp/ruview-mmwave-identity-indicator-test
/tmp/ruview-mmwave-identity-indicator-test
