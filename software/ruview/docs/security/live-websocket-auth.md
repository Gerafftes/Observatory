# Live WebSocket authentication

When `RUVIEW_API_TOKEN` is set, `/ws/sensing`, `/ws/introspection` and
`/api/v1/stream/pose` require the token before upgrading. This also applies
on loopback when a token is explicitly configured. Tokenless loopback remains
supported; routable startup still requires a token. Health probes and static
UI remain public. Introspection is served on the HTTP listener only.

## Client setup

- Browser dashboard, visualization, Observatory and Pose Fusion: enter the
  server address in **Server URL** and its token in **Server token**, then
  select **Connect**. It is stored in sessionStorage for this tab and the
  selected server hostname; paired HTTP/WS ports share it. **Clear token**
  removes it for that server. Configure each different host explicitly.
- Mobile: save the server URL first, enter the token in Settings, then save.
  The token is session-only and excluded from AsyncStorage. Changing the
  server URL clears the draft credential. Reconnection retains the saved token.
- Desktop Sensing: enter the token and select **Connect**. It remains in memory
  for the Sensing page; enter it again after leaving the page.
- Python: `SensingClient(url, token=os.environ['RUVIEW_API_TOKEN'])`.
  The token argument is optional for tokenless local servers.
- Native clients can continue sending `Authorization: Bearer <token>`.

Use HTTPS/WSS or a trusted encrypted tunnel for remote deployments. Token
encoding is not encryption. Keep exact Origin and Host allowlists configured;
they complement authentication and remain enforced independently.

## Wire contract

Browser clients offer two Sec-WebSocket-Protocol values:

1. `ruview.v1`
2. `ruview.bearer.` followed by lowercase hexadecimal UTF-8 token bytes.

The server selects only `ruview.v1`; it never echoes the credential offer.
Protocol credentials are accepted only on the three live WebSocket routes,
not on REST endpoints. Explicit Authorization takes precedence: a wrong or
duplicate Authorization header is rejected even with a valid protocol offer.
Multiple credential offers are rejected. Query-string tokens are not accepted.

## Regression checks

From `RuView/v2`: `cargo test -p wifi-densepose-sensing-server --lib` and
`cargo build -p wifi-densepose-sensing-server --bin sensing-server`.

From `RuView`: `node --test ui/tests/*.test.mjs`.

From `RuView/python`, with pytest, pytest-asyncio and websockets installed:
`SENSING_SERVER_BIN=/absolute/path/to/sensing-server pytest -q tests/test_sensing_ws_auth.py`.
This starts temporary loopback listeners with simulated data, checks missing,
wrong and valid credentials, public health, independent Origin rejection,
protocol negotiation, and actual Python stream receipt. It loads the pure
Python client source without requiring the unrelated native DSP extension.

Mobile checks: `tsc --noEmit` and the focused Jest service/settings tests.
Desktop checks: `tsc --types vite/client` and `vite build`; the explicit Vite
types supply the CSS-import declaration missing from the existing tsconfig.
