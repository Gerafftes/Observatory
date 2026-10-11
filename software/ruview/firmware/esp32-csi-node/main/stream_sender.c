/**
 * @file stream_sender.c
 * @brief UDP stream sender for CSI frames.
 *
 * Opens a UDP socket and sends serialized ADR-018 frames to the aggregator.
 */

#include "stream_sender.h"

#include <stdbool.h>
#include <string.h>
#include "esp_log.h"
#include "esp_timer.h"
#include "lwip/sockets.h"
#include "lwip/netdb.h"
#include "sdkconfig.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"

static const char *TAG = "stream_sender";

static int s_sock = -1;
static struct sockaddr_in s_dest_addr;
static char s_target_ip[16];
static uint16_t s_target_port;
static bool s_target_configured;
static volatile bool s_rebuild_requested;
static portMUX_TYPE s_config_lock = portMUX_INITIALIZER_UNLOCKED;
static volatile uint32_t s_socket_rebuilds;
static volatile uint32_t s_send_failures;
static volatile uint32_t s_enomem_backoffs;
static portMUX_TYPE s_stats_lock = portMUX_INITIALIZER_UNLOCKED;
static volatile uint32_t s_send_calls;
static volatile uint32_t s_invalid_calls;
static volatile uint32_t s_socket_unavailable;
static volatile uint32_t s_backoff_suppressed;
static volatile uint32_t s_sendto_attempts;
static volatile uint32_t s_sendto_failures;
static volatile uint32_t s_os_datagrams_accepted;
static volatile uint32_t s_partial_sends;
static volatile uint32_t s_csi_send_calls;
static volatile uint32_t s_csi_socket_unavailable;
static volatile uint32_t s_csi_backoff_suppressed;
static volatile uint32_t s_csi_sendto_attempts;
static volatile uint32_t s_csi_sendto_failures;
static volatile uint32_t s_csi_os_datagrams_accepted;
static int64_t s_last_sendto_attempt_us;
static int64_t s_last_os_accept_us;
static StaticSemaphore_t s_sender_mutex_storage;
static SemaphoreHandle_t s_sender_mutex;

static bool sender_lock(void)
{
    if (s_sender_mutex == NULL) {
        portENTER_CRITICAL(&s_config_lock);
        if (s_sender_mutex == NULL) {
            s_sender_mutex = xSemaphoreCreateMutexStatic(&s_sender_mutex_storage);
        }
        portEXIT_CRITICAL(&s_config_lock);
    }
    return s_sender_mutex != NULL
        && xSemaphoreTake(s_sender_mutex, portMAX_DELAY) == pdTRUE;
}

static bool sender_lock_until(int64_t deadline_us)
{
    if (deadline_us <= 0) {
        return sender_lock();
    }
    if (s_sender_mutex == NULL) {
        portENTER_CRITICAL(&s_config_lock);
        if (s_sender_mutex == NULL) {
            s_sender_mutex = xSemaphoreCreateMutexStatic(&s_sender_mutex_storage);
        }
        portEXIT_CRITICAL(&s_config_lock);
    }
    if (s_sender_mutex == NULL) {
        return false;
    }

    for (;;) {
        int64_t now_us = esp_timer_get_time();
        if (now_us > deadline_us) {
            return false;
        }
        uint64_t remaining_ms = (uint64_t)(deadline_us - now_us + 999) / 1000;
        uint32_t timeout_ticks = (uint32_t)pdMS_TO_TICKS(remaining_ms);
        if (timeout_ticks == 0) {
            timeout_ticks = 1;
        }
        if (xSemaphoreTake(s_sender_mutex, timeout_ticks) == pdTRUE) {
            return true;
        }
        if (esp_timer_get_time() > deadline_us) {
            return false;
        }
    }
}

static void sender_unlock(void)
{
    if (s_sender_mutex != NULL) {
        (void)xSemaphoreGive(s_sender_mutex);
    }
}

/**
 * ENOMEM backoff state.
 * When sendto fails with ENOMEM (errno 12), we suppress further sends for
 * a cooldown period to let lwIP reclaim packet buffers.  Without this,
 * rapid-fire CSI callbacks can exhaust the pbuf pool and crash the device.
 */
static int64_t s_backoff_until_us = 0;       /* esp_timer timestamp to resume */
#define ENOMEM_COOLDOWN_MS  100              /* base backoff; doubles per streak */
#define ENOMEM_COOLDOWN_MAX_MS 2000          /* cap on the exponential backoff */
#define ENOMEM_LOG_INTERVAL 50               /* log every Nth suppressed send */
static uint32_t s_enomem_suppressed = 0;
/* Consecutive ENOMEM episodes without an intervening successful send. A fixed
 * 100 ms backoff is too short to drain sustained lwIP/WiFi buffer pressure
 * (#1135 bug #1: tier-2 + concurrent TX keeps the node stuck), so the backoff
 * grows 100→200→400→…→2000 ms per streak and resets on the first send that
 * succeeds. */
