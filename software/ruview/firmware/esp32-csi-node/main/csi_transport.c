/**
 * @file csi_transport.c
 * @brief Bounded CSI capture pool, processing queue, and UDP sender task.
 */

#include "csi_transport.h"

#include <string.h>

#include "c6_sync_espnow.h"
#include "csi_collector.h"
#include "edge_processing.h"
#include "esp_log.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
#include "freertos/task.h"
#include "stream_sender.h"

#define CSI_TRANSPORT_MAX_ATTEMPTS 3u
#define CSI_TRANSPORT_MAX_AGE_US 100000LL
#define CSI_TRANSPORT_STOP_SLOT UINT8_MAX

#ifndef CSI_TRANSPORT_TASK_STACK_BYTES
#ifdef CONFIG_CSI_TRANSPORT_TASK_STACK_BYTES
#define CSI_TRANSPORT_TASK_STACK_BYTES CONFIG_CSI_TRANSPORT_TASK_STACK_BYTES
#else
#define CSI_TRANSPORT_TASK_STACK_BYTES 8192u
#endif
#endif

#ifndef CONFIG_CSI_TRANSPORT_TASK_PRIORITY
#define CONFIG_CSI_TRANSPORT_TASK_PRIORITY 5
#endif

#ifndef CONFIG_C6_SYNC_EVERY_N_FRAMES
#define CONFIG_C6_SYNC_EVERY_N_FRAMES 20
#endif

typedef enum {
    CSI_SLOT_FREE,
    CSI_SLOT_COPYING,
    CSI_SLOT_QUEUED,
    CSI_SLOT_PROCESSING,
} csi_slot_state_t;

typedef struct {
    csi_slot_state_t state;
    int64_t callback_started_at_us;
    int64_t enqueued_at_us;
    wifi_pkt_rx_ctrl_t rx_ctrl;
    uint8_t mac[6];
    bool first_word_invalid;
    uint16_t iq_len;
    int8_t iq_data[CSI_MAX_IQ_SIZE];
} csi_capture_slot_t;

typedef struct {
    bool delivered;
    bool expired;
} csi_send_result_t;

static const char *TAG = "csi_transport";
static StaticQueue_t s_queue_control;
static uint8_t s_queue_storage[CSI_TRANSPORT_CAPTURE_SLOTS * sizeof(uint8_t)];
static QueueHandle_t s_queue;
static TaskHandle_t s_processing_task;
static bool s_accepting;
static bool s_stop_requested;
static csi_capture_slot_t s_slots[CSI_TRANSPORT_CAPTURE_SLOTS];
static uint8_t s_serialized_frame[CSI_MAX_FRAME_SIZE];
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;

static volatile uint32_t s_captures_enqueued;
static volatile uint32_t s_start_failures;
static volatile uint32_t s_not_ready_drops;
static volatile uint32_t s_invalid_drops;
static volatile uint32_t s_pool_exhausted_drops;
static volatile uint32_t s_stale_queued_replacements;
static volatile uint32_t s_queue_publish_failures;
static volatile uint32_t s_ownership_errors;
static volatile uint32_t s_captures_waiting;
static volatile uint32_t s_captures_in_flight;
static volatile uint32_t s_captures_aborted;
static volatile uint32_t s_pool_in_use;
static volatile uint32_t s_pool_high_watermark;
static volatile uint32_t s_queue_high_watermark;
static volatile uint32_t s_serialization_failures;
static volatile uint32_t s_serialized_frames;
static volatile uint32_t s_csi_send_calls;
static volatile uint32_t s_retries;
static volatile uint32_t s_transport_errors;
static volatile uint32_t s_frames_os_accepted;
static volatile uint32_t s_frames_abandoned;
static volatile uint32_t s_frames_expired;
static volatile uint32_t s_edge_enqueue_accepted;
static volatile uint32_t s_edge_enqueue_drops;
static volatile uint32_t s_edge_enqueue_skipped;
static volatile uint32_t s_sync_packets_attempted;
static volatile uint32_t s_sync_packets_sent;
static uint64_t s_queue_age_total_us;
static volatile uint32_t s_queue_age_max_us;
static volatile uint32_t s_queue_age_last_us;
static volatile uint32_t s_queue_age_samples;
static uint64_t s_callback_to_dequeue_total_us;
static volatile uint32_t s_callback_to_dequeue_max_us;
static volatile uint32_t s_callback_to_dequeue_last_us;

