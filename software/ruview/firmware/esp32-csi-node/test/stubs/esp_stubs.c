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
#include "csi_transport.h"
#include "wasm_runtime.h"
#include <stdint.h>
#include <string.h>

/** Monotonically increasing microsecond counter for esp_timer_get_time(). */
static int64_t s_fake_time_us = 0;
static int64_t s_fake_time_step_us = 50000;

int64_t esp_timer_get_time(void)
{
    int64_t step_us = __atomic_load_n(&s_fake_time_step_us, __ATOMIC_RELAXED);
    return __atomic_add_fetch(&s_fake_time_us, step_us, __ATOMIC_RELAXED);
}

void esp_test_set_time_us(int64_t time_us) {
    __atomic_store_n(&s_fake_time_us, time_us, __ATOMIC_RELAXED);
}
void esp_test_set_time_step_us(int64_t step_us) {
    __atomic_store_n(&s_fake_time_step_us, step_us, __ATOMIC_RELAXED);
}
void esp_test_advance_time_us(int64_t delta_us) {
    (void)__atomic_add_fetch(&s_fake_time_us, delta_us, __ATOMIC_RELAXED);
}

QueueHandle_t xQueueCreateStatic(size_t capacity, size_t item_size,
                                uint8_t *storage, StaticQueue_t *control)
{
    if (capacity == 0 || item_size == 0 || storage == NULL || control == NULL) {
        return NULL;
    }
    control->storage = storage;
    control->item_size = item_size;
    control->capacity = capacity;
    control->head = 0;
    control->tail = 0;
    control->count = 0;
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    if (control->lock_initialized) {
        (void)pthread_mutex_destroy(&control->lock);
    }
    (void)pthread_mutex_init(&control->lock, NULL);
    control->lock_initialized = true;
#endif
    return control;
}

BaseType_t xQueueSend(QueueHandle_t queue, const void *item, uint32_t wait_ticks)
{
    (void)wait_ticks;
    if (queue == NULL || item == NULL) {
        return pdFALSE;
    }
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    (void)pthread_mutex_lock(&queue->lock);
#endif
    if (queue->count == queue->capacity) {
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
        (void)pthread_mutex_unlock(&queue->lock);
#endif
        return pdFALSE;
    }
    memcpy(queue->storage + queue->tail * queue->item_size, item, queue->item_size);
    queue->tail = (queue->tail + 1) % queue->capacity;
    queue->count++;
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    (void)pthread_mutex_unlock(&queue->lock);
#endif
    return pdTRUE;
}

BaseType_t xQueueReceive(QueueHandle_t queue, void *item, uint32_t wait_ticks)
{
    (void)wait_ticks;
    if (queue == NULL || item == NULL) {
        return pdFALSE;
    }
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    (void)pthread_mutex_lock(&queue->lock);
#endif
    if (queue->count == 0) {
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
        (void)pthread_mutex_unlock(&queue->lock);
#endif
        return pdFALSE;
    }
    memcpy(item, queue->storage + queue->head * queue->item_size, queue->item_size);
    queue->head = (queue->head + 1) % queue->capacity;
    queue->count--;
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    (void)pthread_mutex_unlock(&queue->lock);
#endif
    return pdTRUE;
}

size_t uxQueueMessagesWaiting(QueueHandle_t queue)
{
    if (queue == NULL) {
        return 0;
    }
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    (void)pthread_mutex_lock(&queue->lock);
#endif
    size_t count = queue->count;
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    (void)pthread_mutex_unlock(&queue->lock);
#endif
    return count;
}

SemaphoreHandle_t xSemaphoreCreateMutexStatic(StaticSemaphore_t *storage)
{
    if (storage == NULL) {
        return NULL;
    }
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    (void)pthread_mutex_init(storage, NULL);
#else
    *storage = 0;
#endif
    return storage;
}

BaseType_t xSemaphoreTake(SemaphoreHandle_t semaphore, uint32_t wait_ticks)
{
    (void)wait_ticks;
    if (semaphore == NULL) {
        return pdFALSE;
    }
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    return pthread_mutex_lock(semaphore) == 0 ? pdTRUE : pdFALSE;
#else
    return pdTRUE;
#endif
}

BaseType_t xSemaphoreGive(SemaphoreHandle_t semaphore)
{
    if (semaphore == NULL) {
        return pdFALSE;
    }
#ifdef CSI_DIAGNOSTICS_THREAD_SAFE_STUBS
    return pthread_mutex_unlock(semaphore) == 0 ? pdTRUE : pdFALSE;
#else
    return pdTRUE;
#endif
}

/* ---- stream_sender stubs ---- */

#ifndef CSI_DIAGNOSTICS_REAL_TRANSPORT
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

esp_err_t csi_transport_start(void)
{
    return ESP_OK;
}

void csi_transport_stop(void)
{
}

bool csi_transport_enqueue_capture(const wifi_csi_info_t *info,
                                   int64_t callback_started_at_us)
{
    (void)callback_started_at_us;
    return info != NULL && info->buf != NULL && info->len > 0;
}

void csi_transport_get_stats(csi_transport_stats_t *stats)
{
    if (stats != NULL) {
        memset(stats, 0, sizeof(*stats));
    }
}
#endif

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
uint32_t c6_sync_espnow_get_age_ms(void) { return UINT32_MAX; }

bool edge_enqueue_csi(const uint8_t *iq_data, uint16_t iq_len,
                      int8_t rssi, uint8_t channel)
{
    (void)iq_data;
    (void)iq_len;
    (void)rssi;
    (void)channel;
    return true;
}

static uint32_t s_edge_enqueue_count;
static uint32_t s_edge_timestamp_us;
static bool s_edge_enqueue_result = true;

bool edge_enqueue_csi_at(const uint8_t *iq_data, uint16_t iq_len,
                         int8_t rssi, uint8_t channel,
                         uint32_t callback_started_at_us)
{
    (void)iq_data;
    (void)iq_len;
    (void)rssi;
    (void)channel;
    __atomic_add_fetch(&s_edge_enqueue_count, 1, __ATOMIC_RELAXED);
    __atomic_store_n(&s_edge_timestamp_us, callback_started_at_us, __ATOMIC_RELAXED);
    return __atomic_load_n(&s_edge_enqueue_result, __ATOMIC_RELAXED);
}

void esp_test_reset_edge_enqueue(void)
{
    __atomic_store_n(&s_edge_enqueue_count, 0, __ATOMIC_RELAXED);
    __atomic_store_n(&s_edge_timestamp_us, 0, __ATOMIC_RELAXED);
    __atomic_store_n(&s_edge_enqueue_result, true, __ATOMIC_RELAXED);
}

void esp_test_set_edge_enqueue_result(bool accepted)
{
    __atomic_store_n(&s_edge_enqueue_result, accepted, __ATOMIC_RELAXED);
}

uint32_t esp_test_edge_enqueue_count(void)
{
    return __atomic_load_n(&s_edge_enqueue_count, __ATOMIC_RELAXED);
}

uint32_t esp_test_edge_timestamp_us(void)
{
    return __atomic_load_n(&s_edge_timestamp_us, __ATOMIC_RELAXED);
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
