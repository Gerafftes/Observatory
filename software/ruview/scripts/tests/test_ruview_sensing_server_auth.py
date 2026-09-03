"""Regression tests for the standalone RuView sensing bridge auth boundary."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import os
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "ruview-sensing-server.py"
SPEC = importlib.util.spec_from_file_location("ruview_sensing_server", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RuViewSensingServerAuthTests(unittest.TestCase):
    def test_missing_or_malformed_credentials_fail_closed(self) -> None:
        self.assertFalse(MODULE.is_authorized(None, ""))
        self.assertFalse(MODULE.is_authorized("Bearer anything", ""))
        self.assertFalse(MODULE.is_authorized("Basic secret", "bridge-token"))
        self.assertFalse(MODULE.is_authorized("Bearer", "bridge-token"))
        self.assertFalse(MODULE.is_authorized("Bearer bridge-token-x", "bridge-token"))

    def test_only_the_explicit_bearer_token_is_accepted(self) -> None:
        self.assertTrue(MODULE.is_authorized("Bearer bridge-token", "bridge-token"))
        self.assertTrue(MODULE.is_authorized("bearer  bridge-token", "bridge-token"))
        self.assertFalse(MODULE.is_authorized("Bearer other-token", "bridge-token"))

    def test_handler_auth_gate_rejects_missing_and_wrong_credentials(self) -> None:
        def handler(authorization: str | None):
            instance = object.__new__(MODULE.Handler)
            instance.headers = {} if authorization is None else {
                "Authorization": authorization,
            }
            instance.send_response = mock.Mock()
            instance.send_header = mock.Mock()
            instance.end_headers = mock.Mock()
            return instance

        with mock.patch.dict(os.environ, {MODULE.API_TOKEN_ENV: "bridge-token"}):
            missing = handler(None)
            self.assertFalse(missing._require_auth())
            missing.send_response.assert_called_once_with(401)
            missing.send_header.assert_any_call(
                "WWW-Authenticate", 'Bearer realm="ruview-sensing"'
            )

            wrong = handler("Bearer wrong")
            self.assertFalse(wrong._require_auth())

            correct = handler("Bearer bridge-token")
            self.assertTrue(correct._require_auth())
            correct.send_response.assert_not_called()


if __name__ == "__main__":
    unittest.main()
