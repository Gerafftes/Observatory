#include "measurement_stream.h"

#include <errno.h>
#include <inttypes.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>

#include "esp_log.h"
#include "esp_system.h"
#include "sdkconfig.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "lwip/inet.h"
#include "lwip/sockets.h"

#include "coordinate_transform.h"
#include "measurement_ack.h"
#include "mmwave_discovery.h"

static const char *TAG = "measurement_stream";
#ifndef CONFIG_MMWAVE_UDP_ACK_ATTEMPTS
#define CONFIG_MMWAVE_UDP_ACK_ATTEMPTS 8
#endif
#ifndef CONFIG_MMWAVE_UDP_ACK_TIMEOUT_MS
#define CONFIG_MMWAVE_UDP_ACK_TIMEOUT_MS 50
#endif

static bool append_json(char *buffer, size_t capacity, size_t *used,
                        const char *format, ...)
{
    if (*used >= capacity) {
        return false;
    }
    va_list arguments;
    va_start(arguments, format);
    int written = vsnprintf(buffer + *used, capacity - *used, format, arguments);
    va_end(arguments);
    if (written < 0 || (size_t)written >= capacity - *used) {
        return false;
    }
    *used += (size_t)written;
    return true;
}

struct measurement_stream {
    int socket_fd;
    mmwave_discovery_t *discovery;
    app_config_t *config;
    uint32_t sequence;
    uint32_t boot_id;
};

static bool sockaddr_matches(const struct sockaddr_in *left,
                             const struct sockaddr_in *right)
{
    return left->sin_family == right->sin_family &&
           left->sin_port == right->sin_port &&
           left->sin_addr.s_addr == right->sin_addr.s_addr;
}

static bool wait_for_ack(measurement_stream_t *stream,
                         const struct sockaddr_in *destination,
                         uint32_t sequence)
{
    uint8_t ack[MMWAVE_ACK_SIZE];
    while (true) {
        struct sockaddr_in source = {0};
        socklen_t source_length = sizeof(source);
        ssize_t received = recvfrom(stream->socket_fd, ack, sizeof(ack), 0,
                                    (struct sockaddr *)&source, &source_length);
        if (received < 0) {
            if (errno == EINTR) {
                continue;
            }
            return false;
        }
        if (sockaddr_matches(&source, destination) &&
            mmwave_ack_matches(ack, (size_t)received, stream->boot_id,
                               sequence)) {
            return true;
        }
    }
}

measurement_stream_t *measurement_stream_create(app_config_t *config)
{
    measurement_stream_t *stream = calloc(1, sizeof(*stream));
    if (stream == NULL) {
        return NULL;
    }
    stream->config = config;
    stream->boot_id = esp_random();
    stream->discovery = mmwave_discovery_create();
    if (stream->discovery == NULL) {
        ESP_LOGW(TAG, "Collector discovery socket unavailable; using persisted target only");
    }
    stream->socket_fd = socket(AF_INET, SOCK_DGRAM, IPPROTO_IP);
    if (stream->socket_fd < 0 ||
        !app_config_transport_valid(config->target_host, config->target_port)) {
        ESP_LOGE(TAG, "Cannot create UDP stream for %s:%u",
                 config->target_host, config->target_port);
        measurement_stream_destroy(stream);
        return NULL;
    }
    const struct timeval ack_timeout = {
        .tv_sec = 0,
        .tv_usec = CONFIG_MMWAVE_UDP_ACK_TIMEOUT_MS * 1000,
    };
    if (setsockopt(stream->socket_fd, SOL_SOCKET, SO_RCVTIMEO, &ack_timeout,
                   sizeof(ack_timeout)) < 0) {
        ESP_LOGE(TAG, "Cannot configure UDP ACK timeout");
        measurement_stream_destroy(stream);
        return NULL;
    }
    return stream;
}

void measurement_stream_destroy(measurement_stream_t *stream)
{
    if (stream == NULL) {
        return;
    }
    if (stream->socket_fd >= 0) {
        close(stream->socket_fd);
    }
    mmwave_discovery_destroy(stream->discovery);
    free(stream);
}

