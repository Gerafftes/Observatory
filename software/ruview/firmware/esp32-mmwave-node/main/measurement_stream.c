#include "measurement_stream.h"

#include <inttypes.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>

#include "esp_log.h"
#include "esp_system.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "lwip/inet.h"
#include "lwip/sockets.h"

#include "coordinate_transform.h"

static const char *TAG = "measurement_stream";
#define UDP_REDUNDANT_COPIES 3
#define UDP_COPY_DELAY_MS 5

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
    struct sockaddr_in destination;
    const app_config_t *config;
    uint32_t sequence;
    uint32_t boot_id;
};

measurement_stream_t *measurement_stream_create(const app_config_t *config)
{
    measurement_stream_t *stream = calloc(1, sizeof(*stream));
    if (stream == NULL) {
        return NULL;
    }
    stream->config = config;
    stream->boot_id = esp_random();
    stream->socket_fd = socket(AF_INET, SOCK_DGRAM, IPPROTO_IP);
    if (stream->socket_fd < 0 ||
        inet_pton(AF_INET, config->target_host, &stream->destination.sin_addr) != 1) {
        ESP_LOGE(TAG, "Cannot create UDP stream for %s:%u",
                 config->target_host, config->target_port);
        measurement_stream_destroy(stream);
        return NULL;
    }
    stream->destination.sin_family = AF_INET;
    stream->destination.sin_port = htons(config->target_port);
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
    free(stream);
}

bool measurement_stream_send(measurement_stream_t *stream,
                             const ld2450_frame_t *frame,
                             int64_t monotonic_time_us)
{
    app_config_t config;
    app_config_snapshot(stream->config, &config);
    char json[1152];
    struct timeval wall_time;
    gettimeofday(&wall_time, NULL);
    int64_t unix_time_ms = wall_time.tv_sec > 1700000000
        ? (int64_t)wall_time.tv_sec * 1000 + wall_time.tv_usec / 1000
        : 0;

    size_t used = 0;
    bool valid = append_json(json, sizeof(json), &used,
        "{\"schema\":\"ruview.mmwave.ld2450.v1\",\"node_id\":\"%s\","
        "\"mode\":\"%s\",\"boot_id\":%" PRIu32 ",\"sequence\":%" PRIu32 ","
        "\"sensor_time_us\":%lld,\"unix_time_ms\":%lld,"
        "\"coordinate_frame\":{\"local\":\"x_right_y_forward_mm\","
        "\"room\":\"x_length_z_width_mm\",\"origin_x_mm\":%ld,"
        "\"origin_z_mm\":%ld,\"yaw_mdeg\":%ld,\"raw_x_inverted\":%s},"
        "\"targets\":[",
        config.node_id, measurement_mode_name(config.mode),
        stream->boot_id, stream->sequence++, (long long)monotonic_time_us,
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
        return false;
    }

    // UDP success only confirms that lwIP accepted the datagram; it does not
    // confirm delivery across the experiment WLAN. Send identical sequence
    // numbers a few milliseconds apart. The server already deduplicates them
    // before sequence validation, while temporal diversity prevents isolated
    // WiFi loss from invalidating the 25-second calibration preflight.
    bool sent = false;
    for (unsigned copy = 0; copy < UDP_REDUNDANT_COPIES; ++copy) {
        sent = sendto(stream->socket_fd, json, used, 0,
                      (struct sockaddr *)&stream->destination,
                      sizeof(stream->destination)) >= 0 || sent;
        if (copy + 1 < UDP_REDUNDANT_COPIES) {
            vTaskDelay(pdMS_TO_TICKS(UDP_COPY_DELAY_MS));
        }
    }
    if (!sent) {
        ESP_LOGW(TAG, "All %u redundant UDP sends failed", UDP_REDUNDANT_COPIES);
    }
    return sent;
}
