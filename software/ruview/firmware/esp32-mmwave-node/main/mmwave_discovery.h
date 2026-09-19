#pragma once

#include <stdint.h>

#include "app_config.h"

typedef struct mmwave_discovery mmwave_discovery_t;

mmwave_discovery_t *mmwave_discovery_create(void);
void mmwave_discovery_destroy(mmwave_discovery_t *discovery);
void mmwave_discovery_poll(mmwave_discovery_t *discovery,
                           app_config_t *config,
                           int64_t monotonic_time_us);
