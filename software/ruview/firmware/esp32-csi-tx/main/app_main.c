#include <errno.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_event.h"
#include "esp_log.h"
#include "esp_mac.h"
#include "esp_netif.h"
#include "esp_system.h"
#include "esp_random.h"
#include "esp_task_wdt.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "esp_private/wifi.h"
#include "tx_led.h"
#include "tx_diag.h"
#include "lwip/inet.h"
#include "lwip/netbuf.h"
#include "lwip/sockets.h"
#include "mbedtls/sha256.h"
#include "nvs_flash.h"

#include "tx_config.h"
#include "tx_ota.h"
#include "tx_packet.h"
#include "tx_scheduler.h"
#include "tx_network.h"
#include "tx_boot.h"
#include "tx_safety.h"

#define SOUNDING_PORT 4210
#define SOCKET_REBUILD_THRESHOLD 8U
#define WIFI_RESTART_THRESHOLD 32U

static const char *TAG = "csi_tx";
static tx_config_t s_config;
static uint64_t s_boot_id;
static TaskHandle_t s_sender_task;
static tx_scheduler_t s_scheduler;
static tx_runtime_status_t s_status;
static portMUX_TYPE s_status_lock = portMUX_INITIALIZER_UNLOCKED;

typedef struct {
    bool active;
    uint32_t generation;
    uint32_t sequence;
    uint64_t begin_us;
    uintptr_t payload_address;
    uint8_t packet[TX_SOUNDING_PACKET_SIZE];
    uint32_t netbuf_calls;
    bool netbuf_returned_nonnull;
    uint32_t wifi_calls;
    int wifi_return;
} tx_send_attempt_t;

static tx_send_attempt_t s_send_attempt;

static tx_send_attempt_t send_attempt_snapshot(void)
{
    portENTER_CRITICAL(&s_status_lock);
    const tx_send_attempt_t attempt = s_send_attempt;
    portEXIT_CRITICAL(&s_status_lock);
    return attempt;
}

static bool send_attempt_current(const tx_send_attempt_t *attempt)
{
    /* Called only under s_status_lock; no remembered packet storage is read. */
    return s_send_attempt.active && s_send_attempt.generation == attempt->generation &&
           s_send_attempt.payload_address == attempt->payload_address;
}

static void send_attempt_begin(uint32_t sequence, const uint8_t packet[TX_SOUNDING_PACKET_SIZE])
{
    const int saved_errno = errno;
    const uint64_t begin_us = esp_timer_get_time();
    portENTER_CRITICAL(&s_status_lock);
    s_send_attempt = (tx_send_attempt_t){.active = true,
        .generation = s_send_attempt.generation + 1, .sequence = sequence, .begin_us = begin_us};
    memcpy(s_send_attempt.packet, packet, sizeof(s_send_attempt.packet));
    s_status.send_path.own_attempts++;
    portEXIT_CRITICAL(&s_status_lock);
    errno = saved_errno;
}

static void send_attempt_complete(int sent, int error_number)
{
    const int saved_errno = errno;
    const uint64_t end_us = esp_timer_get_time();
    portENTER_CRITICAL(&s_status_lock);
    if (s_send_attempt.active && sent == -1 && error_number == ENOMEM) {
        const tx_send_attempt_t *attempt = &s_send_attempt;
        tx_send_path_status_t *path = &s_status.send_path;
        tx_send_path_stage_t stage = TX_SEND_PATH_UNCLASSIFIED;
        if (attempt->netbuf_calls == 1 && !attempt->netbuf_returned_nonnull && attempt->wifi_calls == 0) {
            stage = TX_SEND_PATH_NETBUF_ALLOCATION;
            path->enomem_netbuf_allocation++;
        } else if (attempt->netbuf_calls == 1 && attempt->netbuf_returned_nonnull &&
                   attempt->wifi_calls == 1 && attempt->wifi_return == ESP_ERR_NO_MEM) {
            stage = TX_SEND_PATH_WIFI_HANDOFF;
            path->enomem_wifi_handoff++;
        } else {
            path->enomem_other_or_unclassified++;
        }
        path->last_enomem = (tx_send_path_error_t){.stage = stage, .sequence = attempt->sequence,
            .capture_begin_us = attempt->begin_us, .capture_end_us = end_us,
            .netbuf_calls = attempt->netbuf_calls, .netbuf_returned_nonnull = attempt->netbuf_returned_nonnull,
            .wifi_calls = attempt->wifi_calls, .wifi_return = attempt->wifi_return};
    }
    s_send_attempt.active = false;
    portEXIT_CRITICAL(&s_status_lock);
    errno = saved_errno;
}

