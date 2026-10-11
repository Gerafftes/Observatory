#ifndef IDF_MOCK_H
#define IDF_MOCK_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <arpa/inet.h>
typedef int esp_err_t;
#define ESP_OK 0
#define ESP_FAIL -1
#define ESP_ERR_INVALID_ARG 0x102
#define ESP_ERR_INVALID_STATE 0x103
#define ESP_ERR_NO_MEM 0x101
#define ESP_ERR_NOT_FOUND 0x105
#define ESP_ERR_NVS_NO_FREE_PAGES 5
#define ESP_ERR_NVS_NEW_VERSION_FOUND 6
#define ESP_ERR_ESP_NETIF_DHCP_ALREADY_STOPPED 7
#define ESP_ERR_TIMEOUT 0x107
#define CONFIG_TX_PROTECT_ORIGINAL_APP 1
void mock_log(const char *format, ...);
#define ESP_LOGE(tag, ...) ((void)(tag), mock_log(__VA_ARGS__))
#define ESP_LOGI(tag, ...) ((void)(tag), mock_log(__VA_ARGS__))
#define ESP_LOGW(tag, ...) ((void)(tag), mock_log(__VA_ARGS__))
#define ESP_EARLY_LOGI(tag, ...) ESP_LOGI(tag, __VA_ARGS__)
#define ESP_PARTITION_TYPE_DATA 1
#define ESP_PARTITION_SUBTYPE_DATA_NVS 2
#define ESP_PARTITION_SUBTYPE_APP_OTA_0 16
typedef struct { uint32_t address,size; bool readonly; int subtype; char label[17]; } esp_partition_t;
const esp_partition_t *esp_partition_find_first(int,int,const char*);
esp_err_t nvs_flash_init_partition_ptr(const esp_partition_t*);
typedef int nvs_handle_t;
#define NVS_READONLY 0
esp_err_t nvs_open(const char*,int,nvs_handle_t*);
void nvs_close(nvs_handle_t);
esp_err_t nvs_get_str(nvs_handle_t,const char*,char*,size_t*);
esp_err_t nvs_get_blob(nvs_handle_t,const char*,void*,size_t*);
esp_err_t nvs_get_u8(nvs_handle_t,const char*,uint8_t*);
esp_err_t nvs_get_u16(nvs_handle_t,const char*,uint16_t*);
typedef struct { int dummy; } esp_netif_t;
typedef struct { int dummy; } esp_netif_config_t;
#define ESP_NETIF_DEFAULT_WIFI_AP() ((esp_netif_config_t){0})
typedef struct { uint32_t addr; } ip4_addr_t;
typedef struct { ip4_addr_t ip,gw,netmask; } esp_netif_ip_info_t;
typedef struct { bool enable; ip4_addr_t start_ip,end_ip; } dhcps_lease_t;
typedef int esp_netif_dhcp_status_t;
#define ESP_NETIF_DHCP_STARTED 1
#define ESP_NETIF_OP_SET 1
#define ESP_NETIF_OP_GET 2
#define ESP_NETIF_REQUESTED_IP_ADDRESS 50
typedef int wifi_second_chan_t;
typedef int wifi_bandwidth_t;
typedef int wifi_mode_t;
typedef int wifi_interface_t;
#define WIFI_BW_HT20 1
#define WIFI_BW_HT40 2
#define WIFI_SECOND_CHAN_NONE 0
#define WIFI_SECOND_CHAN_ABOVE 1
#define WIFI_SECOND_CHAN_BELOW 2
#define WIFI_MODE_NULL 0
#define WIFI_MODE_STA 1
#define WIFI_MODE_APSTA 3
typedef struct { uint8_t mac[6]; } wifi_sta_info_t;
typedef struct { wifi_sta_info_t sta[15]; int num; } wifi_sta_list_t;
#define WIFI_IF_AP 1
#define WIFI_MODE_AP 2
#define WIFI_STORAGE_RAM 1
#define WIFI_AUTH_WPA2_PSK 3
#define WIFI_COUNTRY_POLICY_MANUAL 1
typedef struct { char cc[3]; uint8_t schan,nchan; int8_t max_tx_power; int policy; } wifi_country_t;
typedef struct { struct { uint8_t ssid[32],password[64]; uint8_t ssid_len,channel,max_connection; int authmode; uint16_t beacon_interval; } ap; } wifi_config_t;
typedef struct {int dummy;} wifi_init_config_t;
#define WIFI_INIT_CONFIG_DEFAULT() ((wifi_init_config_t){0})
esp_err_t esp_netif_init(void);
esp_err_t esp_event_loop_create_default(void);
typedef const char *esp_event_base_t;
#define WIFI_EVENT "wifi"
#define ESP_EVENT_ANY_ID -1
#define WIFI_EVENT_AP_STACONNECTED 1
#define WIFI_EVENT_AP_STADISCONNECTED 2
#define WIFI_EVENT_AP_START 3
esp_err_t esp_event_handler_register(esp_event_base_t, int32_t,
                                    void (*)(void *, esp_event_base_t, int32_t, void *), void *);
