#include "identity_indicator.h"

#include <string.h>

int identity_indicator_pulse_count(const char *node_id)
{
    if (node_id == NULL) {
        return 0;
    }
    if (strcmp(node_id, "MMWAVE1") == 0) {
        return 1;
    }
    if (strcmp(node_id, "MMWAVE2") == 0) {
        return 2;
    }
    return 0;
}