#ifdef CSI_DIAGNOSTICS_TEST
static bool s_test_fail_next_publish;
static bool s_test_fail_next_serialization;
#endif

static uint32_t elapsed_us_saturated(int64_t now_us, int64_t then_us)
{
    int64_t elapsed_us = now_us - then_us;
    if (elapsed_us <= 0) {
        return 0;
    }
    return elapsed_us > UINT32_MAX ? UINT32_MAX : (uint32_t)elapsed_us;
}

static void release_slot_locked(uint8_t slot_index)
{
    s_slots[slot_index].state = CSI_SLOT_FREE;
    if (s_pool_in_use > 0) {
        s_pool_in_use--;
    }
}

static int claim_slot(void)
{
    int slot_index = -1;
    portENTER_CRITICAL(&s_lock);
    if (!s_accepting || s_queue == NULL) {
        s_not_ready_drops++;
    } else {
        for (uint8_t index = 0; index < CSI_TRANSPORT_CAPTURE_SLOTS; ++index) {
            if (s_slots[index].state == CSI_SLOT_FREE) {
                s_slots[index].state = CSI_SLOT_COPYING;
                s_pool_in_use++;
                if (s_pool_in_use > s_pool_high_watermark) {
                    s_pool_high_watermark = s_pool_in_use;
                }
                slot_index = index;
                break;
            }
        }
    }
    portEXIT_CRITICAL(&s_lock);
    return slot_index;
}

static bool evict_oldest_queued_capture(void)
{
    portENTER_CRITICAL(&s_lock);
    QueueHandle_t queue = s_queue;
    bool accepting = s_accepting;
    portEXIT_CRITICAL(&s_lock);
    if (!accepting || queue == NULL) {
        return false;
    }

    uint8_t slot_index = CSI_TRANSPORT_STOP_SLOT;
    if (xQueueReceive(queue, &slot_index, 0) != pdTRUE) {
        return false;
    }
    if (slot_index == CSI_TRANSPORT_STOP_SLOT) {
        (void)xQueueSend(queue, &slot_index, 0);
        return false;
    }

    bool evicted = false;
    portENTER_CRITICAL(&s_lock);
    if (slot_index < CSI_TRANSPORT_CAPTURE_SLOTS
        && s_slots[slot_index].state == CSI_SLOT_QUEUED) {
        if (s_captures_waiting > 0) {
            s_captures_waiting--;
        }
        s_captures_aborted++;
        s_stale_queued_replacements++;
        release_slot_locked(slot_index);
        evicted = true;
    } else {
        s_ownership_errors++;
    }
    portEXIT_CRITICAL(&s_lock);
    return evicted;
}

static bool begin_processing(uint8_t slot_index)
{
    if (slot_index >= CSI_TRANSPORT_CAPTURE_SLOTS) {
        portENTER_CRITICAL(&s_lock);
        s_ownership_errors++;
        portEXIT_CRITICAL(&s_lock);
        return false;
    }

    csi_capture_slot_t *slot = &s_slots[slot_index];
    int64_t dequeued_at_us = esp_timer_get_time();
    portENTER_CRITICAL(&s_lock);
    if (slot->state != CSI_SLOT_QUEUED) {
        s_ownership_errors++;
        portEXIT_CRITICAL(&s_lock);
        return false;
    }

    slot->state = CSI_SLOT_PROCESSING;
    if (s_captures_waiting > 0) {
        s_captures_waiting--;
    }
    s_captures_in_flight++;

    uint32_t queue_age_us = elapsed_us_saturated(dequeued_at_us, slot->enqueued_at_us);
    uint32_t callback_to_dequeue_us = elapsed_us_saturated(
        dequeued_at_us,
        slot->callback_started_at_us
    );
    s_queue_age_total_us += queue_age_us;
    s_queue_age_last_us = queue_age_us;
    if (queue_age_us > s_queue_age_max_us) {
        s_queue_age_max_us = queue_age_us;
    }
    s_queue_age_samples++;
    s_callback_to_dequeue_total_us += callback_to_dequeue_us;
    s_callback_to_dequeue_last_us = callback_to_dequeue_us;
    if (callback_to_dequeue_us > s_callback_to_dequeue_max_us) {
        s_callback_to_dequeue_max_us = callback_to_dequeue_us;
    }
    portEXIT_CRITICAL(&s_lock);
    return true;
}

