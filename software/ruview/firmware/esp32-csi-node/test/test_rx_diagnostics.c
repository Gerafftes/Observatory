/** Deterministic host tests for RX selection, capture ownership, and diagnostics. */

#include "esp_stubs.h"
#include "csi_collector.h"
#include "csi_transport.h"
#include "nvs_config.h"
#include "stream_sender.h"

#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>

nvs_config_t g_nvs_config;

static int s_failures;

#define CHECK(condition, message) do {                                      \
        if (!(condition)) {                                                 \
            fprintf(stderr, "FAIL: %s (line %d)\n", message, __LINE__);     \
            s_failures++;                                                   \
        }                                                                   \
    } while (0)

static const uint8_t FILTER_MAC[6] = {0x00, 0x11, 0x22, 0x33, 0x44, 0x55};
static const uint8_t ALTERNATE_FILTER_MAC[6] = {0x02, 0x11, 0x22, 0x33, 0x44, 0x66};

#define TEST_IQ_LEN 4
#define TEST_FRAME_SIZE \
    (CSI_HEADER_SIZE + TEST_IQ_LEN + CSI_SOURCE_BINDING_SIZE \
     + CSI_IDENTITY_TRAILER_SIZE)

static wifi_csi_info_t test_info(const uint8_t source_mac[6], int8_t *iq, int16_t len)
{
    wifi_csi_info_t info;
    memset(&info, 0, sizeof(info));
    memcpy(info.mac, source_mac, sizeof(info.mac));
    info.buf = iq;
    info.len = len;
    info.rx_ctrl.channel = 6;
    info.rx_ctrl.rssi = -48;
    info.rx_ctrl.noise_floor = -92;
    info.rx_ctrl.sig_mode = 1;
    return info;
}

static void configure_collector(const uint8_t filter_mac[6])
{
    memset(&g_nvs_config, 0, sizeof(g_nvs_config));
    g_nvs_config.filter_mac_set = 1;
    memcpy(g_nvs_config.filter_mac, filter_mac, sizeof(g_nvs_config.filter_mac));
    csi_collector_set_node_id(1);
    csi_collector_set_boot_id(UINT64_C(0x0102030405060708));
}

static void prepare_transport_test(int send_result)
{
    csi_collector_test_reset();
    csi_transport_test_reset();
    stream_sender_test_reset();
    esp_test_reset_edge_enqueue();
    configure_collector(FILTER_MAC);
    esp_test_set_time_step_us(0);
    stream_sender_test_set_sendto_result(send_result, 0);
    CHECK(csi_transport_start() == ESP_OK, "transport starts");
}

static void prepare_callback_test(void)
{
    prepare_transport_test(TEST_FRAME_SIZE);
}

static void drain_transport(void)
{
    while (csi_transport_test_process_one()) {
    }
}

static bool invoke_at(wifi_csi_info_t *info, int64_t now_us)
{
    uint32_t sequence_before = csi_collector_get_sequence();
    esp_test_set_time_us(now_us);
    csi_collector_test_invoke_callback(info);
    drain_transport();
    return csi_collector_get_sequence() == sequence_before + 1;
}

static void check_callback_balances(const csi_collector_stats_t *stats)
{
    CHECK(stats->callbacks_received
              == stats->invalid_inputs + stats->foreign_tx
                  + stats->rate_limited + stats->admitted,
          "callback outcomes are disjoint and balanced");
    CHECK(stats->admitted == stats->capture_handoff_attempts,
          "every admitted frame reaches one capture handoff");
    CHECK(stats->capture_handoff_attempts
              == stats->capture_handoff_accepted + stats->capture_handoff_drops,
          "capture handoff balance closes");
    CHECK(stats->callback_duration_samples == stats->callbacks_received,
          "callback timing samples cover every outcome");
}