esp_netif_t *esp_netif_new(const esp_netif_config_t*);
esp_err_t esp_netif_attach_wifi_ap(esp_netif_t*);
esp_err_t esp_wifi_set_default_wifi_ap_handlers(void);
esp_err_t esp_netif_dhcps_stop(esp_netif_t*);
esp_err_t esp_netif_dhcps_start(esp_netif_t*);
esp_err_t esp_netif_set_ip_info(esp_netif_t*,const esp_netif_ip_info_t*);
esp_err_t esp_netif_get_ip_info(esp_netif_t*,esp_netif_ip_info_t*);
esp_err_t esp_netif_dhcps_option(esp_netif_t*,int,int,void*,size_t);
esp_err_t esp_netif_dhcps_get_status(esp_netif_t*,esp_netif_dhcp_status_t*);
esp_err_t esp_efuse_mac_get_default(uint8_t*);
esp_err_t esp_wifi_init(const wifi_init_config_t*);
esp_err_t esp_wifi_set_storage(int);
esp_err_t esp_wifi_set_mode(int);
esp_err_t esp_wifi_get_mode(wifi_mode_t*);
esp_err_t esp_wifi_ap_get_sta_list(wifi_sta_list_t*);
esp_err_t esp_wifi_set_mac(int,const uint8_t*);
esp_err_t esp_wifi_get_mac(int,uint8_t*);
esp_err_t esp_wifi_set_config(int,const wifi_config_t*);
esp_err_t esp_wifi_get_config(int,wifi_config_t*);
esp_err_t esp_wifi_set_country(const wifi_country_t*);
esp_err_t esp_wifi_get_country(wifi_country_t*);
esp_err_t esp_wifi_set_protocol(int,uint8_t);
esp_err_t esp_wifi_get_protocol(int,uint8_t*);
esp_err_t esp_wifi_set_bandwidth(int,wifi_bandwidth_t);
esp_err_t esp_wifi_get_bandwidth(int,wifi_bandwidth_t*);
esp_err_t esp_wifi_start(void);
esp_err_t esp_wifi_stop(void);
esp_err_t esp_wifi_set_channel(uint8_t,wifi_second_chan_t);
esp_err_t esp_wifi_get_channel(uint8_t*,wifi_second_chan_t*);
esp_err_t esp_wifi_set_max_tx_power(int8_t);
esp_err_t esp_wifi_get_max_tx_power(int8_t*);
typedef struct {char version[32];} esp_app_desc_t;
const esp_app_desc_t *esp_app_get_description(void);
typedef int esp_ota_img_states_t;
enum {ESP_OTA_IMG_NEW,ESP_OTA_IMG_PENDING_VERIFY,ESP_OTA_IMG_VALID,ESP_OTA_IMG_INVALID,ESP_OTA_IMG_ABORTED,ESP_OTA_IMG_UNDEFINED};
typedef int esp_ota_handle_t;
const esp_partition_t *esp_ota_get_running_partition(void);
const esp_partition_t *esp_ota_get_next_update_partition(void*);
esp_err_t esp_ota_get_state_partition(const esp_partition_t*,esp_ota_img_states_t*);
esp_err_t esp_partition_get_sha256(const esp_partition_t*,uint8_t*);
esp_err_t esp_ota_begin(const esp_partition_t*,int,esp_ota_handle_t*);
esp_err_t esp_ota_write(esp_ota_handle_t,const void*,size_t);
esp_err_t esp_ota_abort(esp_ota_handle_t);
esp_err_t esp_ota_end(esp_ota_handle_t);
esp_err_t esp_ota_set_boot_partition(const esp_partition_t*);
bool esp_ota_check_rollback_is_possible(void);
esp_err_t esp_ota_mark_app_invalid_rollback_and_reboot(void);
esp_err_t esp_ota_mark_app_valid_cancel_rollback(void);
uint32_t esp_get_free_heap_size(void);
void esp_restart(void);
int64_t esp_timer_get_time(void);
typedef void *esp_timer_handle_t;
typedef struct {
 void (*callback)(void *); int dispatch_method; const char *name; bool skip_unhandled_events;
} esp_timer_create_args_t;
#define ESP_TIMER_TASK 0
esp_err_t esp_timer_create(const esp_timer_create_args_t*, esp_timer_handle_t*);
esp_err_t esp_timer_start_periodic(esp_timer_handle_t, uint64_t);
esp_err_t esp_timer_stop(esp_timer_handle_t);
esp_err_t esp_timer_delete(esp_timer_handle_t);
int esp_reset_reason(void);
void esp_fill_random(void*,size_t);
esp_err_t esp_task_wdt_add(void*);
esp_err_t esp_task_wdt_delete(void*);
esp_err_t esp_task_wdt_reset(void);
void vTaskDelay(unsigned);
typedef uint32_t TickType_t;
typedef void *TaskHandle_t;
typedef unsigned UBaseType_t;
UBaseType_t uxTaskGetStackHighWaterMark(TaskHandle_t);
typedef int portMUX_TYPE;
#define portMUX_INITIALIZER_UNLOCKED 0
#ifdef TX_TEST_CRITICAL_HOOK
void tx_test_critical(bool enter);
#define portENTER_CRITICAL(p) ((void)(p), tx_test_critical(true))
#define portEXIT_CRITICAL(p) ((void)(p), tx_test_critical(false))
#else
#define portENTER_CRITICAL(p) ((void)(p))
#define portEXIT_CRITICAL(p) ((void)(p))
#endif
#define pdPASS 1
#define pdTRUE 1
#define portMAX_DELAY UINT32_MAX
int xTaskCreate(void (*)(void *), const char *, unsigned, void *, unsigned, TaskHandle_t *);
void vTaskDelete(TaskHandle_t);
void vTaskSuspend(TaskHandle_t);
TickType_t xTaskGetTickCount(void);
void xTaskNotifyGive(TaskHandle_t);
uint32_t ulTaskNotifyTake(int, TickType_t);
#ifndef TX_TEST_TICK_HZ
#define TX_TEST_TICK_HZ 1000
#endif
#define configTICK_RATE_HZ TX_TEST_TICK_HZ
#define pdMS_TO_TICKS(x) ((x) * configTICK_RATE_HZ / 1000)
typedef struct {int content_len;} httpd_req_t;
typedef void *httpd_handle_t;
typedef struct {int stack_size,server_port,recv_wait_timeout,send_wait_timeout;} httpd_config_t;
#define HTTPD_DEFAULT_CONFIG() ((httpd_config_t){.stack_size=4096})
typedef struct {const char *uri; int method; esp_err_t (*handler)(httpd_req_t*);void *user_ctx;} httpd_uri_t;
#define HTTP_GET 0
#define HTTP_POST 1
#define HTTPD_500_INTERNAL_SERVER_ERROR 500
#define HTTPD_403_FORBIDDEN 403
#define HTTPD_400_BAD_REQUEST 400
#define HTTPD_408_REQ_TIMEOUT 408
#define TX_GIT_HASH "fixture"
#define TX_SOURCE_SHA "fixture-sources"
esp_err_t httpd_req_get_hdr_value_str(httpd_req_t*,const char*,char*,size_t);
esp_err_t httpd_resp_send_err(httpd_req_t*,int,const char*);
esp_err_t httpd_resp_set_type(httpd_req_t*,const char*);
esp_err_t httpd_resp_send(httpd_req_t*,const char*,int);
esp_err_t httpd_resp_send_chunk(httpd_req_t*,const char*,int);
esp_err_t httpd_resp_sendstr(httpd_req_t*,const char*);
int httpd_req_recv(httpd_req_t*,char*,size_t);
esp_err_t httpd_start(httpd_handle_t*,const httpd_config_t*);
esp_err_t httpd_stop(httpd_handle_t);
esp_err_t httpd_register_uri_handler(httpd_handle_t,const httpd_uri_t*);
#endif