measurement_stream_result_t measurement_stream_send(
    measurement_stream_t *stream, const ld2450_frame_t *frame,
    int64_t monotonic_time_us)
{
    measurement_stream_result_t result = {0};
    mmwave_discovery_poll(stream->discovery, stream->config, monotonic_time_us);
    app_config_t config;
    app_config_snapshot(stream->config, &config);
    struct sockaddr_in destination = {
        .sin_family = AF_INET,
        .sin_port = htons(config.target_port),
    };
    if (inet_pton(AF_INET, config.target_host, &destination.sin_addr) != 1) {
        ESP_LOGE(TAG, "Invalid UDP target %s:%u",
                 config.target_host, config.target_port);
        return result;
    }
    char json[1152];
    struct timeval wall_time;
    gettimeofday(&wall_time, NULL);
    int64_t unix_time_ms = wall_time.tv_sec > 1700000000
        ? (int64_t)wall_time.tv_sec * 1000 + wall_time.tv_usec / 1000
        : 0;

    size_t used = 0;
    uint32_t sequence = stream->sequence++;
    bool valid = append_json(json, sizeof(json), &used,
        "{\"schema\":\"ruview.mmwave.ld2450.v1\",\"node_id\":\"%s\","
        "\"mode\":\"%s\",\"boot_id\":%" PRIu32 ",\"sequence\":%" PRIu32 ","
        "\"sensor_time_us\":%lld,\"unix_time_ms\":%lld,"
        "\"coordinate_frame\":{\"local\":\"x_right_y_forward_mm\","
        "\"room\":\"x_length_z_width_mm\",\"origin_x_mm\":%ld,"
        "\"origin_z_mm\":%ld,\"yaw_mdeg\":%ld,\"raw_x_inverted\":%s},"
        "\"targets\":[",
        config.node_id, measurement_mode_name(config.mode),
        stream->boot_id, sequence, (long long)monotonic_time_us,
        (long long)unix_time_ms, (long)config.origin_x_mm,
        (long)config.origin_z_mm, (long)config.yaw_mdeg,
        config.invert_raw_x ? "true" : "false");

    for (size_t index = 0; index < LD2450_TARGET_COUNT && valid; ++index) {
        const ld2450_target_t *target = &frame->targets[index];
        int32_t room_x_mm = 0;
        int32_t room_z_mm = 0;
        if (target->present) {
            coordinate_transform_to_room(
                config.origin_x_mm, config.origin_z_mm,
                config.yaw_mdeg, config.invert_raw_x,
                target->x_mm, target->y_mm, &room_x_mm, &room_z_mm);
        }
        valid = append_json(json, sizeof(json), &used,
            "%s{\"slot\":%u,\"present\":%s,\"x_mm\":%d,\"y_mm\":%d,"
            "\"room_x_mm\":%ld,\"room_z_mm\":%ld,\"speed_cm_s\":%d,"
            "\"resolution_mm\":%u}",
            index == 0 ? "" : ",", (unsigned)(index + 1),
            target->present ? "true" : "false", target->x_mm, target->y_mm,
            (long)room_x_mm, (long)room_z_mm, target->speed_cm_s,
            target->resolution_mm);
    }
    if (!valid || !append_json(json, sizeof(json), &used, "]}")) {
        ESP_LOGE(TAG, "Measurement JSON overflow");
        return result;
    }

    // sendto() only confirms local queueing. The collector ACK closes the
    // actual Node -> WLAN -> server path; retries reuse the same sequence so
    // the server can safely deduplicate a late ACK or retransmission.
    for (unsigned attempt = 0; attempt < CONFIG_MMWAVE_UDP_ACK_ATTEMPTS; ++attempt) {
        result.attempts += 1;
        ssize_t sent = sendto(stream->socket_fd, json, used, 0,
                              (struct sockaddr *)&destination,
                              sizeof(destination));
        if (sent == (ssize_t)used) {
            result.sent = true;
            if (wait_for_ack(stream, &destination, sequence)) {
                result.acknowledged = true;
                break;
            }
        } else {
            vTaskDelay(1);
        }
    }
    if (!result.acknowledged) {
        ESP_LOGW(TAG, "No collector ACK for sequence %" PRIu32 " after %u attempts",
                 sequence, result.attempts);
    }
    return result;
}
