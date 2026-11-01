#pragma once
#include "esp_stubs.h"
typedef void *httpd_handle_t;
typedef struct {
    int content_len;
    const char *body;
    size_t received;
    int response_error;
    bool authenticated, fail_receive;
    bool wrong_device;
} httpd_req_t;
typedef struct {
    const char *uri;
    int method;
    esp_err_t (*handler)(httpd_req_t *);
    void *user_ctx;
} httpd_uri_t;
#define HTTP_GET 0
#define HTTP_POST 1
#define HTTPD_RESP_USE_STRLEN (-1)
#define HTTPD_400_BAD_REQUEST 400
#define HTTPD_403_FORBIDDEN 403
#define HTTPD_500_INTERNAL_SERVER_ERROR 500
#define HTTPD_SOCK_ERR_TIMEOUT (-3)
typedef struct { unsigned stack_size, server_port, max_uri_handlers, recv_wait_timeout; } httpd_config_t;
#define HTTPD_DEFAULT_CONFIG() ((httpd_config_t){0})
esp_err_t httpd_start(httpd_handle_t *server, const httpd_config_t *config);
esp_err_t httpd_stop(httpd_handle_t server);
int httpd_req_recv(httpd_req_t *req, char *buffer, size_t size);
esp_err_t httpd_resp_send_err(httpd_req_t *req, int status, const char *reason);
esp_err_t httpd_resp_send(httpd_req_t *req, const char *data, int size);
esp_err_t httpd_resp_set_type(httpd_req_t *req, const char *type);
esp_err_t httpd_register_uri_handler(httpd_handle_t server, const httpd_uri_t *uri);
esp_err_t httpd_req_get_hdr_value_str(httpd_req_t *req, const char *name, char *out, size_t size);
