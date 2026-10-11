#!/bin/sh
set -eu

test_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$test_dir/../../../../.." && pwd)

if [ -z "${IDF_PATH:-}" ] && [ -d "$repo_root/.toolchains/esp-idf-v5.4" ]; then
    IDF_PATH="$repo_root/.toolchains/esp-idf-v5.4"
    export IDF_PATH
fi

if [ -z "${TX_PYTHON:-}" ] && [ -x "$repo_root/.toolchains/python-3.15/bin/python3.15" ]; then
    TX_PYTHON="$repo_root/.toolchains/python-3.15/bin/python3.15"
elif [ -z "${TX_PYTHON:-}" ] && command -v python3.15 >/dev/null 2>&1; then
    TX_PYTHON=python3.15
fi

TX_PYTHON="${TX_PYTHON:-python3}"
"$TX_PYTHON" -c 'import sys; raise SystemExit(sys.version_info < (3, 15))' || {
    printf 'Python 3.15 or newer is required for TX Python tests.\n' >&2
    exit 1
}

c++ -std=c++11 -Wall -Wextra -Werror \
    -I"$test_dir/.." \
    "$test_dir/test_tx_packet.cpp" \
    -o /tmp/ruview-csi-tx-packet-test
/tmp/ruview-csi-tx-packet-test

cc -std=c11 -Wall -Wextra -Werror \
    -I"$test_dir/../main" \
    "$test_dir/test_idf_contract.c" \
    "$test_dir/../main/tx_packet.c" \
    "$test_dir/../main/tx_scheduler.c" \
    -o /tmp/ruview-csi-tx-idf-contract-test
/tmp/ruview-csi-tx-idf-contract-test

cc -std=c11 -Wall -Wextra -Werror \
    -I"$test_dir/stubs" -I"$test_dir/../main" \
    "$test_dir/test_led.c" "$test_dir/../main/tx_led.c" \
    -o /tmp/ruview-csi-tx-led-test
/tmp/ruview-csi-tx-led-test

cc -std=c11 -Wall -Wextra -Werror \
    -I"$test_dir/stubs" -I"$test_dir/../main" \
    "$test_dir/test_app_boot.c" "$test_dir/../main/tx_boot.c" \
    "$test_dir/../main/tx_packet.c" "$test_dir/../main/tx_scheduler.c" \
    -o /tmp/ruview-csi-tx-app-boot-test
/tmp/ruview-csi-tx-app-boot-test

for rollback in off on; do
    option=""
    if [ "$rollback" = on ]; then option="-DCONFIG_TX_VERIFIED_ROLLBACK=1"; fi
    cc -std=c11 -Wall -Wextra -Werror $option \
        -I"$test_dir/stubs" -I"$test_dir/../main" \
        "$test_dir/test_runtime.c" "$test_dir/../main/tx_config.c" \
        "$test_dir/../main/tx_network.c" "$test_dir/../main/tx_boot.c" \
        -o "/tmp/ruview-csi-tx-runtime-$rollback-test"
    "/tmp/ruview-csi-tx-runtime-$rollback-test"
done
"$TX_PYTHON" -B -m unittest discover -s "$test_dir" -p 'test_*.py' -v

# Unsafe first-boot/persistence configurations must fail compilation.
for flag in SECURE_BOOT SECURE_FLASH_ENC_ENABLED BOOTLOADER_APP_ANTI_ROLLBACK NVS_ENCRYPTION ESP_WIFI_NVS_ENABLED ESP_PHY_CALIBRATION_AND_DATA_STORAGE ESP_PHY_INIT_DATA_IN_PARTITION ESP_COREDUMP_ENABLE_TO_FLASH; do
    if cc -x c -fsyntax-only -I"$test_dir/stubs" -D"CONFIG_$flag=1" \
        "$test_dir/../main/tx_safety.h" >/tmp/ruview-csi-tx-safety-compile.log 2>&1; then
        printf 'Unsafe configuration compiled: %s\n' "$flag" >&2
        exit 1
    fi
done
printf 'TX first-boot/persistence compile guards passed\n'
