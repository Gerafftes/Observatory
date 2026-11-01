#pragma once
#include "esp_stubs.h"
typedef void (*esp_heap_caps_alloc_failed_hook_t)(size_t, uint32_t, const char *);
static inline esp_err_t heap_caps_register_failed_alloc_callback(esp_heap_caps_alloc_failed_hook_t hook)
{ (void)hook; return ESP_OK; }
#define MALLOC_CAP_INTERNAL 1u
#define MALLOC_CAP_8BIT 2u
#ifdef RX_OTA_TEST
bool heap_caps_check_integrity(uint32_t caps, bool print_errors);
#endif