static bool abort_queued_slot(uint8_t slot_index)
{
    bool no_waiting_captures = false;
    portENTER_CRITICAL(&s_lock);
    if (slot_index >= CSI_TRANSPORT_CAPTURE_SLOTS
        || s_slots[slot_index].state != CSI_SLOT_QUEUED) {
        s_ownership_errors++;
    } else {
        if (s_captures_waiting > 0) {
            s_captures_waiting--;
        }
        s_captures_aborted++;
        release_slot_locked(slot_index);
        no_waiting_captures = s_captures_waiting == 0;
    }
    portEXIT_CRITICAL(&s_lock);
    return no_waiting_captures;
}

static csi_send_result_t send_csi_frame(
    const uint8_t *data,
    size_t len,
    int64_t callback_started_at_us
)
{
    csi_send_result_t outcome = {0};
    for (uint32_t attempt = 0; attempt < CSI_TRANSPORT_MAX_ATTEMPTS; ++attempt) {
        if (esp_timer_get_time() - callback_started_at_us > CSI_TRANSPORT_MAX_AGE_US) {
            outcome.expired = true;
            break;
        }
        if (attempt > 0) {
            portENTER_CRITICAL(&s_lock);
            s_retries++;
            portEXIT_CRITICAL(&s_lock);
            vTaskDelay(pdMS_TO_TICKS(10u * attempt));
            if (esp_timer_get_time() - callback_started_at_us > CSI_TRANSPORT_MAX_AGE_US) {
                outcome.expired = true;
                break;
            }
        }

        portENTER_CRITICAL(&s_lock);
        s_csi_send_calls++;
        portEXIT_CRITICAL(&s_lock);
        bool deadline_expired = false;
        int result = stream_sender_send_until(
            data,
            len,
            callback_started_at_us + CSI_TRANSPORT_MAX_AGE_US,
            &deadline_expired
        );
        if (deadline_expired) {
            outcome.expired = true;
            break;
        }
        if (result == (int)len) {
            outcome.delivered = true;
            break;
        }
        portENTER_CRITICAL(&s_lock);
        s_transport_errors++;
        portEXIT_CRITICAL(&s_lock);
    }
    return outcome;
}

static void send_sync_packet_if_due(void)
{
    uint32_t sequence = csi_collector_get_sequence();
    if (CONFIG_C6_SYNC_EVERY_N_FRAMES == 0
        || sequence == 0
        || (sequence % CONFIG_C6_SYNC_EVERY_N_FRAMES) != 0) {
        return;
    }

    uint8_t sync[32];
    uint32_t sync_magic = 0xC511A110u;
    uint64_t local_us = (uint64_t)esp_timer_get_time();
    uint64_t epoch_us = c6_sync_espnow_get_epoch_us();
    int64_t offset_smoothed = c6_sync_espnow_get_offset_us_smoothed();
    uint8_t flags = 0;
    if (c6_sync_espnow_is_leader()) flags |= 0x01;
    if (c6_sync_espnow_is_valid())  flags |= 0x02;
    if (offset_smoothed != 0)       flags |= 0x04;

    memcpy(&sync[0], &sync_magic, 4);
    sync[4] = csi_collector_get_node_id();
    sync[5] = 0x01;
    sync[6] = flags;
    sync[7] = 0;
    memcpy(&sync[8], &local_us, 8);
    memcpy(&sync[16], &epoch_us, 8);
    memcpy(&sync[24], &sequence, 4);
    uint32_t zero32 = 0;
    memcpy(&sync[28], &zero32, 4);

    portENTER_CRITICAL(&s_lock);
    s_sync_packets_attempted++;
    portEXIT_CRITICAL(&s_lock);
    if (stream_sender_send(sync, sizeof(sync)) == (int)sizeof(sync)) {
        portENTER_CRITICAL(&s_lock);
        s_sync_packets_sent++;
        portEXIT_CRITICAL(&s_lock);
    }
}

