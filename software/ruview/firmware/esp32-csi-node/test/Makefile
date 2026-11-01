# Makefile for ESP32 CSI firmware fuzz testing targets (ADR-061 Layer 6).
#
# Requirements:
#   - clang with libFuzzer support (clang 6.0+)
#   - Linux or macOS (host-based fuzzing, no ESP-IDF needed)
#
# Usage:
#   make all                  # Build all fuzz targets
#   make fuzz_serialize       # Build serialize target only
#   make fuzz_edge            # Build edge enqueue target only
#   make fuzz_nvs             # Build NVS config target only
#   make run_serialize        # Build and run serialize fuzzer (30s)
#   make run_edge             # Build and run edge fuzzer (30s)
#   make run_nvs              # Build and run NVS fuzzer (30s)
#   make run_all              # Run all fuzzers (30s each)
#   make clean                # Remove build artifacts
#
# Environment variables:
#   FUZZ_DURATION=60          # Override fuzz duration in seconds
#   FUZZ_JOBS=4               # Parallel fuzzing jobs

TEST_OUTPUT ?= .
CC       = clang
# ADR-110: -DCONFIG_CSI_FRAME_HE_TAGGING=1 enables the byte-18/19 HE path
# in csi_collector.c so the fuzzer exercises that code as well as the
# legacy zero-fill path. CONFIG_SOC_WIFI_HE_SUPPORT is left UNSET to
# exercise the legacy S3 branch (sig_mode/cwb/stbc). Add it to CFLAGS for
# a parallel HE-stub build if you want fuzz coverage of the C6 branch.
CFLAGS   = -fsanitize=fuzzer,address,undefined -g -O1 \
           -Istubs -I../main \
           -DCONFIG_CSI_NODE_ID=1 \
           -DCONFIG_CSI_WIFI_CHANNEL=6 \
           -DCONFIG_CSI_WIFI_SSID=\"test\" \
           -DCONFIG_CSI_TARGET_IP=\"192.168.1.1\" \
           -DCONFIG_CSI_TARGET_PORT=5500 \
           -DCONFIG_ESP_WIFI_CSI_ENABLED=1 \
           -DCONFIG_CSI_FRAME_HE_TAGGING=1 \
           -Wno-unused-function

STUBS_SRC = stubs/esp_stubs.c
MAIN_DIR  = ../main

# Default fuzz duration (seconds) and jobs
FUZZ_DURATION ?= 30
FUZZ_JOBS     ?= 1

.PHONY: all clean run_serialize run_edge run_nvs run_all test_adr110 run_adr110 \
        test_vitals run_vitals test_mmwave_detect run_mmwave_detect \
	        test_csi_source_binding run_csi_source_binding test_node_identity \
	        run_node_identity test_rx_diagnostics run_rx_diagnostics \
	        test_rx_raw_profile run_rx_raw_profile test_csi_samples run_csi_samples host_tests

all: fuzz_serialize fuzz_edge fuzz_nvs test_adr110 test_vitals test_mmwave_detect \
     test_csi_source_binding test_node_identity test_csi_samples

test_csi_samples: test_csi_samples.c $(MAIN_DIR)/csi_samples.h
	cc -std=c99 -Wall -Wextra -I$(MAIN_DIR) $< -o $(TEST_OUTPUT)/$@ -lm

run_csi_samples: test_csi_samples
	$(TEST_OUTPUT)/test_csi_samples

# --- Stable RX identity and LED palette ---
test_node_identity: test_node_identity.c $(MAIN_DIR)/node_identity.c $(MAIN_DIR)/node_identity.h
	cc -std=c99 -Wall -Wextra -I$(MAIN_DIR) \
	    test_node_identity.c $(MAIN_DIR)/node_identity.c -o $(TEST_OUTPUT)/$@

run_node_identity: test_node_identity
	$(TEST_OUTPUT)/test_node_identity

# --- ADR-110 encoding unit tests ---
# Host-side, no libFuzzer needed — plain C99 deterministic table tests
# for mac_to_eui64() and PPDU-type → ADR-018 byte 18 mapping.
# Builds with stock cc/gcc/clang — runs in CI on Ubuntu.
test_adr110: test_adr110_encoding.c
	cc -std=c99 -Wall -Wextra -o $(TEST_OUTPUT)/$@ $<

