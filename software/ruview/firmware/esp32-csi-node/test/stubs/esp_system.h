#pragma once
#include "esp_stubs.h"
uint32_t esp_get_free_heap_size(void);
int esp_reset_reason(void);
void esp_restart(void);