static void check_transport_balance(const csi_transport_stats_t *stats)
{
    CHECK(stats->captures_enqueued
              == stats->captures_waiting + stats->captures_in_flight
                  + stats->captures_aborted + stats->serialization_failures
                  + stats->frames_os_accepted + stats->frames_abandoned,
          "capture balance includes pending, aborted, and terminal outcomes");
    CHECK(stats->pool_in_use
              == stats->captures_waiting + stats->captures_in_flight,
          "pool ownership matches waiting plus in-flight captures");
    CHECK(stats->ownership_errors == 0, "capture ownership state stays valid");
}

static void make_csi_packet(uint8_t *packet, size_t len)
{
    memset(packet, 0, len);
    uint32_t magic = CSI_MAGIC;
    memcpy(packet, &magic, sizeof(magic));
}

static void test_foreign_frame_before_valid_does_not_consume_slot(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    uint8_t other_mac[6];
    memcpy(other_mac, FILTER_MAC, sizeof(other_mac));
    other_mac[5] ^= 0xff;

    prepare_callback_test();
    wifi_csi_info_t foreign = test_info(other_mac, iq, sizeof(iq));
    wifi_csi_info_t matching = test_info(FILTER_MAC, iq, sizeof(iq));
    CHECK(!invoke_at(&foreign, 1000000), "foreign frame is rejected");
    CHECK(invoke_at(&matching, 1000000),
          "matching frame at the same instant is admitted");

    csi_collector_stats_t stats;
    csi_collector_get_stats(&stats);
    CHECK(stats.callbacks_received == 2, "both callback entries are counted");
    CHECK(stats.foreign_tx == 1, "foreign TX has one unambiguous rejection");
    CHECK(stats.rate_limited == 0, "foreign TX consumes no rate-limit slot");
    CHECK(stats.admitted == 1 && stats.capture_handoff_accepted == 1,
          "matching frame alone reaches the capture pool");
    check_callback_balances(&stats);
}

static void test_invalid_inputs_do_not_consume_slot(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t missing_buffer = test_info(FILTER_MAC, iq, sizeof(iq));
    missing_buffer.buf = NULL;
    wifi_csi_info_t empty = test_info(FILTER_MAC, iq, 0);
    wifi_csi_info_t oversized = test_info(FILTER_MAC, iq, INT16_MAX);
    wifi_csi_info_t matching = test_info(FILTER_MAC, iq, sizeof(iq));

    prepare_callback_test();
    CHECK(!invoke_at(NULL, 1100000), "null CSI info is rejected");
    CHECK(!invoke_at(&missing_buffer, 1100000), "missing I/Q buffer is rejected");
    CHECK(!invoke_at(&empty, 1100000), "empty I/Q buffer is rejected");
    CHECK(!invoke_at(&oversized, 1100000), "oversized I/Q buffer is rejected");
    CHECK(invoke_at(&matching, 1100000),
          "valid frame at the same instant keeps the full slot");

    csi_collector_stats_t stats;
    csi_collector_get_stats(&stats);
    CHECK(stats.invalid_inputs == 4, "all invalid forms share one reject counter");
    CHECK(stats.rate_limited == 0, "invalid inputs consume no rate-limit slot");
    CHECK(stats.admitted == 1 && stats.capture_handoff_accepted == 1,
          "only the valid frame is handed off");
    check_callback_balances(&stats);
}

static void test_callback_only_copies_and_publishes(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t matching = test_info(FILTER_MAC, iq, sizeof(iq));

    prepare_callback_test();
    esp_test_set_time_us(1500000);
    csi_collector_test_invoke_callback(&matching);

    stream_sender_stats_t sender;
    csi_transport_stats_t transport;
    stream_sender_get_stats(&sender);
    csi_transport_get_stats(&transport);
    CHECK(csi_collector_get_sequence() == 0,
          "callback assigns no wire sequence before worker processing");
    CHECK(sender.send_calls == 0, "callback performs no UDP send");
    CHECK(esp_test_edge_enqueue_count() == 0,
          "callback performs no local signal-processing handoff");
    CHECK(transport.captures_waiting == 1 && transport.pool_in_use == 1,
          "callback leaves exactly one owned capture for the worker");

    CHECK(csi_transport_test_process_one(), "worker consumes published slot");
    stream_sender_get_stats(&sender);
    CHECK(csi_collector_get_sequence() == 1, "worker assigns the wire sequence");
    CHECK(sender.csi_send_calls == 1, "worker performs the raw UDP send");
#if defined(CONFIG_CSI_RAW_PROFILE) && CONFIG_CSI_RAW_PROFILE
    csi_transport_get_stats(&transport);
    CHECK(esp_test_edge_enqueue_count() == 0,
          "raw profile never enqueues local signal processing");
    CHECK(transport.edge_enqueue_skipped == 1,
          "raw profile reports the deliberately skipped edge path");
#else
    CHECK(esp_test_edge_enqueue_count() == 1,
          "worker performs the bounded local-processing handoff");
#endif
}

