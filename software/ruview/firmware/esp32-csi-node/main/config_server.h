/**
 * @file config_server.h
 * @brief Development HTTP endpoint for updating CSI node NVS settings.
 */

#ifndef CONFIG_SERVER_H
#define CONFIG_SERVER_H

#include "esp_err.h"
#include "esp_http_server.h"

/**
 * Register configuration endpoints on the existing OTA/WASM HTTP server.
 *
 * Endpoints:
 *   GET  /config
 *   POST /config?target_ip=192.0.2.5&reboot=1
 *
 * @param server Existing HTTP server handle.
 * @return ESP_OK on success.
 */
esp_err_t config_server_register(httpd_handle_t server);

#endif /* CONFIG_SERVER_H */