void *__real_netbuf_alloc(struct netbuf *buf, u16_t size);
int __real_esp_wifi_internal_tx(wifi_interface_t interface, void *buffer, uint16_t length);

void *__wrap_netbuf_alloc(struct netbuf *buf, u16_t size)
{
    const int entry_errno = errno;
    tx_send_attempt_t attempt = {0};
    if (size == TX_SOUNDING_PACKET_SIZE && xTaskGetCurrentTaskHandle() == s_sender_task)
        attempt = send_attempt_snapshot();
    errno = entry_errno;
    void *result = __real_netbuf_alloc(buf, size);
    const int real_errno = errno;
    if (attempt.active) {
        portENTER_CRITICAL(&s_status_lock);
        if (send_attempt_current(&attempt)) {
            s_send_attempt.netbuf_calls++;
            s_send_attempt.netbuf_returned_nonnull = result != NULL;
            s_send_attempt.payload_address = (uintptr_t)result;
            s_status.send_path.netbuf_calls++;
        }
        portEXIT_CRITICAL(&s_status_lock);
    }
    errno = real_errno;
    return result;
}

static uint16_t frame_u16(const uint8_t *bytes)
{
    return (uint16_t)((uint16_t)bytes[0] << 8) | bytes[1];
}

static bool own_sounding_frame(const void *buffer, uint16_t length, const tx_send_attempt_t *attempt)
{
    const uint8_t *frame = buffer;
    static const uint8_t broadcast_mac[6] = {255, 255, 255, 255, 255, 255};
    static const uint8_t broadcast_ip[4] = {192, 168, 4, 255};
    if (!frame || length < 14 + 20 + 8 + TX_SOUNDING_PACKET_SIZE ||
        frame_u16(frame + 12) != 0x0800 || memcmp(frame, broadcast_mac, 6) != 0 ||
        memcmp(frame + 6, s_config.ap_mac, 6) != 0) return false;
    const uint8_t *ip = frame + 14;
    const unsigned ihl = (ip[0] & 15U) * 4U;
    if (ip[0] >> 4 != 4 || ihl < 20 || ihl > 60 ||
        (unsigned)length < 14 + ihl + 8 + TX_SOUNDING_PACKET_SIZE || ip[9] != 17 ||
        (frame_u16(ip + 6) & 0xbfffU) != 0 ||
        (unsigned)frame_u16(ip + 2) != ihl + 8 + TX_SOUNDING_PACKET_SIZE ||
        memcmp(ip + 16, broadcast_ip, 4) != 0) return false;
    const uint8_t *udp = ip + ihl;
    const uint8_t *payload = udp + 8;
    return frame_u16(udp + 2) == SOUNDING_PORT &&
           frame_u16(udp + 4) == 8 + TX_SOUNDING_PACKET_SIZE &&
           (uintptr_t)payload == attempt->payload_address &&
           memcmp(payload, attempt->packet, TX_SOUNDING_PACKET_SIZE) == 0;
}

