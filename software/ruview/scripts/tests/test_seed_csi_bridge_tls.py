"""Exercise the SEED client's TLS boundary using only loopback servers."""

from __future__ import annotations

lazy from contextlib import contextmanager
lazy from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
lazy import os
lazy from pathlib import Path
lazy import ssl
lazy import subprocess
lazy import sys
lazy import tempfile
lazy from threading import Thread
lazy import unittest
lazy from unittest.mock import patch
lazy from urllib.error import HTTPError, URLError

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
lazy from scripts.seed_csi_bridge import SeedClient  # noqa: E402


FIXTURE_TOKEN = "loopback-test-value"


class SeedTLSBoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        root = Path(cls.enterClassContext(tempfile.TemporaryDirectory()))
        cls.trusted = cls.make_certificate(root, "trusted", "IP:127.0.0.1")
        cls.foreign = cls.make_certificate(root, "foreign", "IP:127.0.0.1")
        cls.wrong_name = cls.make_certificate(root, "wrong-name", "DNS:other.invalid")

    @staticmethod
    def make_certificate(root, name, san):
        cert = root / f"{name}.pem"
        key = root / f"{name}.key"
        subprocess.run(
            ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
             "-subj", f"/CN={name}", "-addext", f"subjectAltName={san}",
             "-keyout", str(key), "-out", str(cert)],
            check=True, capture_output=True,
        )
        return str(cert), str(key)

    def setUp(self):
        self.enterContext(patch.dict(os.environ, {"no_proxy": "*", "NO_PROXY": "*"}))

    @contextmanager
    def serve(self, identity=None, redirect_to=None):
        requests = []

        class Handler(BaseHTTPRequestHandler):
            def handle_request(self):
                self.rfile.read(int(self.headers.get("Content-Length", 0)))
                requests.append((self.command, self.path, "Authorization" in self.headers))
                if redirect_to:
                    self.send_response(302)
                    self.send_header("Location", redirect_to)
                    self.end_headers()
                    return
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(b'{"ok": true}')

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

    def test_default_trust_rejects_foreign_certificate(self):
        with self.serve(self.foreign) as (url, requests):
            with self.assertRaises(URLError) as failure:
                SeedClient(url, FIXTURE_TOKEN).ingest([])
            self.assertIsInstance(failure.exception.reason, ssl.SSLCertVerificationError)
            self.assertEqual(requests, [])

    def test_explicit_trust_accepts_matching_certificate_and_preserves_requests(self):
        with self.serve(self.trusted) as (url, requests):
            client = SeedClient(url + "/seed/", FIXTURE_TOKEN, ca_file=self.trusted[0])
            self.assertEqual(client.status(), {"ok": True})
            self.assertEqual(client.ingest([]), {"ok": True})
            self.assertEqual(requests, [
                ("GET", "/seed/api/v1/status", False),
                ("POST", "/seed/api/v1/store/ingest", True),
            ])

    def test_explicit_trust_rejects_foreign_certificate(self):
        with self.serve(self.foreign) as (url, requests):
            with self.assertRaises(URLError) as failure:
                SeedClient(url, FIXTURE_TOKEN, ca_file=self.trusted[0]).ingest([])
            self.assertIsInstance(failure.exception.reason, ssl.SSLCertVerificationError)
            self.assertEqual(requests, [])

    def test_trusted_certificate_with_wrong_host_is_rejected(self):
        with self.serve(self.wrong_name) as (url, requests):
            with self.assertRaises(URLError) as failure:
                SeedClient(url, FIXTURE_TOKEN, ca_file=self.wrong_name[0]).ingest([])
            self.assertIsInstance(failure.exception.reason, ssl.SSLCertVerificationError)
            self.assertEqual(requests, [])

    def test_rejects_http_and_ambiguous_base_urls(self):
        for url in ("http://127.0.0.1", "https:///missing", "https://@127.0.0.1",
                    "https://127.0.0.1:invalid", "https://%31%32%37.0.0.1",
                    "https://127.0.0.1?query", "https://127.0.0.1#fragment"):
            with self.subTest(url=url), self.assertRaises(ValueError):
                SeedClient(url, FIXTURE_TOKEN)

    def test_empty_ca_file_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "SEED CA file must not be empty"):
            SeedClient("https://127.0.0.1", FIXTURE_TOKEN, ca_file="")

    def test_authenticated_redirect_does_not_reach_other_endpoint(self):
        for identity in (None, self.trusted):
            with self.subTest(secure_target=identity is not None):
                with self.serve(identity) as (target, target_requests):
                    with self.serve(self.trusted, redirect_to=target + "/redirect") as (url, requests):
                        with self.assertRaises(HTTPError) as failure:
                            SeedClient(url, FIXTURE_TOKEN, ca_file=self.trusted[0]).ingest([])
                        self.assertEqual(failure.exception.code, 302)
                        failure.exception.close()
                        self.assertEqual(requests, [("POST", "/api/v1/store/ingest", True)])
                    self.assertEqual(target_requests, [])


if __name__ == "__main__":
    unittest.main()
