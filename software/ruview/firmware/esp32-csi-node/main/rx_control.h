#ifndef RX_CONTROL_H
#define RX_CONTROL_H
#include <stdbool.h>
#include "esp_http_server.h"

esp_err_t rx_control_register(httpd_handle_t server);
/* HTTP handlers run on the same server task. A bounded host lease protects
 * a measurement block from config writes, restart and OTA. */
bool rx_control_measurement_active(void);
esp_err_t rx_wifi_start(bool *connected);
#endif
