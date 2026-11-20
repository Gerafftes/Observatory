# RX Unified Tag 14 Ergebnis vom 29 Oktober 2026

**Gespeicherter Abschluss vom 29. Oktober 2026; öffentliche Fassung vom 4. November 2026.** Die Messfolge Q900 → Short30 → Basis180 → Peerpilot60 → Paar300 einschließlich der Rückkehr auf TX-only wurde im damaligen Abschluss als bestanden bewertet. Das beschreibt den historischen Messstand, keinen heutigen Geräte- oder Prozesszustand. Die damaligen Messungen und Tests wurden für diese Fassung nicht wiederholt.

Der Abschluss dieser Messfolge umfasste keinen 60-minütigen Stabilitätslauf. Die später getrennt dokumentierte TX-only-Neubewertung steht im [Projektstatus](status.md) und im [Validierungsstand des Projekt-READMEs](../../README.md#aktueller-validierungsstand). Daraus folgt kein globaler Firmware-Freeze und keine Freigabe für weitere Geräteaktionen.

## Ergebnisse der Messfolge

| Stufe | Frames | Gemeinsame gültige Fenster | Auswertung | Endeabweichung |
| --- | ---: | ---: | --- | ---: |
| Q900 | – | – | Vier frische RX-Boots qualifiziert, alle eigenen Leases freigegeben | Erste Beobachtungen ca. 810 s, letzte ca. 915 s |
| Short30 | 4.341 | 23/23 | Nativer Audit und PCAP bestanden | 24,93 ms |
| Gültige Basis180 mit korrigiertem Host | 26.760 | 173/173 | Zwei identische native Audits und PCAP bestanden | 37,98 ms |
| Frischer Peerpilot60 vor Paar300 | 9.486 | 53/53 | Funktionaler Nachweis, nativer Audit und PCAP bestanden | 11,49 ms |
| Paar300 | 47.601 | 293/293 | Zwei identische native Audits und PCAP bestanden | 24,69 ms |

Für Paar300 wurden RX1/RX4 nach Ablauf des vorherigen Paarfensters mit unverändertem Paarprofil Generation 10 frisch gestartet. Ihre Paarbereitschaft und der funktionale Peerpilot wurden erneut geprüft. RX2/RX3 behielten ihre ursprünglichen qualifizierten Boots. Die neue Aufnahme passte in das ursprüngliche 900-Sekunden-Paarfenster. Der Pilot verwendete ausdrücklich sein funktionales Receipt; Paar300 verwendete die Paarqualifikation.

| Peerlink in Paar300 | Qualifizierte Frames | Gültige Fenster | Ursprüngliche Mindestanforderung |
| --- | ---: | ---: | --- |
| RX4 → RX1 | 2.581 | 293/293 | Mindestens 600 Frames und 264/293 Fenster |
| RX1 → RX4 | 2.578 | 293/293 | Mindestens 600 Frames und 264/293 Fenster |

Alle TX-Kontrolllinks erreichten ebenfalls 293/293 gültige Fenster. Die neue Pilot-/Paarkette zeigte keine neuen Expired-, Abandoned-, Serialisierungs- oder Abbruchfehler. Die geschlossenen PCAPs hatten keine Kernel-Drops; der Abgleich fand keine unerklärten zusätzlichen Datagramme im Aufnahmeinneren. Alle aufgezeichneten Payloads waren im jeweiligen PCAP vorhanden.

Unerklärte Sequenzlücken bleiben separat dokumentiert: Short30 **4**, gültige Basis180 **9**, frischer Pilot **4**, Paar300 **24**. Die bestandenen Phasengrenzen und der vollständige PCAP/Raw-Abgleich beweisen keine verlustfreie Funkübertragung. Die Messfolge ist außerdem kein Nachweis physischer Raumleistung, D6-Qualifikation oder einer Vier-RX-Mesh-Topologie.

## Versionen und unveränderte Grenzen

Alle Phasen verwenden denselben neuen Firmwarestand:

- App-SHA256: `c2c84c958c465c4cf1809da05a88a0310038638cd0bbcff5fa0e9c539dd645fb`.
- Laufender ESP-Image-Digest: `8347ddc7230ad7dc8f1ed42cb5c763e5353fcab94248f4b7d922df6088acb308`.
- Firmware-Quellmanifest: `3cdddf976a5a8f6fcfc648a1d25167a319b178b508aea4e95d26a87bdfb02dae`.

Q900/Short30 liefen mit dem damals geprüften Host `3aa730bc…ca4d00`. Nach dem Recorderfix liefen die gültige Basis180 und die Peerphasen mit `d262b462…8a45d4`. Beide Hoststände, ihre Quellen und die tatsächlich verwendeten Setups sind pro Phase gebunden; es wird kein einzelner unveränderter Hostbuild für die gesamte Historie behauptet. Der neue Host änderte nur `recording_routes.rs`; Firmware, Raw-Auditlogik und Messgrenzen blieben unverändert. Der vollständige Host-Binärtarget-Testlauf bestand mit 555 Tests und 6 vorhandenen ignorierten Tests.

Phasendauern, 900-Sekunden-Paarfenster, 50-ms-Ende-/Zeitabbildungsgrenzen, Ressourcen, Layouts, Masken, Raten und Fenstergrenzen wurden nicht gelockert.

## Negative Befunde bleiben erhalten

Die erste Basis180 vom 29.10. bleibt **negativ**: Vier zusätzliche gültige Datagramme im PCAP fehlten im Raw-Nachweis. Die Offlineanalyse reproduzierte blockierende Dateiarbeit in der Statusroute und fehlende Recorderbilanzierung früher Queueverluste. Der korrigierte Host lagert den Scan aus und bilanziert neue CSI-Queueverluste konservativ bis zum Writerabschluss. Historische Verluste werden über einen eigenen Aufnahmestartstand abgegrenzt. Die gültige neue Basis wurde separat gemessen; sie wertet die alte Datei nicht nachträglich auf.

Die damalige zusätzliche Schlussprüfung verifizierte **124 unveränderliche Nachweisdateien** gegen gespeicherte Hashes und prüfte die Raw-/PCAP-Bindungen der akzeptierten Phasen. Das weiterlaufende operative Hostlog wurde ausdrücklich nicht als unveränderliche geschlossene Datei behandelt.

Rohdaten, vollständige Geräte-/Laufdiagnosen und die privaten Originalbelege sind nicht Teil dieser öffentlichen Darstellung. Die dokumentierten Software-Artefakthashes binden die damaligen Versionen; sie sind keine Geräteauthentisierung.