run_adr110: test_adr110
	$(TEST_OUTPUT)/test_adr110

# --- Vitals count + presence logic unit tests (issue #998 / #996) ---
# Host-side, no libFuzzer. Pins the person-count gate (no over-count for one
# body) and the presence hysteresis (no flicker on a dithering score). Pulls
# the named tuning constants from ../main/edge_processing.h so the test and the
# firmware can never disagree on thresholds.
test_vitals: test_vitals_count_presence.c $(MAIN_DIR)/edge_processing.h
	cc -std=c99 -Wall -Wextra -Istubs -I$(MAIN_DIR) -o $(TEST_OUTPUT)/$@ $< -lm

run_vitals: test_vitals
	$(TEST_OUTPUT)/test_vitals

# --- mmWave LD2410 detection predicate (#1135 bug #2) ---
# Host-side, no libFuzzer. Proves a floating-UART head pattern (0xF4F3F2F1)
# without a valid frame length+tail is REJECTED, so a phantom LD2410 is never
# detected on a node with no sensor wired. Tests the real predicate the
# firmware uses (../main/mmwave_detect.h) — test and firmware can't disagree.
test_mmwave_detect: test_mmwave_detect.c $(MAIN_DIR)/mmwave_detect.h
	cc -std=c99 -Wall -Wextra -I$(MAIN_DIR) -o $(TEST_OUTPUT)/$@ $<

run_mmwave_detect: test_mmwave_detect
	$(TEST_OUTPUT)/test_mmwave_detect

# --- ADR-018 runtime TX source-binding trailer ---
# Links the real serializer against host ESP-IDF/Mbed-TLS test doubles.
test_csi_source_binding: test_csi_source_binding.c $(MAIN_DIR)/csi_collector.c $(STUBS_SRC)
	cc -std=c99 -Wall -Wextra -Istubs -I$(MAIN_DIR) \
	    -DCONFIG_CSI_NODE_ID=1 \
	    -DCONFIG_CSI_WIFI_CHANNEL=6 \
	    -DCONFIG_CSI_WIFI_SSID=\"test\" \
	    -DCONFIG_CSI_TARGET_IP=\"192.0.2.1\" \
	    -DCONFIG_CSI_TARGET_PORT=5500 \
	    -DCONFIG_ESP_WIFI_CSI_ENABLED=1 \
	    -DCONFIG_CSI_FRAME_HE_TAGGING=1 \
	    $^ -o $(TEST_OUTPUT)/$@ -lm

run_csi_source_binding: test_csi_source_binding
	$(TEST_OUTPUT)/test_csi_source_binding

# --- RX callback/queue/sender diagnostic accounting ---
test_rx_diagnostics: test_rx_diagnostics.c $(MAIN_DIR)/csi_collector.c \
		$(MAIN_DIR)/csi_transport.c $(MAIN_DIR)/stream_sender.c $(STUBS_SRC)
	cc -std=c99 -Wall -Wextra -Istubs -I$(MAIN_DIR) \
	    -DCONFIG_CSI_NODE_ID=1 \
	    -DCONFIG_CSI_WIFI_CHANNEL=6 \
	    -DCONFIG_CSI_WIFI_SSID=\"test\" \
	    -DCONFIG_CSI_TARGET_IP=\"192.0.2.1\" \
	    -DCONFIG_CSI_TARGET_PORT=5500 \
	    -DCONFIG_ESP_WIFI_CSI_ENABLED=1 \
	    -DCONFIG_CSI_FRAME_HE_TAGGING=1 \
	    -DCSI_DIAGNOSTICS_TEST=1 \
	    -DCSI_DIAGNOSTICS_REAL_TRANSPORT=1 \
	    -DCSI_DIAGNOSTICS_THREAD_SAFE_STUBS=1 \
	    $^ -o $(TEST_OUTPUT)/$@ -lm -pthread

run_rx_diagnostics: test_rx_diagnostics
	$(TEST_OUTPUT)/test_rx_diagnostics