static void test_alternating_19_8_and_20_2_ms(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t matching = test_info(FILTER_MAC, iq, sizeof(iq));
    int64_t now_us = 2000000;
    const uint32_t frame_count = 101;

    prepare_callback_test();
    for (uint32_t index = 0; index < frame_count; ++index) {
        if (index > 0) {
            now_us += (index & 1u) ? 19800 : 20200;
        }
        CHECK(invoke_at(&matching, now_us),
              "alternating 19.8/20.2 ms frame is admitted");
    }

    csi_collector_stats_t stats;
    csi_collector_get_stats(&stats);
    CHECK(stats.admitted == frame_count && stats.rate_limited == 0,
          "alternating jitter does not halve the selected stream");
    CHECK(csi_collector_get_sequence() == frame_count,
          "worker serializes each selected jitter frame once");
    check_callback_balances(&stats);
}

static void test_long_jitter_sequence(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t matching = test_info(FILTER_MAC, iq, sizeof(iq));
    const int64_t start_us = 5000000;
    const uint32_t frame_count = 200;

    prepare_callback_test();
    for (uint32_t index = 0; index < frame_count; ++index) {
        int32_t jitter_us = index == 0
            ? 0
            : (int32_t)((index * 137u) % 901u) - 450;
        int64_t arrival_us = start_us + (int64_t)index * 20000 + jitter_us;
        CHECK(invoke_at(&matching, arrival_us),
              "bounded long-run jitter frame is admitted");
    }

    csi_collector_stats_t stats;
    csi_collector_get_stats(&stats);
    CHECK(stats.admitted == frame_count && stats.rate_limited == 0,
          "long bounded-jitter series stays phase locked");
    check_callback_balances(&stats);
}

static void test_bursts_and_sustained_overload_are_bounded(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t matching = test_info(FILTER_MAC, iq, sizeof(iq));
    const int64_t start_us = 10000000;

    prepare_callback_test();
    for (uint32_t index = 0; index < 10; ++index) {
        bool admitted = invoke_at(&matching, start_us);
        CHECK(admitted == (index == 0),
              "same-instant burst admits only its first frame");
    }
    for (uint32_t millisecond = 1; millisecond <= 10000; ++millisecond) {
        (void)invoke_at(&matching, start_us + (int64_t)millisecond * 1000);
    }

    csi_collector_stats_t stats;
    csi_collector_get_stats(&stats);
    CHECK(stats.callbacks_received == 10010, "entire overload stream is classified");
    CHECK(stats.admitted == 501, "ten-second inclusive window admits 501 schedule slots");
    CHECK(stats.rate_limited == 9509, "all excess overload frames are rejected");
    CHECK(stats.capture_handoff_accepted == stats.admitted,
          "drained capture pool accepts every selected overload frame");
    check_callback_balances(&stats);
}

static void test_long_pause_does_not_create_credit_burst(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t matching = test_info(FILTER_MAC, iq, sizeof(iq));
    const int64_t start_us = 12000000;
    const int64_t after_pause_us = start_us + 10000000;

    prepare_callback_test();
    CHECK(invoke_at(&matching, start_us), "first frame is admitted");
    CHECK(invoke_at(&matching, after_pause_us), "first frame after long pause is admitted");
    CHECK(!invoke_at(&matching, after_pause_us + 1),
          "long pause does not accumulate a second immediate slot");
    CHECK(invoke_at(&matching, after_pause_us + 19500),
          "next frame may use the explicit 500 us early tolerance");
    CHECK(!invoke_at(&matching, after_pause_us + 19501),
          "early-tolerance admission does not open a burst");

    csi_collector_stats_t stats;
    csi_collector_get_stats(&stats);
    CHECK(stats.admitted == 3 && stats.rate_limited == 2,
          "long-pause behavior is bounded and explicit");
    check_callback_balances(&stats);
}

