# mmWave transport OTA rollout and before/after comparison

Date: 2026-09-13
Scope: `MMWAVE1` and the local sensing server; RX/CSI stayed offline and was not used.

## Outcome

Firmware `0.1.4` is running on `MMWAVE1`. The node uses one UDP copy, no copy
delay, a 50 ms bridge send limit, and disabled WiFi power saving. Its persistent
transport target is `192.168.4.3:5010`. The CAD mounting transform is also
persistent on the node:

- origin: `[3950, 3300]` mm
- yaw: `180000` mdeg
- raw X inversion: `false`

The server receives fresh radar packets and exposes the new rolling transport
metrics in the UI. The final image SHA-256 is
`8d08b661df74ee8c040511a5d2a7c6d5fb5481604e1bd1c7239400563ddb09e5`.

The overall transport is **better, but not loss-free**. Tail arrival latency,
server processing latency, unique arrival rate, duplicates, and queue behavior
improved. Median arrival spacing became 18.6% larger, and the remaining 13.1%
sequence-loss ratio still blocks the loss-free radar preflight.

## Important correction to the planned baseline

The plan expected the old image to send to `192.168.4.3:5010` while the server
received on `192.168.4.4:5010`. Immediately before the OTA, the live system had
already changed: both node and receiver used `192.168.4.3:5010`. A real
pre-OTA UDP baseline was therefore available and was measured instead of being
marked unavailable.

Pre-OTA node status:

- firmware `0.1.0`
- UART bytes `659576`
- valid radar frames `21985`
- successful UDP sends `5113`
- UDP send failures `40`
- transform `[0, 0]`, yaw `0`, raw X not inverted

The zero transform confirms that the CAD orientation had not yet reached the
physical node before this rollout.

## Implemented changes

1. The firmware bridge send interval is configurable and defaults to 50 ms.
2. The normal transport uses one UDP copy with no artificial copy delay.
3. WiFi power saving is disabled for the mains-powered radar node.
4. The server requests a 256 KiB UDP receive buffer and decouples socket reads
   from packet processing.
5. A short reorder window, sequence validation, and deduplication handle late or
   repeated UDP datagrams.
6. The authenticated `/transport` route persists a repaired collector address.
7. The active UDP send path now takes a fresh atomic configuration snapshot for
   every measurement. This fixes a rollout bug where `/transport` changed the
   reported target but the already-created socket continued using the old one
   until reboot.
8. The server keeps a 256-sample in-memory window with valid count/rate,
   arrival median/P95, and receive-to-process median/P95. No measurement window
   is written to SQLite; cumulative counters remain intact.
9. The UI displays these rolling values in the mmWave calibration diagnostics.

## Controlled runs

Each main run lasted 60 seconds. RX stayed off. The same node, mounting point,
server process, UDP port, and WLAN were used. The observed target state was not
perfectly identical: the third pre-OTA run contained 73 `room_bounds`
rejections, while the post-OTA runs were mostly `no_target`. Transport-neutral
metrics therefore include accepted plus rejected unique packets where possible.

### Before OTA (`0.1.0`, three copies, 5 ms spacing)

| Run | Raw UDP | Accepted | Rejected | Lost seq. | Duplicates | Window valid rate | Arrival median / P95 | Process median / P95 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 351 | 128 | 0 | 30 | 223 | 2.23 Hz | 318 / 1339 ms | 11 / 21 ms |
| 2 | 374 | 139 | 3 | 23 | 231 | 2.10 Hz | 348 / 1283 ms | 11 / 21 ms |
| 3 | 247 | 32 | 73 | 44 | 142 | 1.47 Hz | 320 / 1468 ms | 12 / 20 ms |

The third node-side UART snapshot was incomplete because the node status probe
was intermittently unavailable. Its server-side counters are complete.

### After OTA (`0.1.2`, one copy, no spacing)

`0.1.2` contained the final transport behavior. Later version numbers were used
only for the controlled redundancy trial and final reinstallation.