test_rx_raw_profile: test_rx_diagnostics.c $(MAIN_DIR)/csi_collector.c \
		$(MAIN_DIR)/csi_transport.c $(MAIN_DIR)/stream_sender.c $(STUBS_SRC)
	cc -std=c99 -Wall -Wextra -Istubs -I$(MAIN_DIR) \
	    -DCONFIG_CSI_NODE_ID=1 \
	    -DCONFIG_CSI_WIFI_CHANNEL=6 \
	    -DCONFIG_CSI_WIFI_SSID=\"test\" \
	    -DCONFIG_CSI_TARGET_IP=\"192.0.2.1\" \
	    -DCONFIG_CSI_TARGET_PORT=5500 \
	    -DCONFIG_ESP_WIFI_CSI_ENABLED=1 \
	    -DCONFIG_CSI_FRAME_HE_TAGGING=1 \
	    -DCONFIG_CSI_RAW_PROFILE=1 \
	    -DCSI_DIAGNOSTICS_TEST=1 \
	    -DCSI_DIAGNOSTICS_REAL_TRANSPORT=1 \
	    -DCSI_DIAGNOSTICS_THREAD_SAFE_STUBS=1 \
	    $^ -o $(TEST_OUTPUT)/$@ -lm -pthread

run_rx_raw_profile: test_rx_raw_profile
	$(TEST_OUTPUT)/test_rx_raw_profile

host_tests: run_adr110 run_vitals run_mmwave_detect run_csi_source_binding run_node_identity \
		run_rx_diagnostics run_rx_raw_profile run_csi_samples run_rx_unified
	@echo "Host tests passed (ADR-110 + vitals + mmwave detect + CSI source binding + identity + RX diagnostics + raw RX profile)"

# --- Serialize fuzzer ---
# Tests csi_serialize_frame() with random wifi_csi_info_t inputs.
# Links against the real csi_collector.c (with stubs for ESP-IDF).
fuzz_serialize: fuzz_csi_serialize.c $(MAIN_DIR)/csi_collector.c $(STUBS_SRC)
	$(CC) $(CFLAGS) $^ -o $(TEST_OUTPUT)/$@ -lm

# --- Edge enqueue fuzzer ---
# Tests the SPSC ring buffer push/pop logic with rapid-fire enqueues.
# Self-contained: reproduces ring buffer logic from edge_processing.c.
fuzz_edge: fuzz_edge_enqueue.c $(STUBS_SRC)
	$(CC) $(CFLAGS) $^ -o $(TEST_OUTPUT)/$@ -lm

# --- NVS config validation fuzzer ---
# Tests all NVS config validation ranges with random values.
# Self-contained: reproduces validation logic from nvs_config.c.
fuzz_nvs: fuzz_nvs_config.c $(STUBS_SRC)
	$(CC) $(CFLAGS) $^ -o $(TEST_OUTPUT)/$@ -lm

# --- Run targets ---
run_serialize: fuzz_serialize
	@mkdir -p corpus_serialize
	./fuzz_serialize corpus_serialize/ -max_total_time=$(FUZZ_DURATION) -max_len=2048 -jobs=$(FUZZ_JOBS)

run_edge: fuzz_edge
	@mkdir -p corpus_edge
	./fuzz_edge corpus_edge/ -max_total_time=$(FUZZ_DURATION) -max_len=4096 -jobs=$(FUZZ_JOBS)

run_nvs: fuzz_nvs
	@mkdir -p corpus_nvs
	./fuzz_nvs corpus_nvs/ -max_total_time=$(FUZZ_DURATION) -max_len=256 -jobs=$(FUZZ_JOBS)

run_all: run_serialize run_edge run_nvs

clean:
	rm -f fuzz_serialize fuzz_edge fuzz_nvs test_adr110 test_vitals \
	      test_mmwave_detect test_csi_source_binding test_node_identity \
	      test_rx_diagnostics test_rx_raw_profile test_csi_samples
	rm -rf corpus_serialize/ corpus_edge/ corpus_nvs/

# Unified S3 release contracts. Outputs may be kept on external measurement storage.
RX_TEST_FLAGS = -std=c99 -Wall -Wextra -Werror -fsanitize=address,undefined -g -Istubs -I$(MAIN_DIR)
RX_PROFILE_SRCS = $(MAIN_DIR)/rx_profile.c $(MAIN_DIR)/rx_settings_codec.c
CJSON_DIR ?= vendor/cjson

