"""Exercise the standalone SEED query CLI against synthetic loopback endpoints."""

from __future__ import annotations

lazy from contextlib import contextmanager, redirect_stdout
lazy from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
lazy import importlib.util
lazy import io
lazy import json
lazy import os
lazy from pathlib import Path
lazy import ssl
lazy import sys
lazy from threading import Thread
lazy import unittest
lazy from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
lazy from scripts.tests import test_seed_csi_bridge_tls as tls_fixture  # noqa: E402

QUERY_PATH = Path(__file__).resolve().parents[2] / "examples/happiness-vector/seed_query.py"
SPEC = importlib.util.spec_from_file_location("seed_query_tls_target", QUERY_PATH)
query = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(query)
FIXTURE_TOKEN = "synthetic-query-test-value"


class SeedQueryTLSBoundaryTests(unittest.TestCase):
    make_certificate = staticmethod(tls_fixture.SeedTLSBoundaryTests.make_certificate)

    @classmethod
    def setUpClass(cls):
        tls_fixture.SeedTLSBoundaryTests.setUpClass.__func__(cls)

    def setUp(self):
        self.enterContext(patch.dict(os.environ, {"no_proxy": "*", "NO_PROXY": "*"}))

    @contextmanager
    def serve(self, identity=None, *, redirect_to=None, status=200, response=None):
        requests = []
        payload = json.dumps({"ok": True} if response is None else response).encode()

        class Handler(BaseHTTPRequestHandler):
            def handle_request(self):
                raw = self.rfile.read(int(self.headers.get("Content-Length", 0)))
                authenticated = self.headers.get("Authorization") == f"Bearer {FIXTURE_TOKEN}"
                requests.append((self.command, self.path, authenticated,
                                 json.loads(raw) if raw else None))
                self.send_response(status)
                if redirect_to is not None:
                    self.send_header("Location", redirect_to)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(payload)

            do_GET = do_POST = handle_request

            def log_message(self, format, *args):
                pass

        with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
            if identity is not None:
                tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
                tls.load_cert_chain(*identity)
                server.socket = tls.wrap_socket(server.socket, server_side=True)
            worker = Thread(target=server.serve_forever, kwargs={"poll_interval": 0.05})
            worker.start()
            try:
                scheme = "https" if identity is not None else "http"
                yield f"{scheme}://127.0.0.1:{server.server_port}", requests
            finally:
                server.shutdown()
                worker.join()

    def test_http_is_rejected_before_authorization_is_sent(self):
        with self.serve() as (url, requests):
            result = query.api(url, "/api/v1/status", FIXTURE_TOKEN)
            self.assertIn("HTTPS", result["error"])
            self.assertEqual(requests, [])

    def test_explicit_trust_preserves_get_post_and_optional_auth(self):
        with self.serve(self.trusted) as (url, requests):
            for token, authenticated in ((FIXTURE_TOKEN, True), (None, False)):
                with self.subTest(authenticated=authenticated):
                    result = query.api(url + "/seed/", "/api/v1/status", token,
                                       ca_file=self.trusted[0])
                    self.assertEqual(result, {"ok": True})
                    result = query.api(url + "/seed/", "/api/v1/store/search", token,
                                       method="POST", data={"k": 3},
                                       ca_file=self.trusted[0])
                    self.assertEqual(result, {"ok": True})
            self.assertEqual(requests, [
                ("GET", "/seed/api/v1/status", True, None),
                ("POST", "/seed/api/v1/store/search", True, {"k": 3}),
                ("GET", "/seed/api/v1/status", False, None),
                ("POST", "/seed/api/v1/store/search", False, {"k": 3}),
            ])

    def test_default_trust_rejects_foreign_certificate(self):
        with self.serve(self.foreign) as (url, requests):
            result = query.api(url, "/api/v1/status", FIXTURE_TOKEN)
            self.assertIn("CERTIFICATE_VERIFY_FAILED", result["error"])
            self.assertEqual(requests, [])

    def test_explicit_trust_rejects_foreign_certificate(self):
        with self.serve(self.foreign) as (url, requests):
            result = query.api(url, "/api/v1/status", FIXTURE_TOKEN,
                               ca_file=self.trusted[0])
            self.assertIn("CERTIFICATE_VERIFY_FAILED", result["error"])
            self.assertEqual(requests, [])

    def test_trusted_certificate_with_wrong_hostname_is_rejected(self):
        with self.serve(self.wrong_name) as (url, requests):
            result = query.api(url, "/api/v1/status", FIXTURE_TOKEN,
                               ca_file=self.wrong_name[0])
            self.assertIn("CERTIFICATE_VERIFY_FAILED", result["error"])
            self.assertEqual(requests, [])

    def test_ambiguous_base_urls_are_rejected_before_tls_setup(self):
        urls = (
            "file:///tmp/seed-query", "https:///missing", "https://@127.0.0.1",
            "https://127.0.0.1:invalid", "https://127.0.0.1:65536",
            "https://127.0.0.1:0", "https://127.0.0.1:",
            "https://%31%32%37.0.0.1", "https://127.0.0.1\\@other.invalid",
            "https://127.0.0.1?", "https://127.0.0.1#", "\thttps://127.0.0.1",
            "https://127.0.0.1/with space", "https://127.0.0.1\n",
        )
        for url in urls:
            with self.subTest(url=url), patch.object(query.ssl, "create_default_context") as tls:
                result = query.api(url, "/api/v1/status", FIXTURE_TOKEN)
                self.assertIn("error", result)
                tls.assert_not_called()

    def test_api_paths_cannot_replace_the_configured_authority(self):
        for path in ("@other.invalid/api", "//other.invalid/api", "https://other.invalid/api"):
            with self.subTest(path=path), patch.object(query.ssl, "create_default_context") as tls:
                result = query.api("https://127.0.0.1", path, FIXTURE_TOKEN)
                self.assertIn("error", result)
                tls.assert_not_called()

    def test_empty_ca_file_is_rejected_before_tls_setup(self):
        with patch.object(query.ssl, "create_default_context") as tls:
            result = query.api("https://127.0.0.1", "/status", FIXTURE_TOKEN, ca_file="")
            self.assertIn("CA file must not be empty", result["error"])
            tls.assert_not_called()

    def test_redirects_never_reach_http_or_other_https_endpoint(self):
        for identity in (None, self.trusted):
            with self.serve(identity) as (target, target_requests):
                for status in (301, 302, 303, 307, 308):
                    for method in ("GET", "POST"):
                        with self.subTest(https_target=identity is not None,
                                          status=status, method=method):
                            with self.serve(self.trusted, redirect_to=target + "/other",
                                            status=status) as (url, requests):
                                result = query.api(url, "/api/v1/store/search", FIXTURE_TOKEN,
                                                   method=method, ca_file=self.trusted[0])
                                self.assertEqual(result["error"], f"HTTP {status}")
                                self.assertEqual(len(requests), 1)
                                self.assertTrue(requests[0][2])
                            self.assertEqual(target_requests, [])

    def test_relative_redirects_are_also_rejected(self):
        for status in (301, 302, 303, 307, 308):
            with self.subTest(status=status):
                with self.serve(self.trusted, redirect_to="/other", status=status) as (url, requests):
                    result = query.api(url, "/status", FIXTURE_TOKEN, ca_file=self.trusted[0])
                    self.assertEqual(result["error"], f"HTTP {status}")
                    self.assertEqual(len(requests), 1)

    def test_http_error_dictionary_is_preserved(self):
        with self.serve(self.trusted, status=403, response={"denied": True}) as (url, requests):
            result = query.api(url, "/status", FIXTURE_TOKEN, ca_file=self.trusted[0])
            self.assertEqual(result, {"error": "HTTP 403", "detail": '{"denied": true}'})
            self.assertEqual(len(requests), 1)

    def test_every_cli_command_uses_private_trust(self):
        response = {
            "device_id": "synthetic-device", "total_vectors": 0, "epoch": 1,
            "dimension": 8, "file_size_bytes": 0, "uptime_secs": 1,
            "witness_chain_length": 0,
        }
        for command in ([], ["status"], ["search"], ["witness"], ["monitor"], ["report"]):
            with self.subTest(command=command):
                with self.serve(self.trusted, response=response) as (url, requests):
                    argv = [str(QUERY_PATH), "--seed", url, "--seed-ca-file", self.trusted[0],
                            "--token", FIXTURE_TOKEN, *command]
                    output = io.StringIO()
                    with patch.object(sys, "argv", argv), redirect_stdout(output):
                        with patch.object(query.time, "sleep", side_effect=KeyboardInterrupt):
                            query.main()
                    self.assertNotIn("Error:", output.getvalue())
                    self.assertTrue(requests)
                    self.assertTrue(all(request[2] for request in requests))


if __name__ == "__main__":
    unittest.main()
