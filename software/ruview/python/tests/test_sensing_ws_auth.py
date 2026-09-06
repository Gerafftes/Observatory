"""Real sensing-server handshakes; simulation only, no sensor acquisition.

Run with SENSING_SERVER_BIN=/absolute/path/to/sensing-server pytest -q
tests/test_sensing_ws_auth.py. The binary runs in a temporary directory.
"""
import contextlib
import http.client
import importlib.util
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

import pytest
# This client is pure Python. Load its source directly so this security check
# doesn't require building the unrelated PyO3 DSP extension.
spec = importlib.util.spec_from_file_location(
    'sensing_ws_client_under_test', Path(__file__).parents[1] / 'wifi_densepose/client/ws.py')
client_module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = client_module
spec.loader.exec_module(client_module)
SensingClient = client_module.SensingClient

TOKEN = 'test-only-token-+/='
PROTOCOL = 'ruview.v1, ruview.bearer.' + TOKEN.encode().hex()


def request(port, path, headers=None, upgrade=True):
    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=2)
    fields = {'Host': f'localhost:{port}'}
    if upgrade:
        fields.update({'Connection': 'Upgrade', 'Upgrade': 'websocket',
                       'Sec-WebSocket-Version': '13',
                       'Sec-WebSocket-Key': 'dGhlIHNhbXBsZSBub25jZQ=='})
    fields.update(headers or {})
    try:
        connection.request('GET', path, headers=fields)
        response = connection.getresponse()
        return response.status, dict(response.getheaders())
    finally:
        connection.close()


@pytest.fixture(params=[True, False], ids=['token-configured', 'tokenless-loopback'])
def server(request, tmp_path):
    binary = os.environ.get('SENSING_SERVER_BIN')
    if not binary:
        pytest.skip('SENSING_SERVER_BIN is required for real listener tests')
    with contextlib.ExitStack() as stack:
        sockets = [stack.enter_context(socket.socket()) for _ in range(3)]
        for sock in sockets:
            sock.bind(('127.0.0.1', 0))
        http_port, ws_port, udp_port = [sock.getsockname()[1] for sock in sockets]
    env = dict(os.environ)
    env['WDP_RUFIELD_SIGNING_SEED'] = '11' * 32
    env.pop('RUVIEW_API_TOKEN', None)
    if request.param:
        env['RUVIEW_API_TOKEN'] = TOKEN
    with (tmp_path / 'server.log').open('w+') as log:
        process = subprocess.Popen([
            binary, '--source', 'simulate', '--bind-addr', '127.0.0.1',
            '--http-port', str(http_port), '--ws-port', str(ws_port),
            '--udp-port', str(udp_port),
        ], cwd=tmp_path, env=env, stdout=log, stderr=log)
        try:
            for _ in range(200):
                if process.poll() is not None:
                    log.seek(0)
                    pytest.fail('server exited: ' + log.read()[-4000:])
                try:
                    with socket.create_connection(('127.0.0.1', http_port), timeout=.1):
                        break
                except OSError:
                    time.sleep(.1)
            else:
                pytest.fail('server startup timed out')
            yield http_port, ws_port, request.param
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


def test_real_listener_authentication(server):
    http, ws, enabled = server
    for port, path in [(ws, '/ws/sensing'), (http, '/ws/sensing'),
                       (http, '/ws/introspection'), (http, '/api/v1/stream/pose')]:
        for headers in [{}, {'Authorization': 'Bearer wrong'},
                        {'Sec-WebSocket-Protocol': 'ruview.v1, ruview.bearer.zz'},
                        {'Sec-WebSocket-Protocol': PROTOCOL + ', ruview.bearer.00'},
                        {'Authorization': 'Bearer wrong', 'Sec-WebSocket-Protocol': PROTOCOL}]:
            assert request(port, path, headers)[0] == (401 if enabled else 101)
        assert request(port, path + '?token=' + TOKEN)[0] == (401 if enabled else 101)
        for headers in [{'Authorization': 'Bearer ' + TOKEN},
                        {'Sec-WebSocket-Protocol': PROTOCOL}]:
            status, response_headers = request(port, path, headers)
            assert status == 101
            assert all(TOKEN not in value and 'ruview.bearer.' not in value for value in response_headers.values())
            if 'Sec-WebSocket-Protocol' in headers:
                assert response_headers.get('sec-websocket-protocol') == 'ruview.v1'
            headers['Origin'] = f'http://localhost:{http}'
            assert request(port, path, headers)[0] == 101
            headers['Origin'] = 'http://untrusted.example'
            assert request(port, path, headers)[0] == 403
    for port in [http, ws]:
        assert request(port, '/health', upgrade=False)[0] == 200
    # Introspection was never registered on the dedicated listener.
    assert request(ws, '/ws/introspection', {'Authorization': 'Bearer ' + TOKEN})[0] == 404


async def test_python_client_receives_real_simulated_stream(server):
    http, ws, enabled = server
    for port in [http, ws]:
        async with SensingClient(f'ws://127.0.0.1:{port}/ws/sensing', token=TOKEN if enabled else None) as client:
            message = await client.recv_one(timeout=5)
            assert message.type
            assert client._ws.subprotocol == ('ruview.v1' if enabled else None)
