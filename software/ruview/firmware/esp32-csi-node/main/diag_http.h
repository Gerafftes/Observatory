#pragma once
#include <stdbool.h>
#include "esp_http_server.h"
bool diag_http_requested(httpd_req_t *req);
esp_err_t diag_http_send(httpd_req_t *req);