int __wrap_esp_wifi_internal_tx(wifi_interface_t interface, void *buffer, uint16_t length)
{
    const int entry_errno = errno;
    tx_send_attempt_t attempt = {0};
    bool matched = false;
    if (interface == WIFI_IF_AP) {
        attempt = send_attempt_snapshot();
        if (attempt.active && attempt.payload_address != 0)
            matched = own_sounding_frame(buffer, length, &attempt);
    }
    errno = entry_errno;
    const int result = __real_esp_wifi_internal_tx(interface, buffer, length);
    const int real_errno = errno;
    if (matched) {
        portENTER_CRITICAL(&s_status_lock);
        if (send_attempt_current(&attempt)) {
            s_send_attempt.wifi_calls++;
            s_send_attempt.wifi_return = result;
            s_status.send_path.wifi_handoffs++;
        }
        portEXIT_CRITICAL(&s_status_lock);
    }
    errno = real_errno;
    return result;
}

static void status_provider(tx_runtime_status_t *status)
{
    portENTER_CRITICAL(&s_status_lock);
    *status = s_status;
    portEXIT_CRITICAL(&s_status_lock);
}

static void wifi_event(void *argument, esp_event_base_t base, int32_t id, void *data)
{
    (void)argument;
    (void)base;
    (void)data;
    portENTER_CRITICAL(&s_status_lock);
    if (id == WIFI_EVENT_AP_STACONNECTED) {
        s_status.station_generation++;
        s_status.station_count++;
    } else if (id == WIFI_EVENT_AP_STADISCONNECTED) {
        s_status.station_generation++;
        if (s_status.station_count > 0) s_status.station_count--;
    }
    portEXIT_CRITICAL(&s_status_lock);
}

static int create_broadcast_socket(tx_send_error_t *failure)
{
    int socket_fd = socket(AF_INET, SOCK_DGRAM, IPPROTO_IP);
    if (socket_fd < 0) {
        const int error_number = errno; /* Preserve before timer/logging calls. */
        *failure = (tx_send_error_t){.operation = TX_SEND_OP_SOCKET,
            .result = socket_fd, .error_number = error_number, .errno_valid = true};
        TX_ERROR("UDP_SOCKET_ERRNO", error_number);
        return -1;
    }
    int enabled = 1;
    int result = setsockopt(socket_fd, SOL_SOCKET, SO_BROADCAST, &enabled, sizeof(enabled));
    if (result != 0) {
        const int error_number = errno;
        *failure = (tx_send_error_t){.operation = TX_SEND_OP_BROADCAST,
            .result = result, .error_number = error_number, .errno_valid = true};
        TX_ERROR("UDP_BROADCAST_ERRNO", error_number);
        close(socket_fd);
        return -1;
    }
    return socket_fd;
}

static void sounding_timer(void *argument)
{
    (void)argument;
    xTaskNotifyGive(s_sender_task);
}