static void process_slot(uint8_t slot_index)
{
    if (!begin_processing(slot_index)) {
        return;
    }

    csi_capture_slot_t *slot = &s_slots[slot_index];
    wifi_csi_info_t info = {
        .rx_ctrl = slot->rx_ctrl,
        .first_word_invalid = slot->first_word_invalid,
        .len = (int16_t)slot->iq_len,
        .buf = slot->iq_data,
    };
    memcpy(info.mac, slot->mac, sizeof(info.mac));

    bool force_serialization_failure = false;
#ifdef CSI_DIAGNOSTICS_TEST
    portENTER_CRITICAL(&s_lock);
    force_serialization_failure = s_test_fail_next_serialization;
    s_test_fail_next_serialization = false;
    portEXIT_CRITICAL(&s_lock);
#endif
    size_t frame_len = force_serialization_failure
        ? 0
        : csi_serialize_frame_at(
            &info,
            slot->callback_started_at_us,
            s_serialized_frame,
            sizeof(s_serialized_frame)
        );

    bool serialization_failed = frame_len == 0;
    csi_send_result_t send_result = {0};
    if (!serialization_failed) {
        portENTER_CRITICAL(&s_lock);
        s_serialized_frames++;
        portEXIT_CRITICAL(&s_lock);
        send_result = send_csi_frame(
            s_serialized_frame,
            frame_len,
            slot->callback_started_at_us
        );
        send_sync_packet_if_due();
    }

#if defined(CONFIG_CSI_RAW_PROFILE) && CONFIG_CSI_RAW_PROFILE
    bool edge_accepted = false;
#else
    bool edge_accepted = edge_enqueue_csi_at(
        (const uint8_t *)slot->iq_data,
        slot->iq_len,
        (int8_t)slot->rx_ctrl.rssi,
        slot->rx_ctrl.channel,
        (uint32_t)((uint64_t)slot->callback_started_at_us & UINT32_MAX)
    );
#endif

    portENTER_CRITICAL(&s_lock);
    if (serialization_failed) {
        s_serialization_failures++;
    } else if (send_result.delivered) {
        s_frames_os_accepted++;
    } else {
        s_frames_abandoned++;
        if (send_result.expired) {
            s_frames_expired++;
        }
    }
#if defined(CONFIG_CSI_RAW_PROFILE) && CONFIG_CSI_RAW_PROFILE
    (void)edge_accepted;
    s_edge_enqueue_skipped++;
#else
    if (edge_accepted) {
        s_edge_enqueue_accepted++;
    } else {
        s_edge_enqueue_drops++;
    }
#endif
    if (s_captures_in_flight > 0) {
        s_captures_in_flight--;
    }
    release_slot_locked(slot_index);
    portEXIT_CRITICAL(&s_lock);
}

static void csi_processing_task(void *arg)
{
    (void)arg;
    uint8_t slot_index;

    for (;;) {
        if (xQueueReceive(s_queue, &slot_index, portMAX_DELAY) != pdTRUE) {
            continue;
        }
        if (slot_index == CSI_TRANSPORT_STOP_SLOT) {
            break;
        }

        portENTER_CRITICAL(&s_lock);
        bool stop_requested = s_stop_requested;
        portEXIT_CRITICAL(&s_lock);
        if (stop_requested) {
            if (abort_queued_slot(slot_index)) {
                break;
            }
            continue;
        }
        process_slot(slot_index);
    }

    portENTER_CRITICAL(&s_lock);
    s_processing_task = NULL;
    portEXIT_CRITICAL(&s_lock);
    vTaskDelete(NULL);
}

