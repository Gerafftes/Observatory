"""HTTP + UDP regression: saved CAD geometry survives a sensing-server restart.

Build the debug sensing-server first. Uses only temporary data and loopback ports.
"""
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "v2/target/debug/sensing-server"


def free_port(kind=socket.SOCK_STREAM):
    with socket.socket(socket.AF_INET, kind) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class NodeStatus(BaseHTTPRequestHandler):
    def do_GET(self):
        body = json.dumps({"node_id": "TEST-RADAR", "sensor": "HLK-LD2450",
                           "target": "192.168.4.50:5010"}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


@unittest.skipUnless(BINARY.exists(), "Build the debug sensing-server first")
class CadRuntimeTest(unittest.TestCase):
    def request(self, path, method="GET", payload=None):
        data = None if payload is None else json.dumps(payload).encode()
        request = Request(f"http://127.0.0.1:{self.http}{path}", data=data,
                          method=method, headers={"Content-Type": "application/json"})
        with urlopen(request, timeout=2) as response:
            return json.load(response)

    def wait_for(self, read, predicate):
        deadline = time.monotonic() + 15
        last = None
        while time.monotonic() < deadline:
            try:
                last = read()
                if predicate(last):
                    return last
            except OSError:
                pass
            time.sleep(0.1)
        self.fail(f"Timed out waiting for server: {last}")

    def start(self, directory, node_url):
        env = dict(os.environ)
        for key in list(env):
            if key.startswith(("SENSING_", "MMWAVE_")) or key == "RUVIEW_API_TOKEN":
                del env[key]
        env["WDP_RUFIELD_SIGNING_SEED"] = secrets.token_hex(32)
        self.process = subprocess.Popen([
            str(BINARY), "--source", "simulate", "--bind-addr", "127.0.0.1",
            "--http-port", str(self.http), "--ws-port", str(self.ws),
            "--mmwave-udp-port", str(self.udp), "--mmwave-node-url", node_url,
            "--ui-path", str(ROOT / "ui"), "--no-edge-registry",
        ], cwd=directory, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.wait_for(lambda: self.request("/health/ready"), lambda result: result["status"] == "ready")

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            self.process.wait(timeout=10)

    def test_save_update_restart_and_radar_without_rx(self):
        self.http, self.ws, self.udp = free_port(), free_port(), free_port(socket.SOCK_DGRAM)
        node = ThreadingHTTPServer(("127.0.0.1", 0), NodeStatus)
        threading.Thread(target=node.serve_forever, daemon=True).start()
        self.addCleanup(node.server_close)
        self.addCleanup(node.shutdown)
        with tempfile.TemporaryDirectory(prefix="ruview-cad-test-") as directory:
            self.start(directory, f"http://127.0.0.1:{node.server_port}")
            try:
                document = {
                    "room_dimensions_m": [4.02, 2.59, 3.44],
                    "transmitter": {"id": "TX", "position_m": [1.5, 1.2, 0.4]},
                    "receivers": [{"id": f"RX{i+1}", "role": "receiver", "position_m": point}
                                  for i, point in enumerate([[0, 0.5, 0.3], [4.02, 0.9, 1], [0, 0.7, 2], [4.02, 0.9, 2.5]])],
                    "mmwave": {"sensor": "HLK-LD2450", "mounting_position_m": [1, 1.2, 0.5]},
                    "points": [{"id": f"P{i+1:02}", "coordinates_m": [1 + i % 3, 0, 0.8 + (i // 3) * 0.8]}
                               for i in range(9)],
                    "radio": {"channel": 6},
                    "environment": {"layout_revision": "test", "furniture_revision": "test", "door_state_revision": "closed"},
                }
                profile = self.request("/api/v1/experiments/setup-profiles", "POST", {"label": "Temporary CAD test", "document": document})
                status = self.request("/api/v1/mmwave/status")
                self.assertEqual(status["cad_profile"]["profile_sha256"], profile["profile_sha256"])
                document["mmwave"]["mounting_position_m"] = [2, 1.2, 1]
                profile = self.request(f'/api/v1/experiments/setup-profiles/{profile["id"]}', "PUT", {"label": "Temporary CAD test", "document": document})
                self.stop()
                self.start(directory, f"http://127.0.0.1:{node.server_port}")
                status = self.request("/api/v1/mmwave/status")
                self.assertEqual(status["cad_profile"]["profile_sha256"], profile["profile_sha256"])
                self.assertEqual(status["mounting_position_m"], [2, 1.2, 1])
                target = {"slot": 1, "present": True, "x_mm": 100, "y_mm": 1000,
                          "room_x_mm": 1000, "room_z_mm": 100, "speed_cm_s": 0, "resolution_mm": 10}
                packet = {"schema": "ruview.mmwave.ld2450.v1", "node_id": "TEST-RADAR", "mode": "calibration",
                          "boot_id": 7, "sequence": 0, "sensor_time_us": 1000, "unix_time_ms": 0,
                          "coordinate_frame": {"local": "x_right_y_forward_mm", "room": "x_length_z_width_mm",
                                               "origin_x_mm": 0, "origin_z_mm": 0, "yaw_mdeg": 0, "raw_x_inverted": False},
                          "targets": [target] + [dict(slot=i, present=False, x_mm=0, y_mm=0,
                                                     room_x_mm=0, room_z_mm=0, speed_cm_s=0, resolution_mm=0)
                                                   for i in (2, 3)]}
                with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
                    sock.sendto(json.dumps(packet).encode(), ("127.0.0.1", self.udp))
                status = self.wait_for(lambda: self.request("/api/v1/mmwave/status"), lambda value: value["packets_received"] > 0)
                self.assertEqual(status["target_position_mm"], [3000, 1100])
                self.assertFalse(status["setup_sealed"])
                self.assertFalse(status["preflight"]["ready"])
                status = self.wait_for(lambda: self.request("/api/v1/mmwave/status"), lambda value: value["connection"]["reachable"])
                self.assertTrue(status["node_control"]["reachable"])
                self.assertIsNone(status["radar_frames_valid"])
                self.assertIn("192.168.4.50:5010", status["connection"]["hint"])
            finally:
                self.stop()


if __name__ == "__main__":
    unittest.main()
