"""Focused source-level checks for the ESP32 security boundaries.

The full ESP-IDF build is the owning validation gate. These checks provide a
fast regression signal on hosts without the ESP-IDF toolchain: each protected
handler must authenticate before reading input or changing device state, and
RVF verification must call real Ed25519 verification.
"""

from __future__ import annotations

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]


def function_region(source: str, signature: str) -> str:
    start = source.index(signature)
    next_function = source.find("\nstatic ", start + len(signature))
    return source[start:] if next_function < 0 else source[start:next_function]


class Esp32SecurityBoundaryTests(unittest.TestCase):
    def test_config_handlers_authenticate_before_nvs_access(self) -> None:
        source = (ROOT / "main/config_server.c").read_text()
        get_handler = function_region(source, "static esp_err_t config_get_handler")
        post_handler = function_region(source, "static esp_err_t config_post_handler")
        self.assertLess(
            get_handler.index("ota_check_auth(req)"),
            get_handler.index("nvs_config_load"),
        )
        self.assertLess(
            post_handler.index("ota_check_auth(req)"),
            post_handler.index("httpd_req_get_url_query_str"),
        )

    def test_all_wasm_lifecycle_handlers_authenticate_first(self) -> None:
        source = (ROOT / "main/wasm_upload.c").read_text()
        self.assertIn("if (ota_check_auth(req))", source)
        for handler, first_operation in (
            ("wasm_upload_handler", "receive_body"),
            ("wasm_list_handler", "wasm_module_info_t"),
            ("wasm_start_handler", "parse_module_id_from_uri"),
            ("wasm_stop_handler", "parse_module_id_from_uri"),
            ("wasm_delete_handler", "parse_module_id_from_uri"),
        ):
            region = function_region(source, f"static esp_err_t {handler}")
            self.assertLess(
                region.index("wasm_require_auth(req)"),
                region.index(first_operation),
                handler,
            )

    def test_ota_auth_is_shared_and_publicly_declared(self) -> None:
        header = (ROOT / "main/ota_update.h").read_text()
        source = (ROOT / "main/ota_update.c").read_text()
        self.assertIn("bool ota_check_auth(httpd_req_t *req);", header)
        self.assertIn("bool ota_check_auth(httpd_req_t *req)", source)
        self.assertNotIn("static bool ota_check_auth", source)

    def test_rvf_verification_is_real_ed25519_not_a_forgeable_digest(self) -> None:
        source = (ROOT / "main/rvf_parser.c").read_text()
        self.assertIn('#include "sodium.h"', source)
        self.assertIn("crypto_sign_ed25519_verify_detached", source)
        self.assertNotIn("SHA-256(pubkey ||", source)
        self.assertNotIn("memcmp(parsed->signature, expected, 32)", source)


if __name__ == "__main__":
    unittest.main()
