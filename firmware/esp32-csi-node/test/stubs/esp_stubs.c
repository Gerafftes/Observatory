/**
 * @file esp_stubs.c
 * @brief Implementation of ESP-IDF stubs for host-based fuzz testing.
 *
 * Must be compiled with: -Istubs -I../main
 * so that ESP-IDF headers resolve to stubs/ and firmware headers
 * resolve to ../main/.
 */

#include "esp_stubs.h"
#include "edge_processing.h"
#include "wasm_runtime.h"
#include <stdint.h>
#include <string.h>

/** Monotonically increasing microsecond counter for esp_timer_get_time(). */
static int64_t s_fake_time_us = 0;

int64_t esp_timer_get_time(void)
{
    /* Advance by 50ms each call (~20 Hz CSI rate simulation). */
    s_fake_time_us += 50000;
    return s_fake_time_us;
}

/* ---- stream_sender stubs ---- */

int stream_sender_send(const uint8_t *data, size_t len)
{
    (void)data;
    return (int)len;
}

int stream_sender_init(void)
{
    return 0;
}

int stream_sender_init_with(const char *ip, uint16_t port)
{
    (void)ip; (void)port;
    return 0;
}

void stream_sender_deinit(void)
{
}

/* ---- wasm_runtime stubs ---- */

void wasm_runtime_on_frame(const float *phases, const float *amplitudes,
                           const float *variances, uint16_t n_sc,
                           const edge_vitals_pkt_t *vitals)
{
    (void)phases; (void)amplitudes; (void)variances;
    (void)n_sc; (void)vitals;
}

esp_err_t wasm_runtime_init(void) { return ESP_OK; }
esp_err_t wasm_runtime_load(const uint8_t *d, uint32_t l, uint8_t *id) { (void)d; (void)l; (void)id; return ESP_OK; }
esp_err_t wasm_runtime_start(uint8_t id) { (void)id; return ESP_OK; }
esp_err_t wasm_runtime_stop(uint8_t id) { (void)id; return ESP_OK; }
esp_err_t wasm_runtime_unload(uint8_t id) { (void)id; return ESP_OK; }
void wasm_runtime_on_timer(void) {}
void wasm_runtime_get_info(wasm_module_info_t *info, uint8_t *count) { (void)info; if(count) *count = 0; }
esp_err_t wasm_runtime_set_manifest(uint8_t id, const char *n, uint32_t c, uint32_t m) { (void)id; (void)n; (void)c; (void)m; return ESP_OK; }

/* ---- mmwave_sensor stubs (ADR-063) ---- */

#include "mmwave_sensor.h"

static mmwave_state_t s_stub_mmwave = {0};

esp_err_t mmwave_sensor_init(int tx, int rx) { (void)tx; (void)rx; return ESP_ERR_NOT_FOUND; }
bool mmwave_sensor_get_state(mmwave_state_t *s) { if (s) *s = s_stub_mmwave; return false; }
const char *mmwave_type_name(mmwave_type_t t) { (void)t; return "None"; }

/* ADR-110 iter 38 — fuzz-harness stub for c6_sync_espnow_is_valid.
 * Real implementation lives in main/c6_sync_espnow.c; the fuzz target
 * (`fuzz_serialize`) only links csi_collector.c against esp_stubs.c, so
 * iter-11's `if (c6_sync_espnow_is_valid()) flags |= (1 << 4);` needs a
 * symbol here or `clang -fsanitize=fuzzer` fails with an undefined-reference
 * linker error. Returning false means the bit-4 cross-node-sync-valid flag
 * stays 0 in fuzz inputs, which is the natural fuzz semantic. */
#include <stdbool.h>
bool c6_sync_espnow_is_valid(void) { return false; }
uint64_t c6_sync_espnow_get_epoch_us(void) { return 0; }
bool c6_sync_espnow_is_leader(void) { return false; }
int64_t c6_sync_espnow_get_offset_us_smoothed(void) { return 0; }

bool edge_enqueue_csi(const uint8_t *iq_data, uint16_t iq_len,
                      int8_t rssi, uint8_t channel)
{
    (void)iq_data;
    (void)iq_len;
    (void)rssi;
    (void)channel;
    return true;
}

/* ---- Mbed TLS SHA-256 test double ---- */

int mbedtls_sha256(const unsigned char *input, size_t ilen,
                   unsigned char output[32], int is224)
{
    static const unsigned char test_input[6] = {
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
    };
    static const unsigned char test_digest[32] = {
        0x48, 0xf4, 0x63, 0x4d, 0x10, 0x02, 0xf9, 0xf3,
        0xc7, 0x57, 0x0c, 0xb4, 0x3e, 0x00, 0xdd, 0x86,
        0x9b, 0x22, 0xc7, 0x95, 0x38, 0xe9, 0xb4, 0xad,
        0xc7, 0xe4, 0x02, 0xde, 0x11, 0x89, 0xcf, 0xe1,
    };

    if (input == NULL || output == NULL || is224 != 0) {
        return ESP_FAIL;
    }
    if (ilen == sizeof(test_input)
        && memcmp(input, test_input, sizeof(test_input)) == 0) {
        memcpy(output, test_digest, sizeof(test_digest));
        return 0;
    }

    /* Fuzz inputs only need a deterministic, memory-safe digest substitute.
     * The fixed vector above is the cryptographic contract test; ESP-IDF uses
     * the real Mbed TLS implementation in production. */
    for (size_t i = 0; i < 32; i++) {
        output[i] = ilen == 0
            ? (unsigned char)i
            : (unsigned char)(input[i % ilen] ^ (unsigned char)i);
    }
    return 0;
}