static uint32_t s_enomem_streak = 0;

#ifdef CSI_DIAGNOSTICS_TEST
static int s_test_sendto_result;
static int s_test_sendto_errno;
static uint8_t s_test_last_payload[4096];
static size_t s_test_last_payload_len;
#endif

static bool sender_payload_is_csi(const uint8_t *data, size_t len)
{
    const uint32_t csi_magic = 0xC5110001u;
    uint32_t observed = 0;
    if (data == NULL || len < sizeof(observed)) {
        return false;
    }
    memcpy(&observed, data, sizeof(observed));
    return observed == csi_magic;
}

static int sender_sendto(const uint8_t *data, size_t len)
{
#ifdef CSI_DIAGNOSTICS_TEST
    s_test_last_payload_len = len < sizeof(s_test_last_payload)
        ? len
        : sizeof(s_test_last_payload);
    memcpy(s_test_last_payload, data, s_test_last_payload_len);
    errno = s_test_sendto_errno;
    return s_test_sendto_result;
#else
    return sendto(s_sock, data, len, 0,
                  (struct sockaddr *)&s_dest_addr, sizeof(s_dest_addr));
#endif
}

/* The caller owns s_sender_mutex. */
static int sender_open_socket_locked(void)
{
    char ip[sizeof(s_target_ip)];
    uint16_t port;
    portENTER_CRITICAL(&s_config_lock);
    memcpy(ip, s_target_ip, sizeof(ip));
    port = s_target_port;
    bool configured = s_target_configured;
    s_rebuild_requested = false;
    portEXIT_CRITICAL(&s_config_lock);
    if (!configured) {
        return -1;
    }

    if (s_sock >= 0) {
        close(s_sock);
        s_sock = -1;
    }
    s_sock = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    if (s_sock < 0) {
        ESP_LOGE(TAG, "Failed to create socket: errno %d", errno);
        s_rebuild_requested = true;
        return -1;
    }

    memset(&s_dest_addr, 0, sizeof(s_dest_addr));
    s_dest_addr.sin_family = AF_INET;
    s_dest_addr.sin_port = htons(port);

    if (inet_pton(AF_INET, ip, &s_dest_addr.sin_addr) <= 0) {
        ESP_LOGE(TAG, "Invalid target IP: %s", ip);
        close(s_sock);
        s_sock = -1;
        s_rebuild_requested = true;
        return -1;
    }

    s_socket_rebuilds++;
    ESP_LOGI(TAG, "UDP sender initialized: %s:%d", ip, port);
    return 0;
}

static int sender_init_internal(const char *ip, uint16_t port)
{
    if (ip == NULL || strnlen(ip, sizeof(s_target_ip)) >= sizeof(s_target_ip) || port == 0) {
        return -1;
    }
    portENTER_CRITICAL(&s_config_lock);
    strncpy(s_target_ip, ip, sizeof(s_target_ip) - 1);
    s_target_ip[sizeof(s_target_ip) - 1] = '\0';
    s_target_port = port;
    s_target_configured = true;
    s_rebuild_requested = true;
    portEXIT_CRITICAL(&s_config_lock);
    if (!sender_lock()) {
        return -1;
    }
    int result = sender_open_socket_locked();
    sender_unlock();
    return result;
}

int stream_sender_init(void)
{
    return sender_init_internal(CONFIG_CSI_TARGET_IP, CONFIG_CSI_TARGET_PORT);
}

int stream_sender_init_with(const char *ip, uint16_t port)
{
    return sender_init_internal(ip, port);
}

int stream_sender_reconfigure(const char *ip, uint16_t port)
{
    if (ip == NULL || strnlen(ip, sizeof(s_target_ip)) >= sizeof(s_target_ip) || port == 0) {
        return -1;
    }
    if (!sender_lock()) {
        return -1;
    }
    portENTER_CRITICAL(&s_config_lock);
    strncpy(s_target_ip, ip, sizeof(s_target_ip) - 1);
    s_target_ip[sizeof(s_target_ip) - 1] = '\0';
    s_target_port = port;
    s_target_configured = true;
    s_rebuild_requested = true;
    portEXIT_CRITICAL(&s_config_lock);
    sender_unlock();
    return 0;
}

void stream_sender_notify_network_change(void)
{
    if (sender_lock()) {
        s_rebuild_requested = true;
        sender_unlock();
    }
}