esp_err_t csi_transport_start(void)
{
    portENTER_CRITICAL(&s_lock);
    if (s_accepting && s_queue != NULL) {
        portEXIT_CRITICAL(&s_lock);
        return ESP_OK;
    }
    portEXIT_CRITICAL(&s_lock);

    QueueHandle_t queue = xQueueCreateStatic(
        CSI_TRANSPORT_CAPTURE_SLOTS,
        sizeof(uint8_t),
        s_queue_storage,
        &s_queue_control
    );
    if (queue == NULL) {
        portENTER_CRITICAL(&s_lock);
        s_start_failures++;
        portEXIT_CRITICAL(&s_lock);
        return ESP_ERR_NO_MEM;
    }

    portENTER_CRITICAL(&s_lock);
    s_queue = queue;
    s_stop_requested = false;
    portEXIT_CRITICAL(&s_lock);
    TaskHandle_t task = NULL;
    BaseType_t created;
#if defined(CONFIG_CSI_TRANSPORT_TASK_CORE) \
        && CONFIG_CSI_TRANSPORT_TASK_CORE >= 0
    created = xTaskCreatePinnedToCore(
        csi_processing_task,
        "csi_process",
        CSI_TRANSPORT_TASK_STACK_BYTES,
        NULL,
        CONFIG_CSI_TRANSPORT_TASK_PRIORITY,
        &task,
        CONFIG_CSI_TRANSPORT_TASK_CORE
    );
#else
    created = xTaskCreate(
        csi_processing_task,
        "csi_process",
        CSI_TRANSPORT_TASK_STACK_BYTES,
        NULL,
        CONFIG_CSI_TRANSPORT_TASK_PRIORITY,
        &task
    );
#endif
    if (created != pdPASS) {
        portENTER_CRITICAL(&s_lock);
        s_queue = NULL;
        s_start_failures++;
        portEXIT_CRITICAL(&s_lock);
        return ESP_ERR_NO_MEM;
    }

    portENTER_CRITICAL(&s_lock);
    s_processing_task = task;
    s_accepting = true;
    portEXIT_CRITICAL(&s_lock);
    ESP_LOGI(TAG, "capture processor ready (slots=%u, stack=%u, priority=%u, core=%d, max_age=%u ms)",
             (unsigned)CSI_TRANSPORT_CAPTURE_SLOTS,
             (unsigned)CSI_TRANSPORT_TASK_STACK_BYTES,
             (unsigned)CONFIG_CSI_TRANSPORT_TASK_PRIORITY,
#if defined(CONFIG_CSI_TRANSPORT_TASK_CORE)
             CONFIG_CSI_TRANSPORT_TASK_CORE,
#else
             -1,
#endif
             (unsigned)(CSI_TRANSPORT_MAX_AGE_US / 1000));
    return ESP_OK;
}

void csi_transport_stop(void)
{
    portENTER_CRITICAL(&s_lock);
    s_accepting = false;
    TaskHandle_t task = s_processing_task;
    QueueHandle_t queue = s_queue;
    s_stop_requested = true;
    portEXIT_CRITICAL(&s_lock);

    if (task != NULL && queue != NULL) {
        uint8_t stop_slot = CSI_TRANSPORT_STOP_SLOT;
        (void)xQueueSend(queue, &stop_slot, portMAX_DELAY);
#ifdef CSI_DIAGNOSTICS_TEST
        uint8_t slot_index;
        while (xQueueReceive(queue, &slot_index, 0) == pdTRUE) {
            if (slot_index == CSI_TRANSPORT_STOP_SLOT) {
                break;
            }
            (void)abort_queued_slot(slot_index);
        }
        portENTER_CRITICAL(&s_lock);
        s_processing_task = NULL;
        portEXIT_CRITICAL(&s_lock);
#else
        for (;;) {
            portENTER_CRITICAL(&s_lock);
            bool stopped = s_processing_task == NULL;
            portEXIT_CRITICAL(&s_lock);
            if (stopped) {
                break;
            }
            vTaskDelay(pdMS_TO_TICKS(1));
        }
#endif
    }

    portENTER_CRITICAL(&s_lock);
    for (uint8_t index = 0; index < CSI_TRANSPORT_CAPTURE_SLOTS; ++index) {
        if (s_slots[index].state != CSI_SLOT_FREE) {
            s_captures_aborted++;
        }
        s_slots[index].state = CSI_SLOT_FREE;
    }
    s_captures_waiting = 0;
    s_captures_in_flight = 0;
    s_pool_in_use = 0;
    s_queue = NULL;
    s_stop_requested = false;
    portEXIT_CRITICAL(&s_lock);
}

