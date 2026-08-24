/**
 * Host contract tests for the ADR-018 runtime TX source-binding trailer.
 */

#include "esp_stubs.h"
#include "csi_collector.h"
#include "nvs_config.h"

#include <stdio.h>
#include <string.h>

nvs_config_t g_nvs_config;

static int s_failures = 0;

#define CHECK(condition, message) do {                                      \
        if (!(condition)) {                                                 \
            fprintf(stderr, "FAIL: %s (line %d)\n", message, __LINE__);     \
            s_failures++;                                                   \
        }                                                                   \
    } while (0)

static uint16_t read_u16_le(const uint8_t *bytes)
{
    return (uint16_t)bytes[0] | ((uint16_t)bytes[1] << 8);
}

static uint32_t read_u32_le(const uint8_t *bytes)
{
    return (uint32_t)bytes[0]
        | ((uint32_t)bytes[1] << 8)
        | ((uint32_t)bytes[2] << 16)
        | ((uint32_t)bytes[3] << 24);
}

static wifi_csi_info_t test_info(const uint8_t source_mac[6], int8_t *iq)
{
    wifi_csi_info_t info;
    memset(&info, 0, sizeof(info));
    memcpy(info.mac, source_mac, sizeof(info.mac));
    info.buf = iq;
    info.len = 4;
    info.rx_ctrl.channel = 6;
    info.rx_ctrl.rssi = -48;
    info.rx_ctrl.noise_floor = -92;
    info.rx_ctrl.sig_mode = 1;
    return info;
}

static const uint8_t TEST_FILTER_MAC[6] = {
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
};

static const uint8_t TEST_FILTER_SHA256[32] = {
    0x48, 0xf4, 0x63, 0x4d, 0x10, 0x02, 0xf9, 0xf3,
    0xc7, 0x57, 0x0c, 0xb4, 0x3e, 0x00, 0xdd, 0x86,
    0x9b, 0x22, 0xc7, 0x95, 0x38, 0xe9, 0xb4, 0xad,
    0xc7, 0xe4, 0x02, 0xde, 0x11, 0x89, 0xcf, 0xe1,
};

static void configure_filter(bool enabled)
{
    memset(&g_nvs_config, 0, sizeof(g_nvs_config));
    g_nvs_config.filter_mac_set = enabled ? 1 : 0;
    if (enabled) {
        memcpy(g_nvs_config.filter_mac, TEST_FILTER_MAC,
               sizeof(g_nvs_config.filter_mac));
    }
    csi_collector_set_node_id(3);
}

static void test_matching_source_binding(void)
{
    int8_t iq[4] = {-2, 3, 4, -5};
    uint8_t packet[CSI_MAX_FRAME_SIZE];
    configure_filter(true);
    wifi_csi_info_t info = test_info(TEST_FILTER_MAC, iq);

    size_t packet_len = csi_serialize_frame(&info, packet, sizeof(packet));
    const size_t iq_end = CSI_HEADER_SIZE + sizeof(iq);
    const uint8_t *binding = &packet[iq_end];

    CHECK(packet_len == iq_end + CSI_SOURCE_BINDING_SIZE,
          "serialized size includes the 40-byte source-binding trailer");
    CHECK(packet[4] == 3, "early-captured node ID is serialized");
    CHECK(memcmp(&packet[CSI_HEADER_SIZE], iq, sizeof(iq)) == 0,
          "I/Q bytes remain unchanged before the trailer");
    CHECK(read_u32_le(&binding[0]) == CSI_SOURCE_BINDING_MAGIC,
          "source-binding magic matches");
    CHECK(binding[4] == CSI_SOURCE_BINDING_VERSION,
          "source-binding version matches");
    CHECK(binding[5] == CSI_SOURCE_BINDING_FLAGS_MASK,
          "matching filtered source sets all binding flags");
    CHECK(read_u16_le(&binding[6]) == CSI_SOURCE_BINDING_SIZE,
          "source-binding length matches");
    CHECK(memcmp(&binding[8], TEST_FILTER_SHA256,
                 sizeof(TEST_FILTER_SHA256)) == 0,
          "filter identity uses SHA-256 of exactly six binary MAC bytes");
}

static void test_mismatched_source_is_not_attested_as_matched(void)
{
    int8_t iq[4] = {1, 2, 3, 4};
    uint8_t packet[CSI_MAX_FRAME_SIZE];
    uint8_t other_source[6];
    memcpy(other_source, TEST_FILTER_MAC, sizeof(other_source));
    other_source[5] ^= 0xff;
    configure_filter(true);
    wifi_csi_info_t info = test_info(other_source, iq);

    size_t packet_len = csi_serialize_frame(&info, packet, sizeof(packet));
    const uint8_t *binding = &packet[CSI_HEADER_SIZE + sizeof(iq)];

    CHECK(packet_len == CSI_HEADER_SIZE + sizeof(iq) + CSI_SOURCE_BINDING_SIZE,
          "mismatched source still serializes safely for direct contract testing");
    CHECK(binding[5]
              == (CSI_SOURCE_BINDING_FLAG_FILTER_ENFORCED
                  | CSI_SOURCE_BINDING_FLAG_IDENTITY_VALID),
          "mismatched source never sets SOURCE_MATCHED");
    CHECK(memcmp(&binding[8], TEST_FILTER_SHA256,
                 sizeof(TEST_FILTER_SHA256)) == 0,
          "mismatch still identifies the configured runtime filter");
}

static void test_unfiltered_binding_is_explicit_and_zeroed(void)
{
    int8_t iq[4] = {1, -1, 2, -2};
    uint8_t packet[CSI_MAX_FRAME_SIZE];
    configure_filter(false);
    wifi_csi_info_t info = test_info(TEST_FILTER_MAC, iq);

    size_t packet_len = csi_serialize_frame(&info, packet, sizeof(packet));
    const uint8_t *binding = &packet[CSI_HEADER_SIZE + sizeof(iq)];
    uint8_t zeros[32] = {0};

    CHECK(packet_len == CSI_HEADER_SIZE + sizeof(iq) + CSI_SOURCE_BINDING_SIZE,
          "unfiltered frames still carry the fixed-size trailer");
    CHECK(binding[5] == 0, "unfiltered frame has no source-binding claims");
    CHECK(memcmp(&binding[8], zeros, sizeof(zeros)) == 0,
          "unfiltered frame carries no stale filter digest");
}

static void test_trailer_capacity_is_required(void)
{
    int8_t iq[4] = {0};
    uint8_t too_small[CSI_HEADER_SIZE + sizeof(iq)
                      + CSI_SOURCE_BINDING_SIZE - 1];
    configure_filter(true);
    wifi_csi_info_t info = test_info(TEST_FILTER_MAC, iq);

    CHECK(csi_serialize_frame(&info, too_small, sizeof(too_small)) == 0,
          "serializer rejects a buffer one byte short of the trailer");
}

int main(void)
{
    test_matching_source_binding();
    test_mismatched_source_is_not_attested_as_matched();
    test_unfiltered_binding_is_explicit_and_zeroed();
    test_trailer_capacity_is_required();

    if (s_failures != 0) {
        fprintf(stderr, "%d source-binding test(s) failed\n", s_failures);
        return 1;
    }
    printf("CSI source-binding trailer tests passed\n");
    return 0;
}