static void sender_task(void *argument)
{
    (void)argument;
    esp_err_t watchdog = esp_task_wdt_add(NULL);
    if (watchdog != ESP_OK) TX_ERROR("SENDER_WDT_ADD", watchdog);
    int socket_fd = -1;
    uint32_t consecutive_errors = 0;
    bool first_send = true;
    const struct sockaddr_in destination = {
        .sin_family = AF_INET,
        .sin_port = htons(SOUNDING_PORT),
        .sin_addr.s_addr = inet_addr("192.168.4.255"),
    };
    uint8_t packet[TX_SOUNDING_PACKET_SIZE];

    while (true) {
        uint32_t notifications = ulTaskNotifyTake(pdTRUE, portMAX_DELAY);
        uint64_t now_us = esp_timer_get_time();
        tx_schedule_result_t schedule =
            tx_scheduler_on_wakeup(&s_scheduler, notifications, now_us);
        tx_packet_build(packet, schedule.sequence, (uint32_t)(now_us / 1000ULL),
                        s_boot_id, s_config.channel, (uint8_t)s_config.rate_hz);

        tx_send_error_t failure = {0};
        if (socket_fd < 0) {
            socket_fd = create_broadcast_socket(&failure);
            portENTER_CRITICAL(&s_status_lock);
            s_status.socket_rebuilds++;
            portEXIT_CRITICAL(&s_status_lock);
        }
        int sent = -1;
        if (socket_fd >= 0) {
            send_attempt_begin(schedule.sequence, packet);
            sent = sendto(socket_fd, packet, sizeof(packet), 0,
                          (const struct sockaddr *)&destination, sizeof(destination));
            const int error_number = sent < 0 ? errno : 0;
            send_attempt_complete(sent, error_number);
            if (sent != (int)sizeof(packet)) {
                failure = (tx_send_error_t){.operation = TX_SEND_OP_SENDTO,
                    .result = sent, .error_number = error_number, .errno_valid = sent < 0};
            }
        }
        if (failure.operation != TX_SEND_OP_NONE) {
            failure.sequence = schedule.sequence;
            failure.occurred_us = esp_timer_get_time();
        }
        if (sent == (int)sizeof(packet) && first_send) {
            TX_CHECKPOINT("UDP_OK"); /* First complete datagram queued, not RF/RX proof. */
            first_send = false;
        } else if (sent != (int)sizeof(packet) && consecutive_errors == 0) {
            /* A short local handoff has no defined errno; never reuse stale errno. */
            TX_ERROR("UDP_SEND_RESULT", failure.errno_valid ? failure.error_number : failure.result);
        }
        portENTER_CRITICAL(&s_status_lock);
        s_status.sequence = schedule.sequence;
        s_status.missed_deadlines = s_scheduler.missed_deadlines;
        s_status.jitter_max_us = s_scheduler.jitter_max_us;
        s_status.jitter_sum_us = s_scheduler.jitter_sum_us;
        s_status.jitter_samples = s_scheduler.jitter_samples;
        if (sent == (int)sizeof(packet)) {
            s_status.sent_packets++;
            consecutive_errors = 0;
        } else {
            s_status.send_errors++;
            s_status.last_send_error = failure;
            if (failure.operation == TX_SEND_OP_SOCKET) s_status.socket_errors++;
            else if (failure.operation == TX_SEND_OP_BROADCAST) s_status.broadcast_errors++;
            else if (failure.errno_valid) s_status.sendto_errors++;
            else s_status.short_sends++;
            consecutive_errors++;
        }
        portEXIT_CRITICAL(&s_status_lock);

        if (consecutive_errors >= SOCKET_REBUILD_THRESHOLD && socket_fd >= 0) {
            close(socket_fd);
            socket_fd = -1;
        }
        if (consecutive_errors >= WIFI_RESTART_THRESHOLD) {
            TX_ERROR("WIFI_RECOVERY_SEND_ERRORS", consecutive_errors);
            ESP_LOGW(TAG, "Controlled SoftAP reinitialization after %lu send errors",
                     (unsigned long)consecutive_errors);
            esp_err_t restart = tx_network_restart(&s_config);
            if (restart != ESP_OK) {
                TX_ERROR("WIFI_RECOVERY_STOP", restart);
                ESP_LOGE(TAG, "SoftAP recovery failed; sender stopped");
                portENTER_CRITICAL(&s_status_lock);
                s_status.boot_check = -1;
                portEXIT_CRITICAL(&s_status_lock);
                close(socket_fd);
                (void)esp_task_wdt_delete(NULL);
                vTaskSuspend(NULL);
            }
            consecutive_errors = 0;
            portENTER_CRITICAL(&s_status_lock);
            s_status.wifi_restarts++;
            portEXIT_CRITICAL(&s_status_lock);
        }
        watchdog = esp_task_wdt_reset();
        if (watchdog != ESP_OK) TX_ERROR("SENDER_WDT_RESET", watchdog);
    }
}

static uint64_t create_boot_id(void)
{
    uint64_t boot_id = 0;
    while (boot_id == 0) {
        esp_fill_random(&boot_id, sizeof(boot_id));
    }
    return boot_id;
}

