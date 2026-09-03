#include <string.h>

#include "driver/uart.h"
#include "esp_event.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_netif_sntp.h"
#include "esp_ota_ops.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "freertos/task.h"
#include "nvs_flash.h"

#include "app_config.h"
#include "ld2450_parser.h"
#include "measurement_stream.h"
#include "node_diagnostics.h"
#include "web_server.h"

#define RADAR_UART UART_NUM_1
#define RADAR_RX_GPIO 20
#define RADAR_TX_GPIO 21
#define RADAR_BAUD 256000
#define RADAR_NETWORK_SETTLE_MS 1000
#define RADAR_STREAM_INTERVAL_US 100000
#define WIFI_CONNECTED_BIT BIT0

static const char *TAG = "mmwave_node";
static EventGroupHandle_t s_wifi_events;
static app_config_t s_config;

static void wifi_event_handler(void *argument, esp_event_base_t base,
                               int32_t event_id, void *event_data)
{
    (void)argument;
    (void)event_data;
    if (base == WIFI_EVENT && event_id == WIFI_EVENT_STA_START) {
        esp_wifi_connect();
    } else if (base == WIFI_EVENT && event_id == WIFI_EVENT_STA_DISCONNECTED) {
        xEventGroupClearBits(s_wifi_events, WIFI_CONNECTED_BIT);
        esp_wifi_connect();
    } else if (base == IP_EVENT && event_id == IP_EVENT_STA_GOT_IP) {
        xEventGroupSetBits(s_wifi_events, WIFI_CONNECTED_BIT);
    }
}

static void wifi_start(void)
{
    s_wifi_events = xEventGroupCreate();
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    esp_netif_create_default_wifi_sta();
    wifi_init_config_t initialization = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&initialization));
    ESP_ERROR_CHECK(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID,
                                               wifi_event_handler, NULL));
    ESP_ERROR_CHECK(esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP,
                                               wifi_event_handler, NULL));
    wifi_config_t wifi = {0};
    strlcpy((char *)wifi.sta.ssid, s_config.wifi_ssid, sizeof(wifi.sta.ssid));
    strlcpy((char *)wifi.sta.password, s_config.wifi_password,
            sizeof(wifi.sta.password));
    wifi.sta.threshold.authmode = s_config.wifi_password[0] == '\0'
        ? WIFI_AUTH_OPEN : WIFI_AUTH_WPA2_PSK;
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &wifi));
    ESP_ERROR_CHECK(esp_wifi_start());
    xEventGroupWaitBits(s_wifi_events, WIFI_CONNECTED_BIT, pdFALSE, pdTRUE,
                        portMAX_DELAY);
}

static void time_sync_start(void)
{
    esp_sntp_config_t config = ESP_NETIF_SNTP_DEFAULT_CONFIG("pool.ntp.org");
    esp_netif_sntp_init(&config);
}

static void radar_task(void *argument)
{
    (void)argument;
    const uart_config_t uart_config = {
        .baud_rate = RADAR_BAUD,
        .data_bits = UART_DATA_8_BITS,
        .parity = UART_PARITY_DISABLE,
        .stop_bits = UART_STOP_BITS_1,
        .flow_ctrl = UART_HW_FLOWCTRL_DISABLE,
        .source_clk = UART_SCLK_DEFAULT,
    };
    ESP_ERROR_CHECK(uart_param_config(RADAR_UART, &uart_config));
    ESP_ERROR_CHECK(uart_set_pin(RADAR_UART, RADAR_TX_GPIO, RADAR_RX_GPIO,
                                 UART_PIN_NO_CHANGE, UART_PIN_NO_CHANGE));
    ESP_ERROR_CHECK(uart_driver_install(RADAR_UART, 2048, 0, 0, NULL, 0));

    // Let the freshly restored WiFi route and ARP entry settle before the
    // first radar frame is counted as a transport diagnostic.
    vTaskDelay(pdMS_TO_TICKS(RADAR_NETWORK_SETTLE_MS));
    measurement_stream_t *stream = measurement_stream_create(&s_config);
    if (stream == NULL) {
        ESP_LOGE(TAG, "Cannot start measurement stream");
        vTaskDelete(NULL);
    }
    ld2450_parser_t parser;
    ld2450_parser_init(&parser);
    uint8_t bytes[128];
    int64_t last_stream_time_us = 0;
    while (true) {
        int count = uart_read_bytes(RADAR_UART, bytes, sizeof(bytes),
                                    pdMS_TO_TICKS(100));
        if (count > 0) {
            node_diagnostics_add_uart_bytes((uint32_t)count);
        }
        for (int index = 0; index < count; ++index) {
            ld2450_frame_t frame;
            if (ld2450_parser_push(&parser, bytes[index], &frame)) {
                node_diagnostics_record_radar_frame();
                int64_t now_us = esp_timer_get_time();
                if (last_stream_time_us == 0 ||
                    now_us - last_stream_time_us >= RADAR_STREAM_INTERVAL_US) {
                    bool sent = measurement_stream_send(stream, &frame, now_us);
                    node_diagnostics_record_udp_send(sent);
                    last_stream_time_us = now_us;
                }
            }
        }
    }
}

void app_main(void)
{
    esp_err_t error = nvs_flash_init();
    if (error == ESP_ERR_NVS_NO_FREE_PAGES || error == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_ERROR_CHECK(nvs_flash_erase());
        error = nvs_flash_init();
    }
    ESP_ERROR_CHECK(error);
    if (!app_config_load(&s_config)) {
        ESP_LOGE(TAG, "WiFi SSID and collector host must be configured");
        return;
    }
    ESP_LOGI(TAG, "Starting %s in %s mode", s_config.node_id,
             measurement_mode_name(s_config.mode));
    wifi_start();
    time_sync_start();
    web_server_start(&s_config);
    xTaskCreate(radar_task, "ld2450", 6144, NULL, 6, NULL);
    ESP_ERROR_CHECK(esp_ota_mark_app_valid_cancel_rollback());
}