static void test_filter_configuration_switch(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t original = test_info(FILTER_MAC, iq, sizeof(iq));
    wifi_csi_info_t alternate = test_info(ALTERNATE_FILTER_MAC, iq, sizeof(iq));
    const int64_t start_us = 23000000;

    prepare_callback_test();
    CHECK(!invoke_at(&alternate, start_us), "old filter rejects alternate source");
    configure_collector(ALTERNATE_FILTER_MAC);
    CHECK(invoke_at(&alternate, start_us), "new filter admits alternate source immediately");
    configure_collector(FILTER_MAC);
    CHECK(!invoke_at(&original, start_us),
          "configuration change does not erase overload history");
    CHECK(!invoke_at(&alternate, start_us), "restored filter rejects alternate source");
    CHECK(invoke_at(&original, start_us + 20000),
          "restored filter admits its bound source at the next slot");

    csi_collector_stats_t stats;
    csi_collector_get_stats(&stats);
    CHECK(stats.foreign_tx == 2, "each foreign source is counted only as foreign");
    CHECK(stats.rate_limited == 1, "matching overload is counted only as limited");
    CHECK(stats.admitted == 2, "each active filter selects its matching source");
    check_callback_balances(&stats);
}

static void test_maximum_and_invalid_capture_lengths(void)
{
    int8_t iq_max[CSI_MAX_IQ_SIZE];
    int8_t one_byte = 1;
    memset(iq_max, 0x5a, sizeof(iq_max));
    wifi_csi_info_t maximum = test_info(FILTER_MAC, iq_max, sizeof(iq_max));
    wifi_csi_info_t missing = test_info(FILTER_MAC, NULL, TEST_IQ_LEN);
    wifi_csi_info_t empty = test_info(FILTER_MAC, &one_byte, 0);
    wifi_csi_info_t oversized = test_info(FILTER_MAC, &one_byte, CSI_MAX_IQ_SIZE + 1);

    prepare_transport_test(CSI_MAX_FRAME_SIZE);
    esp_test_set_time_us(3000000);
    CHECK(!csi_transport_enqueue_capture(NULL, 3000000), "null capture is rejected");
    CHECK(!csi_transport_enqueue_capture(&missing, 3000000), "null I/Q is rejected");
    CHECK(!csi_transport_enqueue_capture(&empty, 3000000), "zero I/Q is rejected");
    CHECK(!csi_transport_enqueue_capture(&oversized, 3000000), "oversized I/Q is rejected");
    CHECK(csi_transport_enqueue_capture(&maximum, 3000000), "maximum I/Q fits fixed slot");
    CHECK(csi_transport_test_process_one(), "maximum capture is processed");

    csi_transport_stats_t stats;
    csi_transport_get_stats(&stats);
    CHECK(stats.invalid_drops == 4, "all malformed captures use the invalid counter");
    CHECK(stats.frames_os_accepted == 1, "maximum capture reaches UDP sender");
    CHECK(stream_sender_test_copy_last_payload(NULL, 0) == CSI_MAX_FRAME_SIZE,
          "maximum serialized frame has exact bounded size");
    check_transport_balance(&stats);
}

