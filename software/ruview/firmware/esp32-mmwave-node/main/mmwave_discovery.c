#include "mmwave_discovery.h"

#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "cJSON.h"
#include "esp_log.h"
#include "esp_system.h"
#include "lwip/inet.h"
#include "lwip/sockets.h"
#include "mbedtls/md.h"

#define DISCOVERY_PORT 5011
#define DISCOVERY_INTERVAL_US 2000000LL
#define DISCOVERY_PACKET_CAPACITY 512
#define DISCOVERY_AUTH_HEX_LENGTH 64
#define DISCOVERY_RESPONSE_SCHEMA "ruview.mmwave.discovery.response.v1"
#define DISCOVERY_REQUEST_SCHEMA "ruview.mmwave.discovery.request.v1"

static const char *TAG = "mmwave_discovery";

struct mmwave_discovery {
    int socket_fd;
    uint32_t pending_nonce;
    int64_t next_request_us;
    bool pending;
};

static bool json_uint32(const cJSON *value, uint32_t *result)
{
    if (!cJSON_IsNumber(value) || value->valuedouble < 0.0 ||
        value->valuedouble > UINT32_MAX) {
        return false;
    }
    uint32_t converted = (uint32_t)value->valuedouble;
    if ((double)converted != value->valuedouble) {
        return false;
    }
    *result = converted;
    return true;
}

static bool hmac_sha256_hex(const char *secret, const char *message,
                            uint8_t output[32])
{
    const mbedtls_md_info_t *digest =
        mbedtls_md_info_from_type(MBEDTLS_MD_SHA256);
    return digest != NULL &&
           mbedtls_md_hmac(digest, (const unsigned char *)secret,
                           strlen(secret), (const unsigned char *)message,
                           strlen(message), output) == 0;
}

static bool constant_time_hex_matches(const char *actual,
                                      const uint8_t expected[32])
{
    if (actual == NULL || strlen(actual) != DISCOVERY_AUTH_HEX_LENGTH) {
        return false;
    }
    static const char hex[] = "0123456789abcdef";
    uint8_t difference = 0;
    for (size_t index = 0; index < 32; ++index) {
        difference |= (uint8_t)(actual[index * 2] != hex[expected[index] >> 4]);
        difference |= (uint8_t)(actual[index * 2 + 1] !=
                                hex[expected[index] & 0x0f]);
    }
    return difference == 0;
}

static bool response_auth_valid(const app_config_t *config,
                                uint32_t nonce, uint16_t collector_port,
                                const char *auth)
{
    char message[160];
    int length = snprintf(message, sizeof(message),
                           "%s\n%s\n%" PRIu32 "\n%u",
                           DISCOVERY_RESPONSE_SCHEMA, config->node_id, nonce,
                           (unsigned)collector_port);
    if (length < 0 || (size_t)length >= sizeof(message)) {
        return false;
    }
    uint8_t expected[32];
    return hmac_sha256_hex(config->ota_psk, message, expected) &&
           constant_time_hex_matches(auth, expected);
}

static bool send_request(mmwave_discovery_t *discovery,
                         const app_config_t *config,
                         int64_t monotonic_time_us)
{
    char request[DISCOVERY_PACKET_CAPACITY];
    uint32_t nonce = esp_random();
    int length = snprintf(request, sizeof(request),
                          "{\"schema\":\"%s\",\"node_id\":\"%s\","
                          "\"nonce\":%" PRIu32 "}",
                          DISCOVERY_REQUEST_SCHEMA, config->node_id, nonce);
    if (length < 0 || (size_t)length >= sizeof(request)) {
        return false;
    }
    struct sockaddr_in destination = {
        .sin_family = AF_INET,
        .sin_port = htons(DISCOVERY_PORT),
        .sin_addr.s_addr = htonl(INADDR_BROADCAST),
    };
    if (sendto(discovery->socket_fd, request, (size_t)length, 0,
               (struct sockaddr *)&destination, sizeof(destination)) < 0) {
        return false;
    }
    discovery->pending_nonce = nonce;
    discovery->pending = true;
    discovery->next_request_us = monotonic_time_us + DISCOVERY_INTERVAL_US;
    return true;
}

