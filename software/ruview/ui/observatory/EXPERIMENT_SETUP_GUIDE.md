# Observatory Control Center – historischer Setup- und Experimentleitfaden

Gespeicherter Bedienablauf vom 29. August 2026; öffentliche Auswahl vom 4. November. Aktuelle fachliche Einstiege sind die [Benutzeranleitung](../../../../docs/user-guide/interface.md) und die [Positionierung](../../../../docs/software/positioning.md). Der damalige UI-/Teststand ist keine heutige gemeinsame Radar- oder Ortungsabnahme.

## Grundregel

WiFi-CSI geht in das WiFi-Modell; mmWave ist eine getrennte Kalibrierungs-/Truth-Referenz. Simulation bleibt `SOFTWARE-ONLY / UNVALIDATED`. Sie kann Profil-/Run-Navigation und Fehlerzustände zeigen, aber keine echte Baseline, gültige Sensorzeitzuordnung oder Positionsgüte liefern.

## Setup, Profil und Run

Ein gespeichertes Profil beschreibt Raum, TX/RX und Referenzmontage. Jede echte Änderung erzeugt eine unveränderliche Profilversion; identisches Speichern erzeugt keine zusätzliche Version. Ein Run bindet genau diese Version. Ein Profil-Hash allein ersetzt kein aktives Laufzeitsiegel.

Der damalige Entwurf für ein versiegeltes Setup blieb bis zur vollständigen Firmware-, Grid-, TX-Filter-, Host- und Referenzbindung unversiegelbar. Erst ein passendes aktives Runtime-Seal kann Profil-ID/-Hash, Run-ID, Setup-ID/-Hash und `active_position_setup_v2` gemeinsam binden. Ein laufender versiegelter Versuch übernimmt neue Entwurfsgeometrie nicht still. Das ersetzt keine anschließenden RX-/Transport-/Zeit-/Radargates.

## Dokumentierte Phasen

1. Setup-Profil und unveränderliche Version speichern.
2. Run aus genau diesem Profil anlegen.
3. Run an die vollständige aktive Setupbindung anschließen.
4. Leere WiFi-Baseline mit echten gültigen RX-Daten erfassen.
5. Geprüfte und zeitlich zuordenbare mmWave-Referenz für die Kalibrierung getrennt erfassen.
6. Blind-Reihenfolge erstellen; Truth während der Aufnahme verborgen halten.
7. Neue echte CSI-Blindaufnahmen ohne Referenz im WiFi-Eingang erfassen.
8. Prediction aus WiFi registrieren und hashbinden.
9. Erst danach getrennte Truth aufdecken/registrieren.
10. Accuracy, Coverage, Distanzfehler, Confusion, `unknown`/`ambiguous` und Qualitätsgates auswerten.
11. Setup, Artefaktbindungen und Grenzen im Report erhalten.

Die kontinuierliche Radarreferenz und ein historisches P01–P09-Kontrollraster sind verschiedene Wege; das manuelle Raster ist keine Voraussetzung des fachlichen Ziels einer Laufkalibrierung. Ohne vollständige echte Aufnahmen/Truth bleiben Kennzahlen Demo-Metadaten. Blinddaten dürfen nicht über Capture-Metadaten in Training oder Prediction gelangen.

## Zustände richtig lesen

| Zustand | Dokumentierte Bedeutung |
| --- | --- |
| CONTROL CENTER READY | Backend/Cockpit erreichbar; keine Sensorabnahme daraus. |
| CONTROL CENTER OFFLINE | Backend nicht erreichbar. |
| 0/4 Nodes | Keine aktiven CSI-RX. |
| Wartet auf mmWave | Keine gültige Radar-Referenz. |
| uart_idle / invalid_frames / udp_blocked | Verschiedene UART-/Parser-/UDP-Diagnoseklassen. |
| streaming | Datenpfad sichtbar; Raumtransformation und Referenzqualität zusätzlich prüfen. |
| SOFTWARE-ONLY / UNVALIDATED | Demo, keine Hardwarequalität. |

Ein realer Lauf benötigt alle vier erwarteten RX, attestierte Quelle, passende Grids und Zeitbezüge, echte leere Baseline, geprüfte Radartransformation sowie getrennte Prediction/Truth und bestandene definierte Blindgates. Ein erfolgreicher Flash, Parser oder optisch aktives UI genügt nicht.

Der Originaltext berichtete einen simulierten Browserdurchlauf ohne Hardware und 68 bestandene UI-Tests. Diese Zahlen bleiben ein gespeicherter damaliger Softwarebefund; sie wurden hier nicht wiederholt und belegen weder Funk noch Radarqualität. Für die Demo gilt ausschließlich die [isolierte Installationsanleitung](../../../../docs/setup/installation.md#simulation-starten).

Weiter: [Cockpit](../../../experiment-cockpit.md), [Live-Datenvertrag](LIVE_DATA_CONTRACT.md) und [Ergebnisübersicht](../../../../docs/research/results.md).