| Run | UART frames | UDP sent | Raw UDP | Accepted | Rejected | Lost seq. | Duplicates | Arrival median / P95 | Process median / P95 | Max sampled queue |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 677 | 159 | 134 | 132 | 2 | 26 | 0 | 372 / 1090 ms | 12 / 20 ms | 1 |
| 2 | 674 | 158 | 152 | 152 | 0 | 10 | 0 | 390 / 869 ms | 9 / 20 ms | 1 |
| 3 | 678 | 159 | 131 | 131 | 0 | 27 | 0 | 407 / 993 ms | 10 / 20 ms | 1 |

No run produced a local UDP send failure, duplicate, reboot, reorder discard, or
monotonically growing queue. The node parsed 2029 UART frames in 180 seconds
(11.27 Hz, 88.7 ms mean spacing) but emitted 476 UDP measurements (2.64 Hz,
378.2 ms mean spacing). The 50 ms value is therefore not the current bottleneck:
the LD2450 data arrives in bursts and the bridge still emits only a new eligible
sample, rather than inventing 20 independent radar frames per second.

## Aggregate comparison

| Metric | Before | After | Change | Verdict |
|---|---:|---:|---:|---|
| Unique packets arriving (accepted + rejected) | 375 / 180 s | 417 / 180 s | **+11.2%** | better |
| Accepted packets | 299 / 180 s | 415 / 180 s | **+38.8%** | better, scene-sensitive |
| Sequence-loss ratio | 20.6% | 13.1% | **−7.4 pp / −36.1% relative** | better, not passed |
| Duplicate datagrams | 596 | 0 | **−100%** | better |
| Raw datagrams including duplicates | 972 | 417 | **−57.1%** | lower airtime/load |
| Rolling valid rate, mean of run endpoints | 1.94 Hz | 2.34 Hz | **+20.9%** | better, scene-sensitive |
| Arrival median, mean of run endpoints | 328.7 ms | 389.7 ms | **+18.6%** | worse |
| Arrival P95, mean of run endpoints | 1363.3 ms | 984.0 ms | **−27.8%** | better |
| Receive-to-process median | 11.3 ms | 10.3 ms | **−8.8%** | better |
| Receive-to-process P95 | 20.7 ms | 20.0 ms | **−3.2%** | slightly better |
| Queue | cumulative peak 8 | no new peak; sampled max 1 | no growth | better/stable |

The accepted-rate improvement is not a pure radio metric because room-bound
rejections differed between runs. The +11.2% unique-arrival result and the
sequence-loss ratio are the more defensible transport comparison.

## Diagramme

Die Vorher-/Nachher-Tabelle ist als vier Bklit-Vergleichsdiagramme
aufbereitet: Ankunft und Verlust, Ankunftslatenz, Serververarbeitung sowie der
separate Redundanztest. Die Diagramme verwenden dieselben Messwerte wie die
Tabelle; P95, Verlustquote und Duplikate bleiben dabei von der
szeneabhängigen akzeptierten Rate getrennt.

| Diagramm | Export |
|---|---|
| Ankunft und Verlust | [`01_ankunft_und_verlust.png`](mmwave-transport_bklit/01_ankunft_und_verlust.png) |
| Ankunftslatenz | [`02_ankunftslatenz.png`](mmwave-transport_bklit/02_ankunftslatenz.png) |
| Serververarbeitung | [`03_serververarbeitung.png`](mmwave-transport_bklit/03_serververarbeitung.png) |
| Redundanzvergleich | [`04_redundanzvergleich.png`](mmwave-transport_bklit/04_redundanzvergleich.png) |

Die Datenzuordnung und QA der PNGs ist in der
[Bklit-Render-Spezifikation](2026-09-14_mmwave-transport_bklit-render-spec.md)
dokumentiert.

