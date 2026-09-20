#pragma once

#include <stdint.h>
#include <string.h>

static inline uint8_t status_led_identity_pulse_count(const char *node_id)
{
    if (node_id != NULL && strcmp(node_id, "MMWAVE1") == 0) {
        return 1;
    }
    if (node_id != NULL && strcmp(node_id, "MMWAVE2") == 0) {
        return 2;
    }
    return 0;
}

void status_led_start(const char *node_id);