static void test_capture_owns_driver_data_and_timestamp(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t info = test_info(FILTER_MAC, iq, sizeof(iq));
    const int64_t callback_us = 4000123;

    prepare_transport_test(TEST_FRAME_SIZE);
    esp_test_set_time_us(callback_us);
    CHECK(csi_transport_enqueue_capture(&info, callback_us), "capture is copied into pool");
    memset(iq, 0x7f, sizeof(iq));
    memset(info.mac, 0xff, sizeof(info.mac));
    info.rx_ctrl.rssi = -1;
    info.rx_ctrl.channel = 11;
    CHECK(csi_transport_test_process_one(), "owned capture is processed after source mutation");

    uint8_t payload[TEST_FRAME_SIZE];
    size_t payload_len = stream_sender_test_copy_last_payload(payload, sizeof(payload));
    size_t binding_offset = CSI_HEADER_SIZE + TEST_IQ_LEN;
    CHECK(payload_len == TEST_FRAME_SIZE, "owned capture serializes one exact frame");
    CHECK(memcmp(&payload[CSI_HEADER_SIZE], (uint8_t[]){1, 2, 3, 4}, TEST_IQ_LEN) == 0,
          "serialized I/Q comes from the pool, not the mutated driver buffer");
    CHECK((int8_t)payload[16] == -48, "copied RX metadata survives source mutation");
    CHECK((payload[binding_offset + 5] & CSI_SOURCE_BINDING_FLAG_SOURCE_MATCHED) != 0,
          "copied source MAC preserves the TX binding decision");
#if defined(CONFIG_CSI_RAW_PROFILE) && CONFIG_CSI_RAW_PROFILE
    csi_transport_stats_t transport_stats;
    csi_transport_get_stats(&transport_stats);
    CHECK(esp_test_edge_enqueue_count() == 0,
          "raw profile skips local signal-processing handoff");
    CHECK(transport_stats.edge_enqueue_skipped == 1,
          "raw profile records the skipped local handoff separately");
#else
    CHECK(esp_test_edge_timestamp_us() == (uint32_t)callback_us,
          "local processing keeps callback-entry time, not dequeue time");
#endif
}

static void test_pool_exhaustion_and_slow_consumer(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t info = test_info(FILTER_MAC, iq, sizeof(iq));

    prepare_transport_test(TEST_FRAME_SIZE);
    esp_test_set_time_us(5000000);
    for (uint32_t index = 0; index < CSI_TRANSPORT_CAPTURE_SLOTS; ++index) {
        CHECK(csi_transport_enqueue_capture(&info, 4999000),
              "fixed pool accepts each configured slot");
    }
    CHECK(csi_transport_enqueue_capture(&info, 4999000),
          "full pool replaces its oldest queued capture without waiting");

    esp_test_advance_time_us(1000);
    CHECK(csi_transport_test_process_one(), "slow consumer eventually processes one slot");
    CHECK(csi_transport_enqueue_capture(&info, 5001000),
          "released slot can be reused immediately");

    csi_transport_stats_t pending;
    csi_transport_get_stats(&pending);
    CHECK(pending.captures_enqueued == 10, "ten captures have owned slots over time");
    CHECK(pending.stale_queued_replacements == 1,
          "fresh capture records one stale queued replacement");
    CHECK(pending.pool_exhausted_drops == 0,
          "replaceable queue pressure is not reported as pool exhaustion");
    CHECK(pending.captures_waiting == 8 && pending.pool_in_use == 8,
          "slow consumer remains bounded to eight pending captures");
    CHECK(pending.queue_high_watermark == CSI_TRANSPORT_CAPTURE_SLOTS,
          "queue high-water mark reaches the fixed capacity");
    CHECK(pending.queue_age_last_us == 1000,
          "queue residence is measured at dequeue outside callback");
    CHECK(pending.callback_to_dequeue_last_us == 2000,
          "callback-to-dequeue age remains a separate measurement");
    check_transport_balance(&pending);

    drain_transport();
    csi_transport_get_stats(&pending);
    CHECK(pending.pool_in_use == 0 && pending.frames_os_accepted == 9,
          "draining returns every slot and sends each accepted capture once");
    check_transport_balance(&pending);
}