bool csi_transport_enqueue_capture(
    const wifi_csi_info_t *info,
    int64_t callback_started_at_us
)
{
    if (info == NULL || info->buf == NULL || info->len <= 0
        || (size_t)info->len > CSI_MAX_IQ_SIZE
        || (info->first_word_invalid && info->len < 6)) {
        portENTER_CRITICAL(&s_lock);
        s_invalid_drops++;
        portEXIT_CRITICAL(&s_lock);
        return false;
    }

    int claimed = claim_slot();
    if (claimed < 0 && evict_oldest_queued_capture()) {
        claimed = claim_slot();
    }
    if (claimed < 0) {
        portENTER_CRITICAL(&s_lock);
        s_pool_exhausted_drops++;
        portEXIT_CRITICAL(&s_lock);
        return false;
    }
    uint8_t slot_index = (uint8_t)claimed;
    csi_capture_slot_t *slot = &s_slots[slot_index];
    slot->callback_started_at_us = callback_started_at_us;
    slot->rx_ctrl = info->rx_ctrl;
    memcpy(slot->mac, info->mac, sizeof(slot->mac));
    slot->first_word_invalid = info->first_word_invalid;
    slot->iq_len = (uint16_t)info->len;
    memcpy(slot->iq_data, info->buf, slot->iq_len);
    if (slot->first_word_invalid) {
        /* ESP-IDF marks the first four bytes as unusable. Preserve that fact
         * in the wire metadata, while keeping the local edge path free of
         * artificial zero-amplitude samples by forwarding the first valid
         * complex sample into the two invalid positions. */
        slot->iq_data[0] = slot->iq_data[4];
        slot->iq_data[1] = slot->iq_data[5];
        slot->iq_data[2] = slot->iq_data[4];
        slot->iq_data[3] = slot->iq_data[5];
    }
    slot->enqueued_at_us = esp_timer_get_time();
    if (slot->callback_started_at_us <= 0) {
        slot->callback_started_at_us = slot->enqueued_at_us;
    }

    bool force_publish_failure = false;
    portENTER_CRITICAL(&s_lock);
#ifdef CSI_DIAGNOSTICS_TEST
    force_publish_failure = s_test_fail_next_publish;
    s_test_fail_next_publish = false;
#endif
    slot->state = CSI_SLOT_QUEUED;
    s_captures_enqueued++;
    s_captures_waiting++;
    QueueHandle_t queue = s_queue;
    portEXIT_CRITICAL(&s_lock);

    if (force_publish_failure
        || queue == NULL
        || xQueueSend(queue, &slot_index, 0) != pdTRUE) {
        portENTER_CRITICAL(&s_lock);
        if (s_captures_enqueued > 0) s_captures_enqueued--;
        if (s_captures_waiting > 0) s_captures_waiting--;
        s_queue_publish_failures++;
        release_slot_locked(slot_index);
        portEXIT_CRITICAL(&s_lock);
        return false;
    }

    uint32_t queue_depth = (uint32_t)uxQueueMessagesWaiting(queue);
    portENTER_CRITICAL(&s_lock);
    if (queue_depth > s_queue_high_watermark) {
        s_queue_high_watermark = queue_depth;
    }
    portEXIT_CRITICAL(&s_lock);
    return true;
}

