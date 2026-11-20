# Projektstatus

**Gespeicherter Dokumentationsstand: 3. November 2026. Öffentliche Fassung: 4. November 2026.** Diese Seite fasst gespeicherte Aussagen zusammen. Sie ist keine aktuelle Abfrage laufender Geräte oder Prozesse.

| Bereich | Gespeicherter Stand | Offen |
| --- | --- | --- |
| CSI-Empfang | Vier-RX-Basis und technischer RX1↔RX4-Paarlauf am 29. Oktober bestanden. | Positionsnutzen des Paars und Erweiterung auf weitere Peerlinks. |
| TX-only-Dauerlauf | 60-minütige Aufnahme nach Offline-Neubewertung bestanden. | Ursprünglicher Online-Abschluss fehlgeschlagen; kein globaler Firmware-Freeze daraus. |
| mmWave | Einzelne historische Transportfenster liegen vor. | Zwei Sensoren gemeinsam hinsichtlich Raum, Zeit und Referenzgenauigkeit abnehmen. |
| Position | Diskreter D6-Ansatz und Softwarepfade vorhanden. | Langsames Ablaufen als WiFi-Kalibrierung, unabhängige Blindprüfung und kontinuierliche Ortung als Gesamtsystem. |
| Setup | Simulation mit lokal ergänztem Quellstand beschrieben; einzelne Firmware-Build-/Installationsanleitungen vorhanden. | Im geprüften lokalen Commit `840a2e6e186278a29efd78e09c970b3d64591bf2` fehlen `--rx-session-only` und `--data-dir`; frischer Clone-Build und Gesamt-Erstinstallation sind nicht nachgewiesen. [Quellstand](../setup/installation.md#quellstand-vor-dem-start-prüfen) |
| Pakete | Separate Docker-/Homebrew-Lieferung vom 1. November offline vorbereitet und geprüft. | Nicht integriert oder veröffentlicht; Linux-Image und echte Homebrew-Installation offen. [Paketstand](../setup/installation.md#docker-und-homebrew) |

Für Ergebnisse und ihre Grenzen siehe [Ergebnisübersicht](../research/results.md). Frühere Pläne und Übergaben beschreiben den jeweils damaligen Stand und sind nicht automatisch eine aktuelle Gerätefreigabe. Die detaillierten neueren Entwicklungs- und Laufbelege liegen teilweise lokal beziehungsweise privat und sind nicht Bestandteil eines frischen öffentlichen Checkouts.

Quellen: [Tag14-Abschluss vom 29. Oktober](rx-unified-tag14-result-20261029.md), [Projekt-README: Validierungsstand](../../README.md#aktueller-validierungsstand), [Ergebniskatalog](../../experiment-reports/README.md) und [Firmwareübersicht](../setup/firmware.md).

## Dokumentierter Softwareprüfstand

Der [lokale Semgrep-Bericht vom 2. November](00-status-und-annahmen.md#2026-11-02-lokale-abnahme-der-semgrep-parserabdeckung) nennt eine abgeschlossene Parserprüfung. Seine Test- und Trefferzahlen gehören zu diesem datierten Bericht; daraus folgt keine neue Sicherheits-, CI- oder GitHub-Abnahme.
