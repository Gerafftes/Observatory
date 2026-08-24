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