void csi_transport_get_stats(csi_transport_stats_t *stats)
{
    if (stats == NULL) {
        return;
    }

    portENTER_CRITICAL(&s_lock);
    QueueHandle_t queue = s_queue;
    TaskHandle_t task = s_processing_task;
    *stats = (csi_transport_stats_t) {
        .captures_enqueued = s_captures_enqueued,
        .start_failures = s_start_failures,
        .not_ready_drops = s_not_ready_drops,
        .invalid_drops = s_invalid_drops,
        .pool_exhausted_drops = s_pool_exhausted_drops,
        .stale_queued_replacements = s_stale_queued_replacements,
        .queue_publish_failures = s_queue_publish_failures,
        .ownership_errors = s_ownership_errors,
        .captures_waiting = s_captures_waiting,
        .captures_in_flight = s_captures_in_flight,
        .captures_aborted = s_captures_aborted,
        .pool_in_use = s_pool_in_use,
        .pool_high_watermark = s_pool_high_watermark,
        .queue_high_watermark = s_queue_high_watermark,
        .serialization_failures = s_serialization_failures,
        .serialized_frames = s_serialized_frames,
        .csi_send_calls = s_csi_send_calls,
        .retries = s_retries,
        .transport_errors = s_transport_errors,
        .frames_os_accepted = s_frames_os_accepted,
        .frames_abandoned = s_frames_abandoned,
        .frames_expired = s_frames_expired,
        .edge_enqueue_accepted = s_edge_enqueue_accepted,
        .edge_enqueue_drops = s_edge_enqueue_drops,
        .edge_enqueue_skipped = s_edge_enqueue_skipped,
        .sync_packets_attempted = s_sync_packets_attempted,
        .sync_packets_sent = s_sync_packets_sent,
        .queue_age_total_us = s_queue_age_total_us,
        .queue_age_max_us = s_queue_age_max_us,
        .queue_age_last_us = s_queue_age_last_us,
        .queue_age_samples = s_queue_age_samples,
        .callback_to_dequeue_total_us = s_callback_to_dequeue_total_us,
        .callback_to_dequeue_max_us = s_callback_to_dequeue_max_us,
        .callback_to_dequeue_last_us = s_callback_to_dequeue_last_us,
        .processing_stack_configured_bytes = CSI_TRANSPORT_TASK_STACK_BYTES,
    };
    portEXIT_CRITICAL(&s_lock);

    stats->queue_depth = queue == NULL
        ? 0u
        : (uint32_t)uxQueueMessagesWaiting(queue);
    if (task != NULL) {
        stats->processing_stack_free_min_bytes =
            (uint32_t)uxTaskGetStackHighWaterMark(task)
                * (uint32_t)sizeof(StackType_t);
        stats->processing_stack_runtime_measured = true;
    }
}

#ifdef CSI_DIAGNOSTICS_TEST
void csi_transport_test_reset(void)
{
    portENTER_CRITICAL(&s_lock);
    s_queue = NULL;
    s_processing_task = NULL;
    s_accepting = false;
    s_stop_requested = false;
    memset(s_slots, 0, sizeof(s_slots));
    s_captures_enqueued = 0;
    s_start_failures = 0;
    s_not_ready_drops = 0;
    s_invalid_drops = 0;
    s_pool_exhausted_drops = 0;
    s_stale_queued_replacements = 0;
    s_queue_publish_failures = 0;
    s_ownership_errors = 0;
    s_captures_waiting = 0;
    s_captures_in_flight = 0;
    s_captures_aborted = 0;
    s_pool_in_use = 0;
    s_pool_high_watermark = 0;
    s_queue_high_watermark = 0;
    s_serialization_failures = 0;
    s_serialized_frames = 0;
    s_csi_send_calls = 0;
    s_retries = 0;
    s_transport_errors = 0;
    s_frames_os_accepted = 0;
    s_frames_abandoned = 0;
    s_frames_expired = 0;
    s_edge_enqueue_accepted = 0;
    s_edge_enqueue_drops = 0;
    s_edge_enqueue_skipped = 0;
    s_sync_packets_attempted = 0;
    s_sync_packets_sent = 0;
    s_queue_age_total_us = 0;
    s_queue_age_max_us = 0;
    s_queue_age_last_us = 0;
    s_queue_age_samples = 0;
    s_callback_to_dequeue_total_us = 0;
    s_callback_to_dequeue_max_us = 0;
    s_callback_to_dequeue_last_us = 0;
    s_test_fail_next_publish = false;
    s_test_fail_next_serialization = false;
    portEXIT_CRITICAL(&s_lock);
}

bool csi_transport_test_process_one(void)
{
    uint8_t slot_index;
    if (s_queue == NULL || xQueueReceive(s_queue, &slot_index, 0) != pdTRUE) {
        return false;
    }
    process_slot(slot_index);
    return true;
}

void csi_transport_test_fail_next_publish(void)
{
    portENTER_CRITICAL(&s_lock);
    s_test_fail_next_publish = true;
    portEXIT_CRITICAL(&s_lock);
}

void csi_transport_test_fail_next_serialization(void)
{
    portENTER_CRITICAL(&s_lock);
    s_test_fail_next_serialization = true;
    portEXIT_CRITICAL(&s_lock);
}
#endif
