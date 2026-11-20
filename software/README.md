# Observatory software source

Dieser öffentliche Dokumentationsstand beschreibt den angepassten RuView-Snapshot. Er enthält Dokumentation und Bilder, keinen ausführbaren Rust-/ESP-IDF-/Browser-Quellbaum. Die folgenden Herkunftsangaben bleiben als datierte Importbindung erhalten.

## Provenance

The parent source snapshot is based on RuView commit
`bbf99e2dc94458e80121d2bacd9a8f229acb5a7c` (`feat: publish RuView calibration
workflow and Observatory docs`). The following former submodules are vendored
at their pinned commits:

| Path | Commit |
|---|---|
| `vendor/midstream` | `8f70d2bb9d64a8ddee7745bc18bc4ff9e151845e` |
| `vendor/ruvector` | `a083bd77fa2f4879595daa68686ed5b2132d981a` |
| `vendor/sublinear-time-solver` | `c25dddf163d8c413628ecdc6e979583d39270f22` |
| `vendor/rvcsi` | `72891d740f92903c78a8208a9069f6c82a4d1cc2` |
| `vendor/rufield` | `509d8ae29e654a322910bd504d325b0dd1fdd895` |
| `v2/crates/ruv-neural` | `c9638faaf8ae1d910039171be487a465a5762313` |
| `v2/crates/ruview-swarm` | `267aba5be2288aa6cbe574492062b04fa8c8a6ce` |
| `v2/crates/worldgraph` | `fdade422069d3162634292710d78cb9963c0f48d` |

## Veröffentlichte Quellreferenzen

Am 4. November 2026 wurden sieben Dateiobjekte am veröffentlichten Commit `028f9d3f2703e1f3610e856f48fa0a709647818b` lesend nachgewiesen. Git-Blobhash und Größe stimmen jeweils mit den gebundenen lokalen Bytes überein. Der lokale Commit `840a2e6e186278a29efd78e09c970b3d64591bf2` ist davon verschieden; insbesondere fehlen ihm die Optionen `--rx-session-only` und `--data-dir`.

- [csi_collector.c](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/firmware/esp32-csi-node/main/csi_collector.c)
- [index.html](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/ui/index.html)
- [server-control.service.js](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/ui/services/server-control.service.js)
- [d6_fingerprint.rs](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/v2/crates/wifi-densepose-sensing-server/src/d6_fingerprint.rs)
- [main.rs](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/v2/crates/wifi-densepose-sensing-server/src/main.rs)
- [position_capture.rs](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/v2/crates/wifi-densepose-sensing-server/src/position_capture.rs)
- [server_control.rs](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/v2/crates/wifi-densepose-sensing-server/src/server_control.rs)

Die Dateiprüfung vom 4. November betraf diese sieben Dateien. Der separate
Schnellstarttest umfasste Build und Start in einem frischen Checkout des
GitHub-Commits `11ccc9b`. Die [Installationsanleitung](../docs/setup/installation.md)
beschreibt den Demo-Aufruf mit eigenem Datenverzeichnis, Loopback und
abgeschalteten mmWave-Netzwerkdiensten und Steuerungshelfer.

## Umfang und Lizenzen

Der vollständige Import enthält Rust-Workspace, UI, Firmware, historische Python-Teile und eigene Fremdkomponenten. Paketabhängigkeiten können dort eigene Downloads benötigen. Git-Historien, Caches, Aufnahmen, Konfigurationen und private Provisionierungsdaten sind kein Bestandteil dieses Dokumentationssatzes.

Lizenz- und Notice-Dateien der jeweiligen Fremdkomponenten gelten für deren Quellbäume; die [Projektlizenz](../LICENSE.md) ersetzt sie nicht. BFLD-/ADR-Verträge und frühere Replay-/Softwaretests sind keine Observatory-Deployment- oder Hardwareabnahme. Hier wurden keine solchen Tests erneut ausgeführt.

Weiter: [Architektur](../docs/software/architecture.md), [Benutzeranleitung](../docs/user-guide/interface.md) und [öffentliche historische Anpassungen](../docs/project/06-ruview-anpassungen.md).