static bool apply_response(mmwave_discovery_t *discovery,
                           app_config_t *config)
{
    uint8_t packet[DISCOVERY_PACKET_CAPACITY];
    struct sockaddr_in peer = {0};
    socklen_t peer_length = sizeof(peer);
    int length = recvfrom(discovery->socket_fd, packet, sizeof(packet) - 1,
                          0, (struct sockaddr *)&peer, &peer_length);
    if (length < 0) {
        return false;
    }
    packet[length] = '\0';
    cJSON *json = cJSON_ParseWithLength((const char *)packet, (size_t)length);
    if (json == NULL) {
        return false;
    }
    const cJSON *schema = cJSON_GetObjectItemCaseSensitive(json, "schema");
    const cJSON *node_id = cJSON_GetObjectItemCaseSensitive(json, "node_id");
    const cJSON *nonce_value = cJSON_GetObjectItemCaseSensitive(json, "nonce");
    const cJSON *collector_port_value =
        cJSON_GetObjectItemCaseSensitive(json, "collector_port");
    const cJSON *auth = cJSON_GetObjectItemCaseSensitive(json, "auth");
    uint32_t nonce = 0;
    uint32_t collector_port_number = 0;
    bool valid = cJSON_IsString(schema) && cJSON_IsString(node_id) &&
                 cJSON_IsString(auth) && node_id->valuestring != NULL &&
                 strcmp(schema->valuestring, DISCOVERY_RESPONSE_SCHEMA) == 0 &&
                 strcmp(node_id->valuestring, config->node_id) == 0 &&
                 json_uint32(nonce_value, &nonce) &&
                 nonce == discovery->pending_nonce &&
                 json_uint32(collector_port_value, &collector_port_number) &&
                 collector_port_number > 0 && collector_port_number <= UINT16_MAX &&
                 response_auth_valid(config, nonce, (uint16_t)collector_port_number,
                                     auth->valuestring);
    if (!valid || peer.sin_family != AF_INET) {
        cJSON_Delete(json);
        return false;
    }

    char collector_host[16];
    if (inet_ntop(AF_INET, &peer.sin_addr, collector_host,
                  sizeof(collector_host)) == NULL) {
        cJSON_Delete(json);
        return false;
    }
    app_config_t current;
    app_config_snapshot(config, &current);
    bool changed = strcmp(current.target_host, collector_host) != 0 ||
                   current.target_port != (uint16_t)collector_port_number;
    bool saved = !changed ||
                 app_config_set_transport(config, collector_host,
                                          (uint16_t)collector_port_number);
    cJSON_Delete(json);
    if (!saved) {
        ESP_LOGW(TAG, "Authenticated collector discovery could not be persisted");
        return false;
    }
    discovery->pending = false;
    if (changed) {
        ESP_LOGI(TAG, "Authenticated collector discovered at %s:%u",
                 collector_host, (unsigned)collector_port_number);
    }
    return true;
}

mmwave_discovery_t *mmwave_discovery_create(void)
{
    mmwave_discovery_t *discovery = calloc(1, sizeof(*discovery));
    if (discovery == NULL) {
        return NULL;
    }
    discovery->socket_fd = socket(AF_INET, SOCK_DGRAM, IPPROTO_IP);
    if (discovery->socket_fd < 0) {
        free(discovery);
        return NULL;
    }
    int broadcast = 1;
    if (setsockopt(discovery->socket_fd, SOL_SOCKET, SO_BROADCAST,
                   &broadcast, sizeof(broadcast)) < 0) {
        close(discovery->socket_fd);
        free(discovery);
        return NULL;
    }
    int flags = fcntl(discovery->socket_fd, F_GETFL, 0);
    if (flags < 0 || fcntl(discovery->socket_fd, F_SETFL, flags | O_NONBLOCK) < 0) {
        close(discovery->socket_fd);
        free(discovery);
        return NULL;
    }
    return discovery;
}

void mmwave_discovery_destroy(mmwave_discovery_t *discovery)
{
    if (discovery == NULL) {
        return;
    }
    if (discovery->socket_fd >= 0) {
        close(discovery->socket_fd);
    }
    free(discovery);
}

void mmwave_discovery_poll(mmwave_discovery_t *discovery,
                           app_config_t *config,
                           int64_t monotonic_time_us)
{
    if (discovery == NULL || config == NULL || config->ota_psk[0] == '\0') {
        return;
    }
    if (monotonic_time_us >= discovery->next_request_us) {
        if (!send_request(discovery, config, monotonic_time_us)) {
            discovery->next_request_us = monotonic_time_us + DISCOVERY_INTERVAL_US;
        }
    }
    while (discovery->pending) {
        if (!apply_response(discovery, config)) {
            if (errno == EAGAIN || errno == EWOULDBLOCK) {
                break;
            }
            break;
        }
    }
}