static void test_publish_and_serialization_failures_release_slots(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t info = test_info(FILTER_MAC, iq, sizeof(iq));

    prepare_transport_test(TEST_FRAME_SIZE);
    csi_transport_test_fail_next_publish();
    CHECK(!csi_transport_enqueue_capture(&info, 6000000),
          "queue publication failure is returned to producer");
    csi_transport_stats_t stats;
    csi_transport_get_stats(&stats);
    CHECK(stats.queue_publish_failures == 1 && stats.pool_in_use == 0,
          "publication failure releases its claimed slot");

    CHECK(csi_transport_enqueue_capture(&info, 6001000),
          "slot is reusable after publication failure");
    csi_transport_test_fail_next_serialization();
    CHECK(csi_transport_test_process_one(), "forced serialization failure is processed");
    csi_transport_get_stats(&stats);
    CHECK(stats.serialization_failures == 1 && stats.pool_in_use == 0,
          "serialization failure is terminal and releases ownership");
    CHECK(stats.frames_os_accepted == 0 && stats.frames_abandoned == 0,
          "serialization failure is not double-counted as a send outcome");
    check_transport_balance(&stats);
}

static void test_local_backpressure_does_not_block_raw_send(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t info = test_info(FILTER_MAC, iq, sizeof(iq));

    prepare_transport_test(TEST_FRAME_SIZE);
    esp_test_set_edge_enqueue_result(false);
    CHECK(csi_transport_enqueue_capture(&info, 7000000), "raw capture is accepted");
    CHECK(csi_transport_test_process_one(), "raw capture is processed");

    csi_transport_stats_t stats;
    csi_transport_get_stats(&stats);
    CHECK(stats.frames_os_accepted == 1, "raw UDP send succeeds before local handoff");
#if defined(CONFIG_CSI_RAW_PROFILE) && CONFIG_CSI_RAW_PROFILE
    CHECK(esp_test_edge_enqueue_count() == 0,
          "raw profile keeps edge processing disabled under backpressure");
    CHECK(stats.edge_enqueue_skipped == 1 && stats.edge_enqueue_drops == 0,
          "disabled edge work is not reported as a queue drop");
#else
    CHECK(stats.edge_enqueue_drops == 1 && stats.edge_enqueue_accepted == 0,
          "full local pipeline is counted without retaining the raw slot");
#endif
    CHECK(stats.pool_in_use == 0, "local backpressure cannot pin capture memory");
    check_transport_balance(&stats);
}

static void test_abort_and_restart_processing(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t info = test_info(FILTER_MAC, iq, sizeof(iq));

    prepare_transport_test(TEST_FRAME_SIZE);
    for (int index = 0; index < 3; ++index) {
        CHECK(csi_transport_enqueue_capture(&info, 8000000 + index),
              "capture is pending before stop");
    }
    csi_transport_stop();

    csi_transport_stats_t stats;
    csi_transport_get_stats(&stats);
    CHECK(stats.captures_aborted == 3, "stop accounts for every pending capture");
    CHECK(stats.pool_in_use == 0 && stats.captures_waiting == 0,
          "stop releases the complete fixed pool");
    CHECK(!csi_transport_enqueue_capture(&info, 8001000),
          "stopped transport rejects new captures without waiting");
    CHECK(stats.captures_enqueued == 3, "stop preserves the diagnostic history");

    CHECK(csi_transport_start() == ESP_OK, "transport restarts after stop");
    CHECK(csi_transport_enqueue_capture(&info, 8002000),
          "restarted transport accepts a fresh capture");
    CHECK(csi_transport_test_process_one(), "fresh capture processes after restart");
    csi_transport_get_stats(&stats);
    CHECK(stats.captures_enqueued == 4 && stats.captures_aborted == 3,
          "restart adds a new capture without rewriting prior outcomes");
    CHECK(stats.frames_os_accepted == 1, "post-restart capture reaches UDP sender");
    check_transport_balance(&stats);
}

typedef struct {
    pthread_mutex_t lock;
    pthread_cond_t condition;
    uint32_t ready;
    bool start;
} concurrent_gate_t;

typedef struct {
    concurrent_gate_t *gate;
    wifi_csi_info_t info;
    int8_t iq[TEST_IQ_LEN];
    bool accepted;
} concurrent_capture_t;

