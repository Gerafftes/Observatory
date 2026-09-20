#pragma once

#include <stdbool.h>
#include <stdint.h>

typedef struct {
    const char *label;
    uint8_t red;
    uint8_t green;
    uint8_t blue;
} node_identity_t;

/**
 * Resolve an RX wire ID to its stable visible identity.
 *
 * The mapping contains no position. Unknown IDs deliberately return false so
 * an unprovisioned device cannot masquerade as one of the configured RXs.
 */
bool node_identity_resolve(uint8_t wire_id, node_identity_t *identity);
