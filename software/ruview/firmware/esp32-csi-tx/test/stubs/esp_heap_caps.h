#ifndef TX_TEST_ESP_HEAP_CAPS_H
#define TX_TEST_ESP_HEAP_CAPS_H
#include <stddef.h>
#include <stdint.h>
#define MALLOC_CAP_8BIT (1U << 2)
#define MALLOC_CAP_INTERNAL (1U << 11)
size_t heap_caps_get_free_size(uint32_t caps);
size_t heap_caps_get_largest_free_block(uint32_t caps);
size_t heap_caps_get_minimum_free_size(uint32_t caps);
#endif