static int stream_sender_send_internal(
    const uint8_t *data,
    size_t len,
    int64_t deadline_us,
    bool *deadline_expired
)
{
    if (deadline_expired != NULL) {
        *deadline_expired = false;
    }
    bool is_csi = sender_payload_is_csi(data, len);
    portENTER_CRITICAL(&s_stats_lock);
    s_send_calls++;
    if (is_csi) {
        s_csi_send_calls++;
    }
    portEXIT_CRITICAL(&s_stats_lock);
    if (data == NULL || len == 0) {
        portENTER_CRITICAL(&s_stats_lock);
        s_invalid_calls++;
        s_send_failures++;
        portEXIT_CRITICAL(&s_stats_lock);
        return -1;
    }

    if (!sender_lock_until(deadline_us)) {
        bool expired = deadline_us > 0 && esp_timer_get_time() > deadline_us;
        if (deadline_expired != NULL) {
            *deadline_expired = expired;
        }
        portENTER_CRITICAL(&s_stats_lock);
        s_send_failures++;
        if (!expired) {
            s_socket_unavailable++;
            if (is_csi) {
                s_csi_socket_unavailable++;
            }
        }
        portEXIT_CRITICAL(&s_stats_lock);
        return -1;
    }

    if (deadline_us > 0 && esp_timer_get_time() > deadline_us) {
        sender_unlock();
        if (deadline_expired != NULL) {
            *deadline_expired = true;
        }
        return -1;
    }

    if ((s_sock < 0 || s_rebuild_requested) && sender_open_socket_locked() != 0) {
        portENTER_CRITICAL(&s_stats_lock);
        s_send_failures++;
        s_socket_unavailable++;
        if (is_csi) {
            s_csi_socket_unavailable++;
        }
        portEXIT_CRITICAL(&s_stats_lock);
        sender_unlock();
        return -1;
    }

    if (deadline_us > 0 && esp_timer_get_time() > deadline_us) {
        sender_unlock();
        if (deadline_expired != NULL) {
            *deadline_expired = true;
        }
        return -1;
    }

    /* ENOMEM backoff: if we recently exhausted lwIP buffers, skip sends
     * until the cooldown expires.  This prevents the cascade of failed
     * sendto calls that leads to a guru meditation crash. */
    if (s_backoff_until_us > 0) {
        int64_t now = esp_timer_get_time();
        if (now < s_backoff_until_us) {
            s_enomem_suppressed++;
            portENTER_CRITICAL(&s_stats_lock);
            s_backoff_suppressed++;
            if (is_csi) {
                s_csi_backoff_suppressed++;
            }
            portEXIT_CRITICAL(&s_stats_lock);
            if ((s_enomem_suppressed % ENOMEM_LOG_INTERVAL) == 1) {
                ESP_LOGW(TAG, "sendto suppressed (ENOMEM backoff, %lu dropped)",
                         (unsigned long)s_enomem_suppressed);
            }
            sender_unlock();
            return -1;
        }
        /* Cooldown expired — resume sending */
        ESP_LOGI(TAG, "ENOMEM backoff expired, resuming sends (%lu were suppressed)",
                 (unsigned long)s_enomem_suppressed);
        s_backoff_until_us = 0;
        s_enomem_suppressed = 0;
    }

    int64_t attempted_at_us = esp_timer_get_time();
    if (deadline_us > 0 && attempted_at_us > deadline_us) {
        sender_unlock();
        if (deadline_expired != NULL) {
            *deadline_expired = true;
        }
        return -1;
    }
    portENTER_CRITICAL(&s_stats_lock);
    s_sendto_attempts++;
    s_last_sendto_attempt_us = attempted_at_us;
    if (is_csi) {
        s_csi_sendto_attempts++;
    }
    portEXIT_CRITICAL(&s_stats_lock);
    int sent = sender_sendto(data, len);
    if (sent < 0) {
        if (errno == ENOMEM) {
            /* Exponential backoff: double the cooldown each consecutive ENOMEM
             * (capped) so sustained buffer pressure actually drains instead of
             * the node re-failing every 100 ms forever (#1135 bug #1). */
            uint32_t shift = s_enomem_streak < 5 ? s_enomem_streak : 5;
            uint32_t cooldown = ENOMEM_COOLDOWN_MS << shift;
            if (cooldown > ENOMEM_COOLDOWN_MAX_MS) cooldown = ENOMEM_COOLDOWN_MAX_MS;
            s_enomem_streak++;
            portENTER_CRITICAL(&s_stats_lock);
            s_enomem_backoffs++;
            portEXIT_CRITICAL(&s_stats_lock);
            s_backoff_until_us = esp_timer_get_time() + (int64_t)cooldown * 1000;
            ESP_LOGW(TAG, "sendto ENOMEM — backing off for %lu ms (streak %lu)",
                     (unsigned long)cooldown, (unsigned long)s_enomem_streak);
        } else {
            ESP_LOGW(TAG, "sendto failed: errno %d", errno);
            s_rebuild_requested = true;
        }
        portENTER_CRITICAL(&s_stats_lock);
        s_send_failures++;
        s_sendto_failures++;
        if (is_csi) {
            s_csi_sendto_failures++;
        }
        portEXIT_CRITICAL(&s_stats_lock);
        sender_unlock();
        return -1;
    }

    portENTER_CRITICAL(&s_stats_lock);
    s_os_datagrams_accepted++;
    s_last_os_accept_us = esp_timer_get_time();
    if (is_csi) {
        s_csi_os_datagrams_accepted++;
    }
    if ((size_t)sent != len) {
        s_partial_sends++;
    }
    portEXIT_CRITICAL(&s_stats_lock);

    /* A send got through — buffer pressure cleared; reset the backoff streak. */
    s_enomem_streak = 0;
    sender_unlock();
    return sent;
}

