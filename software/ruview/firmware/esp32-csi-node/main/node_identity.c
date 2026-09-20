#include "node_identity.h"

#include <stddef.h>

bool node_identity_resolve(uint8_t wire_id, node_identity_t *identity)
{
    if (identity == NULL) {
        return false;
    }

    switch (wire_id) {
    case 1:
        *identity = (node_identity_t){.label = "RX1", .red = 10, .green = 132, .blue = 255};
        return true;
    case 2:
        *identity = (node_identity_t){.label = "RX2", .red = 48, .green = 209, .blue = 88};
        return true;
    case 3:
        *identity = (node_identity_t){.label = "RX3", .red = 255, .green = 55, .blue = 95};
        return true;
    case 4:
        *identity = (node_identity_t){.label = "RX4", .red = 191, .green = 90, .blue = 242};
        return true;
    default:
        *identity = (node_identity_t){0};
        return false;
    }
}