static void *enqueue_concurrently(void *argument)
{
    concurrent_capture_t *capture = argument;
    (void)pthread_mutex_lock(&capture->gate->lock);
    capture->gate->ready++;
    (void)pthread_cond_broadcast(&capture->gate->condition);
    while (!capture->gate->start) {
        (void)pthread_cond_wait(&capture->gate->condition, &capture->gate->lock);
    }
    (void)pthread_mutex_unlock(&capture->gate->lock);
    capture->accepted = csi_transport_enqueue_capture(&capture->info, 9000000);
    return NULL;
}

static void test_concurrent_handoffs_preserve_ownership(void)
{
    enum { PRODUCER_COUNT = 16 };
    pthread_t threads[PRODUCER_COUNT];
    concurrent_capture_t captures[PRODUCER_COUNT];
    concurrent_gate_t gate = {
        .lock = PTHREAD_MUTEX_INITIALIZER,
        .condition = PTHREAD_COND_INITIALIZER,
    };

    prepare_transport_test(TEST_FRAME_SIZE);
    for (int index = 0; index < PRODUCER_COUNT; ++index) {
        captures[index].gate = &gate;
        memset(captures[index].iq, index, sizeof(captures[index].iq));
        captures[index].info = test_info(FILTER_MAC, captures[index].iq,
                                         sizeof(captures[index].iq));
        captures[index].accepted = false;
        CHECK(pthread_create(&threads[index], NULL, enqueue_concurrently,
                             &captures[index]) == 0,
              "concurrent producer thread starts");
    }

    (void)pthread_mutex_lock(&gate.lock);
    while (gate.ready < PRODUCER_COUNT) {
        (void)pthread_cond_wait(&gate.condition, &gate.lock);
    }
    gate.start = true;
    (void)pthread_cond_broadcast(&gate.condition);
    (void)pthread_mutex_unlock(&gate.lock);

    uint32_t accepted = 0;
    for (int index = 0; index < PRODUCER_COUNT; ++index) {
        CHECK(pthread_join(threads[index], NULL) == 0,
              "concurrent producer thread joins");
        if (captures[index].accepted) {
            accepted++;
        }
    }
    CHECK(accepted >= CSI_TRANSPORT_CAPTURE_SLOTS && accepted <= PRODUCER_COUNT,
          "concurrent producers keep at least one bounded pool generation");

    csi_transport_stats_t stats;
    csi_transport_get_stats(&stats);
    CHECK(stats.pool_exhausted_drops == PRODUCER_COUNT - accepted,
          "non-replaceable concurrent handoffs are counted once");
    CHECK(stats.stale_queued_replacements == accepted - CSI_TRANSPORT_CAPTURE_SLOTS,
          "accepted excess handoffs replace an older queued capture");
    CHECK(stats.pool_in_use == CSI_TRANSPORT_CAPTURE_SLOTS,
          "each concurrently accepted capture owns one distinct slot");
    check_transport_balance(&stats);

    drain_transport();
    csi_transport_get_stats(&stats);
    CHECK(stats.frames_os_accepted == CSI_TRANSPORT_CAPTURE_SLOTS,
          "only the bounded freshest generation is sent");
    check_transport_balance(&stats);
}