int stream_sender_send(const uint8_t *data, size_t len)
{
    return stream_sender_send_internal(data, len, 0, NULL);
}

int stream_sender_send_until(
    const uint8_t *data,
    size_t len,
    int64_t deadline_us,
    bool *deadline_expired
)
{
    return stream_sender_send_internal(data, len, deadline_us, deadline_expired);
}

void stream_sender_get_stats(stream_sender_stats_t *stats)
{
    if (stats == NULL) {
        return;
    }
    portENTER_CRITICAL(&s_stats_lock);
    *stats = (stream_sender_stats_t) {
        .socket_rebuilds = s_socket_rebuilds,
        .send_failures = s_send_failures,
        .enomem_backoffs = s_enomem_backoffs,
        .send_calls = s_send_calls,
        .invalid_calls = s_invalid_calls,
        .socket_unavailable = s_socket_unavailable,
        .backoff_suppressed = s_backoff_suppressed,
        .sendto_attempts = s_sendto_attempts,
        .sendto_failures = s_sendto_failures,
        .os_datagrams_accepted = s_os_datagrams_accepted,
        .partial_sends = s_partial_sends,
        .csi_send_calls = s_csi_send_calls,
        .csi_socket_unavailable = s_csi_socket_unavailable,
        .csi_backoff_suppressed = s_csi_backoff_suppressed,
        .csi_sendto_attempts = s_csi_sendto_attempts,
        .csi_sendto_failures = s_csi_sendto_failures,
        .csi_os_datagrams_accepted = s_csi_os_datagrams_accepted,
        .last_sendto_attempt_us = s_last_sendto_attempt_us,
        .last_os_accept_us = s_last_os_accept_us,
    };
    portEXIT_CRITICAL(&s_stats_lock);
}

#ifdef CSI_DIAGNOSTICS_TEST
void stream_sender_test_reset(void)
{
    (void)sender_lock();
    s_sock = 1;
    s_target_configured = true;
    s_rebuild_requested = false;
    s_socket_rebuilds = 0;
    s_send_failures = 0;
    s_enomem_backoffs = 0;
    s_send_calls = 0;
    s_invalid_calls = 0;
    s_socket_unavailable = 0;
    s_backoff_suppressed = 0;
    s_sendto_attempts = 0;
    s_sendto_failures = 0;
    s_os_datagrams_accepted = 0;
    s_partial_sends = 0;
    s_csi_send_calls = 0;
    s_csi_socket_unavailable = 0;
    s_csi_backoff_suppressed = 0;
    s_csi_sendto_attempts = 0;
    s_csi_sendto_failures = 0;
    s_csi_os_datagrams_accepted = 0;
    s_last_sendto_attempt_us = 0;
    s_last_os_accept_us = 0;
    s_backoff_until_us = 0;
    s_enomem_suppressed = 0;
    s_enomem_streak = 0;
    s_test_sendto_result = 0;
    s_test_sendto_errno = 0;
    s_test_last_payload_len = 0;
    memset(s_test_last_payload, 0, sizeof(s_test_last_payload));
    sender_unlock();
}

void stream_sender_test_set_sendto_result(int result, int error_number)
{
    s_test_sendto_result = result;
    s_test_sendto_errno = error_number;
}

size_t stream_sender_test_copy_last_payload(uint8_t *buffer, size_t buffer_size)
{
    if (buffer == NULL || buffer_size == 0) {
        return s_test_last_payload_len;
    }
    size_t copy_len = s_test_last_payload_len < buffer_size
        ? s_test_last_payload_len
        : buffer_size;
    memcpy(buffer, s_test_last_payload, copy_len);
    return s_test_last_payload_len;
}
#endif

void stream_sender_deinit(void)
{
    if (!sender_lock()) {
        return;
    }
    if (s_sock >= 0) {
        close(s_sock);
        s_sock = -1;
        ESP_LOGI(TAG, "UDP sender closed");
    }
    sender_unlock();
}