<table>
<tr>
<td><a href="mmwave-transport_bklit/01_ankunft_und_verlust.png"><img src="mmwave-transport_bklit/01_ankunft_und_verlust.png" alt="Ankunft und Verlust im Vorher-Nachher-Vergleich" width="480"></a><br><strong>Ankunft und Verlust</strong></td>
<td><a href="mmwave-transport_bklit/02_ankunftslatenz.png"><img src="mmwave-transport_bklit/02_ankunftslatenz.png" alt="Ankunftsmedian und P95 im Vorher-Nachher-Vergleich" width="480"></a><br><strong>Ankunftslatenz</strong></td>
</tr>
<tr>
<td><a href="mmwave-transport_bklit/03_serververarbeitung.png"><img src="mmwave-transport_bklit/03_serververarbeitung.png" alt="Serververarbeitung und Queue-Evidenz" width="480"></a><br><strong>Serververarbeitung</strong></td>
<td><a href="mmwave-transport_bklit/04_redundanzvergleich.png"><img src="mmwave-transport_bklit/04_redundanzvergleich.png" alt="Vergleich einer und zweier UDP-Kopien" width="480"></a><br><strong>Redundanzvergleich</strong></td>
</tr>
</table>

Die vier statischen PNG-Exporte oben sind die kanonische, im Repository
verlinkte Diagrammansicht.

## Redundancy decision

Because the one-copy runs proved real sequence loss, a separate 60-second test
used two copies with 5 ms spacing (`0.1.3`):

| Mode | Accepted | Lost seq. | Loss ratio | Duplicates | Arrival median / P95 | Process median / P95 |
|---|---:|---:|---:|---:|---:|---:|
| One copy, three-run aggregate | 415 | 63 | 13.1% | 0 | 389.7 / 984.0 ms | 10.3 / 20.0 ms |
| Two copies, one run | 132 | 21 | 13.7% | 105 | 360 / 1076 ms | 12 / 21 ms |

The second copy did not reduce loss and increased duplicate traffic and tail
processing latency. The node was therefore returned to the one-copy/no-delay
default in final firmware `0.1.4`.

## Final live verification

- `/ota/status`: firmware `0.1.4`, partition `ota_1`
- `/transport`: authenticated idempotent update returned
  `192.168.4.3:5010`; the route is implemented and no longer returns 404
- target equals receiver: `192.168.4.3:5010`
- transform equals the active SQLite profile: `[3950, 3300]`, yaw `180000`
- UART and valid-radar counters continued increasing
- three UDP send failures occurred only during post-reboot ARP/WLAN startup;
  the counter remained at three while successful sends rose from 29 to 110
- server `raw_udp_packets > 0`, `packets_received > 0`, queue length `0`
- `radar_stream_fresh = true`
- overall radar preflight is still **not ready**, because the rolling
  `radar_sequence_loss_free` gate continues to observe sequence gaps
- browser reload preserved the saved CAD profile, transform, collapsed setup
  details, and RX/TX visibility choice; the new rolling metrics render live

## Tests

- ESP-IDF 5.4.4 `idf.py build` for `esp32c3`: passed
- final image size: 890880 bytes, 53% of the smallest app partition free
- UI tests: 110 passed
- mmWave route tests: 6 passed
- mmWave connection tests: 2 passed
- transport-window metric test: 1 passed
- LD2450 parser test: passed
- `cargo check`: passed
- OTA tool Python syntax check: passed
- `git diff --check`: passed

## Source-backed constraints

Hi-Link does not expose an arbitrary host-controlled LD2450 reporting-rate knob;
the 50 ms bridge setting must remain a send limit, not a promise of 20 new sensor
frames per second. Espressif documents the latency/power tradeoff of WiFi power
saving and the lwIP buffer controls used here. UDP still provides no delivery
guarantee, so sequence measurements remain the deciding evidence rather than a
successful `sendto` return.

- https://ask.hlktech.com/question/277155.html
- https://www.hlktech.com/en/Goods-226.html
- https://docs.espressif.com/projects/esp-faq/en/latest/software-framework/protocols/lwip.html
- https://docs.espressif.com/projects/esp-idf/en/release-v5.4/esp32/api-guides/wifi.html
- https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/lwip.html
- https://docs.rs/socket2/latest/socket2/struct.Socket.html
- https://docs.rs/tokio/latest/tokio/net/struct.UdpSocket.html
- https://www.rfc-editor.org/rfc/rfc8085.html

## Next engineering decision

Do not add more blind UDP copies. The next optimization should measure and
address the approximately 13% single-copy sequence loss directly (RSSI/channel,
ARP/startup behavior, WiFi driver retry/airtime, and packet scheduling) while
keeping the current bounded server queue and one-copy low-latency default.
