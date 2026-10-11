"""Host-only checks of both real ESP-NOW receive callback variants."""
from __future__ import annotations

lazy import subprocess
lazy import tempfile
lazy import unittest
lazy from pathlib import Path


class CallbackCompatibilityTests(unittest.TestCase):
    def test_legacy_and_current_idf_callbacks_keep_packet_and_null_checks(self):
        root = Path(__file__).resolve().parents[2]
        source = (root / 'software/ruview/firmware/esp32-csi-node/main/c6_sync_espnow.c').read_text()
        mac = source[source.index('static uint64_t mac6_to_u64('):source.index('static void send_beacon(')]
        receive = source[source.index('static void on_recv_data('):source.index('/* Issue #944:')]
        harness = r'''
#include <assert.h>
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#define ESP_IDF_VERSION_VAL(a,b,c) ((a)*10000+(b)*100+(c))
#define ESP_LOGI(...) ((void)0)
#define BEACON_MAGIC 0x53454E50u
#define BEACON_PROTO_VER 1
#define OFFSET_EMA_SHIFT 3
typedef struct { uint32_t magic; uint8_t proto_ver, leader_flag; uint16_t reserved; uint64_t leader_epoch_us; } espnow_beacon_t;
typedef struct { const uint8_t *src_addr; } esp_now_recv_info_t;
static uint32_t s_rx_count, s_rx_magic_match;
static uint64_t s_leader_id, s_local_id = 10, s_last_seen_us;
static bool s_is_leader = true, s_smoothed_seeded;
static int64_t s_offset_us, s_offset_us_smoothed;
static uint64_t esp_timer_get_time(void) { return 1000; }
'''
        checks = r'''
static void receive(const uint8_t *mac, const uint8_t *data, int len) {
#if ESP_IDF_VERSION >= ESP_IDF_VERSION_VAL(5, 0, 0)
    esp_now_recv_info_t info = { .src_addr = mac };
    on_recv(mac ? &info : NULL, data, len);
#else
    on_recv(mac, data, len);
#endif
}
int main(void) {
    const uint8_t mac[6] = {0,0,0,0,0,1};
    espnow_beacon_t beacon = { .magic = BEACON_MAGIC, .proto_ver = 1, .leader_flag = 1, .leader_epoch_us = 1500 };
    receive(mac, NULL, sizeof(beacon));
    receive(mac, (uint8_t *)&beacon, sizeof(beacon)-1);
    beacon.proto_ver = 2;
    receive(mac, (uint8_t *)&beacon, sizeof(beacon));
    assert(s_rx_count == 3 && s_rx_magic_match == 0 && s_is_leader);
    beacon.proto_ver = 1;
    receive(mac, (uint8_t *)&beacon, sizeof(beacon));
    assert(s_rx_count == 4 && s_rx_magic_match == 1 && !s_is_leader);
    assert(s_leader_id == 1 && s_offset_us == 500 && s_offset_us_smoothed == 500);
    assert(s_last_seen_us == 1000 && s_smoothed_seeded);
    beacon.leader_epoch_us = 1580;
    receive(mac, (uint8_t *)&beacon, sizeof(beacon));
    assert(s_offset_us == 580 && s_offset_us_smoothed == 510);
    receive(NULL, (uint8_t *)&beacon, sizeof(beacon));
    assert(s_rx_count == 6 && s_rx_magic_match == 3 && s_leader_id == 0);
    return 0;
}
'''
        with tempfile.TemporaryDirectory() as temporary:
            for version in (40400, 50000, 50500, 60000):
                with self.subTest(idf=version):
                    executable = Path(temporary) / f'callback-{version}'
                    subprocess.run(['cc', '-x', 'c', '-std=c99', '-Wall', '-Wextra', '-Werror',
                                    f'-DESP_IDF_VERSION={version}', '-o', str(executable), '-'],
                                   input=harness + mac + receive + checks, text=True, check=True)
                    subprocess.run([str(executable)], check=True)


if __name__ == '__main__':
    unittest.main()
