#pragma once

#include <stdbool.h>
#include <stdint.h>

#include "app_config.h"
#include "ld2450_parser.h"

typedef struct measurement_stream measurement_stream_t;

measurement_stream_t *measurement_stream_create(const app_config_t *config);
void measurement_stream_destroy(measurement_stream_t *stream);
bool measurement_stream_send(measurement_stream_t *stream,
                             const ld2450_frame_t *frame,
                             int64_t monotonic_time_us);