.PHONY: run_rx_unified test_rx_profile test_rx_settings test_rx_unified_transport

test_rx_profile: test_rx_profile.c $(RX_PROFILE_SRCS) $(MAIN_DIR)/rx_budget.c
	$(CC) $(RX_TEST_FLAGS) $^ -o $(TEST_OUTPUT)/$@

test_rx_settings: test_rx_settings.c $(RX_PROFILE_SRCS) $(MAIN_DIR)/rx_settings.c
	$(CC) $(RX_TEST_FLAGS) -DRX_SETTINGS_TEST $^ -o $(TEST_OUTPUT)/$@

test_rx_unified_transport: test_rx_unified_transport.c $(MAIN_DIR)/csi_collector.c \
	$(MAIN_DIR)/csi_transport.c $(MAIN_DIR)/stream_sender.c $(MAIN_DIR)/csi_peer.c \
	$(MAIN_DIR)/rx_profile.c $(MAIN_DIR)/rx_budget.c $(MAIN_DIR)/rx_diag.c $(STUBS_SRC)
	$(CC) $(RX_TEST_FLAGS) -DCONFIG_RX_UNIFIED=1 -DCONFIG_CSI_RAW_PROFILE=1 \
		-DCONFIG_CSI_FRAME_HE_TAGGING=1 -DCSI_DIAGNOSTICS_TEST=1 \
		-DCSI_DIAGNOSTICS_REAL_TRANSPORT=1 -DCSI_DIAGNOSTICS_THREAD_SAFE_STUBS=1 \
		$^ -o $(TEST_OUTPUT)/$@ -lm -pthread

test_rx_control: test_rx_control.c $(RX_PROFILE_SRCS) $(STUBS_SRC) $(CJSON_DIR)/cJSON.c
	$(CC) $(RX_TEST_FLAGS) -Wno-deprecated-declarations -DCONFIG_RX_UNIFIED=1 \
		-I$(CJSON_DIR) $^ -o $(TEST_OUTPUT)/$@ -lm

test_rx_wifi: test_rx_wifi.c $(STUBS_SRC)
	$(CC) $(RX_TEST_FLAGS) -std=gnu11 -Wno-unused-parameter -DRX_WIFI_TEST=1 \
		-DCSI_DIAGNOSTICS_THREAD_SAFE_STUBS=1 $^ -o $(TEST_OUTPUT)/$@ -lm -pthread

test_rx_ota: test_rx_ota.c $(STUBS_SRC) $(CJSON_DIR)/cJSON.c
	$(CC) $(RX_TEST_FLAGS) -std=gnu11 -Wno-deprecated-declarations -Wno-unused-parameter \
		-DRX_OTA_TEST=1 -DCONFIG_RX_UNIFIED=1 -DCSI_DIAGNOSTICS_REAL_TRANSPORT=1 \
		-I$(CJSON_DIR) $^ -o $(TEST_OUTPUT)/$@ -lm

test_rx_main: test_rx_main.c $(MAIN_DIR)/rx_profile.c
	$(CC) $(RX_TEST_FLAGS) -DRX_MAIN_TEST=1 -DCONFIG_RX_UNIFIED=1 $^ -o $(TEST_OUTPUT)/$@

run_rx_unified: test_rx_profile test_rx_settings test_rx_unified_transport test_rx_control test_rx_wifi test_rx_ota test_rx_main
	$(TEST_OUTPUT)/test_rx_profile
	$(TEST_OUTPUT)/test_rx_settings
	$(TEST_OUTPUT)/test_rx_unified_transport 0
	$(TEST_OUTPUT)/test_rx_unified_transport 1
	$(TEST_OUTPUT)/test_rx_unified_transport 2
	$(TEST_OUTPUT)/test_rx_unified_transport 3
	$(TEST_OUTPUT)/test_rx_unified_transport 4
	$(TEST_OUTPUT)/test_rx_control
	$(TEST_OUTPUT)/test_rx_wifi
	$(TEST_OUTPUT)/test_rx_ota
	$(TEST_OUTPUT)/test_rx_main