static esp_err_t sha256_hex(const uint8_t *data, size_t length, char output[65])
{
    uint8_t digest[32];
    static const char hex[] = "0123456789abcdef";
    if (mbedtls_sha256(data, length, digest, 0) != 0) return ESP_FAIL;
    for (size_t index = 0; index < sizeof(digest); ++index) {
        output[index * 2] = hex[digest[index] >> 4];
        output[index * 2 + 1] = hex[digest[index] & 0x0f];
    }
    output[64] = '\0';
    return ESP_OK;
}

void app_main(void)
{
    ESP_EARLY_LOGI("tx_boot", "TX_BOOT APP_MAIN_ENTER reset_reason=%d", esp_reset_reason());
    ESP_LOGI("tx_boot", "TX_BOOT IDENTITY source=%s git=%s", TX_SOURCE_SHA, TX_GIT_HASH);
    const uint64_t boot_started = esp_timer_get_time();
    esp_timer_handle_t timer = NULL;
    const char *stage = "PROFILE";
    TX_CHECKPOINT("BEGIN PROFILE");
    esp_err_t error = tx_config_init(&s_config);
    if (error != ESP_OK) {
        TX_ERROR(stage, error);
        ESP_LOGE(TAG, "NVS/configuration unavailable; no erase, AP or sender");
        return;
    }
    TX_CHECKPOINT("PROFILE_OK");
#define START(name, call) do { \
    stage = name; \
    TX_CHECKPOINT("BEGIN " name); \
    error = (call); \
    if (error != ESP_OK) goto failed; \
} while (0)
    TX_CHECKPOINT("BEGIN BOOT_ID");
    s_boot_id = create_boot_id();
    char source_mac_sha256[65];
    START("SOURCE_MAC_HASH", sha256_hex(s_config.ap_mac, sizeof(s_config.ap_mac), source_mac_sha256));
    START("NETWORK", tx_network_start(&s_config));
    START("WIFI_EVENTS", esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, wifi_event, NULL));
    START("STATUS", tx_ota_start(s_boot_id, &s_config, source_mac_sha256, status_provider));
    TX_CHECKPOINT("STATUS_OK");

    tx_scheduler_init(&s_scheduler, s_config.rate_hz, esp_timer_get_time());
    stage = "SENDER_TASK";
    TX_CHECKPOINT("BEGIN SENDER_TASK");
    if (xTaskCreate(sender_task, "tx_sender", 4096, NULL, 6, &s_sender_task) != pdPASS) {
        error = ESP_ERR_NO_MEM;
        goto failed;
    }
    const esp_timer_create_args_t timer_args = {
        .callback = sounding_timer, .dispatch_method = ESP_TIMER_TASK,
        .name = "tx_cadence", .skip_unhandled_events = false,
    };
    START("TIMER_CREATE", esp_timer_create(&timer_args, &timer));
    START("TIMER_START", esp_timer_start_periodic(timer, s_scheduler.interval_us));
    TX_CHECKPOINT("SOUNDING_STARTED");

    /* led_strip 3.0.3 waits indefinitely inside refresh. Its worker cannot
       hold up AP/status/sounding; the main task waits at most one second. */
    (void)tx_led_start(&s_config);
    tx_runtime_status_t status;
    status_provider(&status);
    tx_boot_check_t check;
    tx_boot_begin(&check, boot_started, esp_timer_get_time(), &status);
    TX_CHECKPOINT("BEGIN BOOT_CHECK");
    int result;
    do {
        const uint64_t due_us = check.last_sample_us + TX_BOOT_SAMPLE_US;
        const uint64_t wait_begin_us = esp_timer_get_time();
        const TickType_t tick_begin = xTaskGetTickCount();
        /* Ticks only block the task. esp_timer remains the deadline authority.
           An early wake/poll waits only the remaining interval, at least one
           tick; no busy wait and no synthetic catch-up samples. */
        TickType_t wait_ticks = 0;
        if (wait_begin_us >= check.last_poll_us && wait_begin_us < due_us) {
            wait_ticks = (TickType_t)(((due_us - wait_begin_us) * configTICK_RATE_HZ
                                      + UINT64_C(999999)) / UINT64_C(1000000));
            vTaskDelay(wait_ticks);
        }
        const uint64_t wait_end_us = esp_timer_get_time();
        const TickType_t tick_end = xTaskGetTickCount();
        const uint64_t probe_begin_us = esp_timer_get_time();
        const bool network_healthy = tx_network_check(&s_config) == ESP_OK;
        const uint32_t free_heap = esp_get_free_heap_size();
        status_provider(&status);
        const uint64_t sample_us = esp_timer_get_time();
        const bool clock_order = wait_begin_us >= check.last_poll_us &&
            wait_end_us >= wait_begin_us && probe_begin_us >= wait_end_us &&
            sample_us >= probe_begin_us;
        if (!clock_order) check.failed = true;
        const bool sampled = sample_us >= due_us;
        result = tx_boot_poll(&check, sample_us, &status, network_healthy, free_heap);
        /* One bounded timing record per poll. Getter and diagnostic log time
           is included in this or the next real completion-to-completion gap. */
        ESP_LOGI("tx_boot", "TX_BOOT_TIMING due_us=%llu wait_begin_us=%llu wait_end_us=%llu tick_begin=%lu tick_end=%lu wait_ticks=%lu probe_begin_us=%llu probe_end_us=%llu sampled=%d sample_us=%llu clock_order=%d result=%d",
                 (unsigned long long)due_us, (unsigned long long)wait_begin_us,
                 (unsigned long long)wait_end_us, (unsigned long)tick_begin,
                 (unsigned long)tick_end, (unsigned long)wait_ticks,
                 (unsigned long long)probe_begin_us, (unsigned long long)sample_us,
                 sampled, (unsigned long long)(sampled ? sample_us : 0), clock_order, result);
    } while (result == 0);
    if (result < 0) {
        stage = "BOOT_CHECK";
        error = ESP_ERR_INVALID_STATE;
        ESP_LOGE(TAG, "Boot check: elapsed_us=%llu sent=%lu errors=%lu restarts=%lu heap=%lu",
                 (unsigned long long)(esp_timer_get_time() - boot_started),
                 (unsigned long)status.sent_packets, (unsigned long)status.send_errors,
                 (unsigned long)status.wifi_restarts, (unsigned long)esp_get_free_heap_size());
        goto failed;
    }
    START("BOOT_ACCEPT", tx_ota_complete_boot_self_test(true));
    portENTER_CRITICAL(&s_status_lock);
    s_status.boot_check = 1;
    portEXIT_CRITICAL(&s_status_lock);
    TX_CHECKPOINT("BOOT_OK");
    ESP_LOGI(TAG, "TX1 local 60s boot check passed; external RX acceptance remains separate");
    return;
failed:
    /* Log before cleanup or the optional verified rollback can obscure failure. */
    TX_ERROR(stage, error);
    TX_CHECKPOINT("BOOT_STOP");
    if (timer != NULL) { (void)esp_timer_stop(timer); (void)esp_timer_delete(timer); }
    if (s_sender_task != NULL) { (void)esp_task_wdt_delete(s_sender_task); vTaskDelete(s_sender_task); }
    tx_network_stop();
    portENTER_CRITICAL(&s_status_lock);
    s_status.boot_check = -1;
    portEXIT_CRITICAL(&s_status_lock);
    ESP_LOGE(TAG, "TX1 start failed; stopped without erasing data");
    error = tx_ota_complete_boot_self_test(false);
    if (error != ESP_OK) TX_ERROR("BOOT_REJECT", error);
#undef START
}
