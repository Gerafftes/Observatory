lazy import copy
lazy import json
lazy import struct
lazy import unittest

lazy from project_tools import mmwave_freshness_packets as packets


class PacketTests(unittest.TestCase):
    devices = {"MMWAVE2": {"ip": "192.168.4.8", "mac": "02:00:00:00:00:08"}}
    host = "192.168.4.9"

    def data(self):
        data = {"src_ip": "192.168.4.8", "dst_ip": self.host, "src_port": 49000,
                "dst_port": 55010, "src_mac": "020000000008", "dst_mac": "020000000009",
                "timestamp_ns": 1_000_000_000, "payload": json.dumps({
                    "schema": "ruview.mmwave.ld2450.v1", "node_id": "MMWAVE2",
                    "boot_id": 123, "sequence": 456}).encode()}
        return data | packets.radar_identity(data, self.devices, self.host)

    def ack(self, timestamp_ns=1_001_000_000):
        ack = {"src_ip": self.host, "dst_ip": "192.168.4.8", "src_port": 55010,
               "dst_port": 49000, "src_mac": "020000000009", "dst_mac": "020000000008",
               "timestamp_ns": timestamp_ns, "payload": struct.pack("!III", 0x5256414B, 123, 456)}
        return ack | packets.radar_identity(ack, self.devices, self.host)

    def test_ack_requires_own_boot_sequence_reverse_ports_and_identity(self):
        data = self.data()
        ack = {"src_ip": self.host, "dst_ip": "192.168.4.8", "src_port": 55010,
               "dst_port": 49000, "src_mac": "020000000009", "dst_mac": "020000000008",
               "timestamp_ns": 1_001_000_000, "payload": struct.pack("!III", 0x5256414B, 123, 456)}
        ack |= packets.radar_identity(ack, self.devices, self.host)
        self.assertTrue(packets.matching_ack(data, ack))
        for change in ({"boot_id": 124}, {"sequence": 457}, {"node_id": "MMWAVE1"},
                       {"dst_port": 49001}, {"bound": False}, {"timestamp_ns": 999_999_999}):
            self.assertFalse(packets.matching_ack(data, ack | change))
        for change in ({"src_ip": self.host}, {"src_mac": "020000000001"}):
            self.assertFalse(packets.radar_identity(data | change, self.devices, self.host)["bound"])

    def test_one_late_ack_cannot_confirm_both_identical_attempts(self):
        first = self.data()
        repeat = first | {"timestamp_ns": 2_000_000_000}
        ack = self.ack(2_001_000_000)
        # The old independent predicate permits the same ACK for both packets.
        self.assertTrue(packets.matching_ack(first, ack))
        self.assertTrue(packets.matching_ack(repeat, ack))
        self.assertEqual(packets.radar_ack_attempt_pairs([first, repeat, ack]), [(1, 2)])

    def test_separate_attempt_windows_preserve_two_observed_acks(self):
        first = self.data()
        repeat = first | {"timestamp_ns": 2_000_000_000}
        rows = [first, self.ack(), repeat, self.ack(2_001_000_000)]
        self.assertEqual(packets.radar_ack_attempt_pairs(rows), [(0, 1), (2, 3)])

    def test_duplicate_or_wrong_path_ack_does_not_reuse_an_attempt(self):
        first = self.data()
        wrong = self.ack() | {"dst_port": 49001}
        ack = self.ack(1_002_000_000)
        repeated_ack = self.ack(1_003_000_000)
        self.assertEqual(packets.radar_ack_attempt_pairs([first, wrong, ack, repeated_ack]), [(0, 2)])
        for change in ({"boot_id": 124}, {"sequence": 457}, {"node_id": "MMWAVE1"},
                       {"bound": False}):
            self.assertEqual(packets.radar_ack_attempt_pairs([first, ack | change]), [])

    def test_attempt_pairs_keep_capture_order_including_equal_timestamps(self):
        first = self.data()
        ack = self.ack(first["timestamp_ns"])
        self.assertEqual(packets.radar_ack_attempt_pairs([ack, first]), [])
        self.assertEqual(packets.radar_ack_attempt_pairs([first, ack]), [(0, 1)])
        with self.assertRaisesRegex(ValueError, "capture timestamp order"):
            packets.radar_ack_attempt_pairs([self.ack(), first])

    def test_integer_packet_timestamp_and_strict_truncation(self):
        payload = b"payload"
        udp = struct.pack("!HHHH", 40000, 55010, 8 + len(payload), 0) + payload
        ip = bytearray(20);ip[0] = 0x45;ip[9] = 17
        struct.pack_into("!H", ip, 2, 20 + len(udp))
        ip[12:16] = bytes([192,168,4,8]);ip[16:20] = bytes([192,168,4,9])
        frame = bytes.fromhex("0200000000090200000000080800") + ip + udp
        header = b"\xd4\xc3\xb2\xa1" + struct.pack("<HHIIII", 2,4,0,0,65535,1)
        raw = header + struct.pack("<IIII", 10,123456,len(frame),len(frame)) + frame
        row = packets.udp_packets(raw)[0]
        self.assertEqual(row["timestamp_ns"], 10_123_456_000)
        self.assertEqual(row["payload"], payload)
        with self.assertRaises(ValueError):packets.udp_packets(raw[:-1])
        invalid = bytearray(raw);struct.pack_into("!H", invalid, 24+16+14+6, 0x2000)
        with self.assertRaises(ValueError):packets.udp_packets(bytes(invalid))

    def test_csi_grid_and_identity_do_not_claim_server_acceptance(self):
        for carriers in (64,128,192):
            header = bytearray(20);struct.pack_into("<I",header,0,0xC5110001)
            header[4:6] = bytes([4,1]);struct.pack_into("<H",header,6,carriers)
            struct.pack_into("<I",header,8,2437)
            frame = bytes(header) + bytes(carriers*2) + struct.pack("<IBBH",0xC511A060,1,7,40)
            frame += bytes.fromhex("ab"*32) + struct.pack("<IBBHQ",0xC511A061,2,1,24,42) + bytes(8)
            result = packets.csi_header(frame,"2a","ab"*32)
            self.assertTrue(result["header_and_trailers_match"])
            self.assertEqual(result["fixed_grid_eligible"],carriers==128)
            self.assertIsNone(result["server_accepted"])
            self.assertFalse(packets.csi_header(frame,"2b","ab"*32)["header_and_trailers_match"])
            self.assertFalse(packets.csi_header(frame,"2a","ac"*32)["header_and_trailers_match"])


if __name__ == "__main__":unittest.main()