static void test_expiry_backoff_and_send_failure_paths(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t info = test_info(FILTER_MAC, iq, sizeof(iq));
    uint8_t packet[32];
    make_csi_packet(packet, sizeof(packet));

    prepare_transport_test(TEST_FRAME_SIZE);
    esp_test_set_time_us(10000000);
    CHECK(csi_transport_enqueue_capture(&info, 10000000), "stale-test capture enqueues");
    esp_test_advance_time_us(101000);
    stream_sender_test_set_sendto_result(-1, ENOMEM);
    CHECK(csi_transport_test_process_one(), "stale capture runs through sender");

    csi_transport_stats_t transport;
    csi_transport_get_stats(&transport);
    CHECK(transport.csi_send_calls == 0, "stale capture is rejected before sendto");
    CHECK(transport.frames_expired == 1, "expired capture is counted");
    CHECK(transport.frames_abandoned == 1, "expired capture is terminally abandoned");
    check_transport_balance(&transport);

    uint8_t stale_packet[4];
    make_csi_packet(stale_packet, sizeof(stale_packet));
    stream_sender_test_reset();
    esp_test_set_time_us(11000000);
    bool deadline_expired = false;
    CHECK(stream_sender_send_until(
              stale_packet,
              sizeof(stale_packet),
              10999999,
              &deadline_expired
          ) == -1,
          "sender refuses an expired CSI datagram");
    CHECK(deadline_expired, "sender identifies expiry separately from socket failure");
    stream_sender_stats_t expired_sender;
    stream_sender_get_stats(&expired_sender);
    CHECK(expired_sender.csi_sendto_attempts == 0,
          "expired CSI datagram never reaches a sendto attempt");

    stream_sender_test_set_sendto_result(-1, ENOMEM);
    CHECK(stream_sender_send(packet, sizeof(packet)) == -1,
          "fresh direct send starts ENOMEM backoff");
    CHECK(stream_sender_send(packet, sizeof(packet)) == -1,
          "active ENOMEM backoff suppresses a call");
    stream_sender_stats_t sender;
    stream_sender_get_stats(&sender);
    CHECK(sender.csi_sendto_attempts == 1, "only real sendto call counts as attempt");
    CHECK(sender.csi_sendto_failures == 1, "real sendto failure is counted");
    CHECK(sender.csi_backoff_suppressed == 1, "backoff suppression is distinct");
    CHECK(sender.enomem_backoffs == 1, "ENOMEM backoff episode is counted");

    esp_test_advance_time_us(101000);
    stream_sender_test_set_sendto_result((int)sizeof(packet), 0);
    CHECK(stream_sender_send(packet, sizeof(packet)) == (int)sizeof(packet),
          "send resumes after backoff");
    stream_sender_get_stats(&sender);
    CHECK(sender.csi_sendto_attempts == 2, "resumed send is a real attempt");
    CHECK(sender.csi_os_datagrams_accepted == 1,
          "successful sendto is recorded as OS-accepted datagram");
}

static void test_runtime_stack_measurement_fields(void)
{
    int8_t iq[TEST_IQ_LEN] = {1, 2, 3, 4};
    wifi_csi_info_t info = test_info(FILTER_MAC, iq, sizeof(iq));

    prepare_callback_test();
    CHECK(invoke_at(&info, 12000000), "callback runs for stack measurement test");
    csi_collector_stats_t collector;
    csi_transport_stats_t transport;
    csi_collector_get_stats(&collector);
    csi_transport_get_stats(&transport);
    CHECK(collector.callback_stack_runtime_measured
              && collector.callback_stack_free_min_bytes == 4096,
          "callback task exposes a runtime high-water measurement");
    CHECK(transport.processing_stack_runtime_measured
              && transport.processing_stack_free_min_bytes == 4096,
          "processing task exposes a runtime high-water measurement");
    CHECK(transport.processing_stack_configured_bytes == 8192,
          "configured processing stack size is reported separately");
}

int main(void)
{
    test_foreign_frame_before_valid_does_not_consume_slot();
    test_invalid_inputs_do_not_consume_slot();
    test_callback_only_copies_and_publishes();
    test_alternating_19_8_and_20_2_ms();
    test_long_jitter_sequence();
    test_bursts_and_sustained_overload_are_bounded();
    test_long_pause_does_not_create_credit_burst();
    test_filter_configuration_switch();
    test_maximum_and_invalid_capture_lengths();
    test_capture_owns_driver_data_and_timestamp();
    test_pool_exhaustion_and_slow_consumer();
    test_publish_and_serialization_failures_release_slots();
    test_local_backpressure_does_not_block_raw_send();
    test_abort_and_restart_processing();
    test_concurrent_handoffs_preserve_ownership();
    test_expiry_backoff_and_send_failure_paths();
    test_runtime_stack_measurement_fields();

    if (s_failures == 0) {
        puts("PASS: RX selection, capture ownership, and diagnostic balances");
        return 0;
    }
    fprintf(stderr, "%d RX diagnostic test(s) failed\n", s_failures);
    return 1;
}
