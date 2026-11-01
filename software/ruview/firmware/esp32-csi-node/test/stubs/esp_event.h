#pragma once
#include "esp_stubs.h"
typedef const char *esp_event_base_t;
typedef void *esp_event_handler_instance_t;
extern const char RX_TEST_WIFI_EVENT[], RX_TEST_IP_EVENT[];
#define WIFI_EVENT RX_TEST_WIFI_EVENT
#define IP_EVENT RX_TEST_IP_EVENT
#define ESP_EVENT_ANY_ID (-1)
#define WIFI_EVENT_STA_START 1
#define WIFI_EVENT_STA_DISCONNECTED 2
#define IP_EVENT_STA_GOT_IP 3
typedef struct { uint32_t reason; } wifi_event_sta_disconnected_t;
esp_err_t esp_event_loop_create_default(void);
esp_err_t esp_event_loop_delete_default(void);
esp_err_t esp_event_handler_instance_register(esp_event_base_t, int32_t,
    void (*)(void *, esp_event_base_t, int32_t, void *), void *, esp_event_handler_instance_t *);
esp_err_t esp_event_handler_instance_unregister(esp_event_base_t, int32_t, esp_event_handler_instance_t);
