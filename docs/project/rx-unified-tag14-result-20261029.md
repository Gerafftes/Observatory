<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX Unified Tag 14 Ergebnis vom 29 Oktober 2026

**Die vereinbarte Messfolge Q900 → Short30 → Basis180 → Peerpilot60 → Paar300 samt TX-only-Rückkehr ist bestanden. Schritt 7 ist abgeschlossen.** Der Firmware-Freeze bleibt gesondert offen: Der [Projektplan](tag14-mit-rx-unified-20261027.md#7-was-nach-tag-14-für-den-firmware-freeze-bleibt) verlangt dafür zusätzlich einen mindestens 60-minütigen Stabilitätslauf. Dieser Abschluss umfasst keinen solchen Dauerlauf und keine Freigabe für weitere Geräteaktionen.

## Bestätigter Abschlusszustand

RX1/RX4 wurden nach Paar300 einmalig auf TX-only Generation 11 zurückgestellt und neu gestartet. RX2/RX3 blieben auf TX-only Generation 1. Alle vier haben leere Peerlisten, Zustand `NORMAL`, bestätigte Geräte-/Quell-/Profilbindung und fortschreitende Host- sowie Gerätezähler.

Die ursprüngliche Rückkehrprüfung bestätigte **21,63 s** stabilen Standardbetrieb. Schritt 7 bestätigte denselben Zustand nochmals **21,76 s rein lesend**, mit gleichen Boots, Profilen und Netzgenerationen und ohne neue harte Transportfehler. Alle Messleases sind inaktiv, kein Recorder ist aktiv. In Schritt 7 wurden keine Profile geschrieben, Geräte neu gestartet oder Aufnahmen gestartet.

Der aktive RX-only-Host ist PID `91923`, Binary-SHA256 `d262b46266878537986324fe354e9943b6920fe9dc50122cee75af45678a45d4`, mit dem passenden Rückkehr-Setup. Diese Prozessnummer ist eine Beobachtung vom Abschlusszeitpunkt, keine dauerhafte Identität.

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

Phasendauern, 900-Sekunden-Paarfenster, 50-ms-Ende-/Zeitabbildungsgrenzen, Ressourcen, Layouts, Masken, Raten und Fenstergrenzen wurden nicht gelockert. Die äußere Freigabe wurde ausdrücklich auf 29.10.2026, 18:00 Uhr Europe/Berlin verlängert. Das davon getrennte native Audit-Rechenbudget wurde ausdrücklich auf **90 s gesamt** erhöht; einschließlich der negativen ersten Basis wurden **64,87 s** verbraucht. Der Ledger wurde nicht zurückgesetzt.

## Negative Befunde bleiben erhalten

Die erste Basis180 vom 29.10. bleibt **negativ**: Vier zusätzliche gültige Datagramme im PCAP fehlten im Raw-Nachweis. Die Offlineanalyse reproduzierte blockierende Dateiarbeit in der Statusroute und fehlende Recorderbilanzierung früher Queueverluste. Der korrigierte Host lagert den Scan aus und bilanziert neue CSI-Queueverluste konservativ bis zum Writerabschluss. Historische Verluste werden über einen eigenen Aufnahmestartstand abgegrenzt. Die gültige neue Basis wurde separat gemessen; sie wertet die alte Datei nicht nachträglich auf.

Die zusätzliche aktuelle Schlussprüfung verifizierte **124 unveränderliche Nachweisdateien** gegen gespeicherte Hashes und prüfte die Raw-/PCAP-Bindungen der akzeptierten Phasen. Das weiterlaufende operative Hostlog wurde ausdrücklich nicht als unveränderliche geschlossene Datei behandelt.

## Private Nachweise

Die folgende Ablage enthält private Belege; Rohdaten, vollständige Diagnosen, Geräteidentitäten und Tokenwerte werden nicht in dieses Repository kopiert.

- Abschluss Schritt 7: `PRIVATE_EVIDENCE/rx-tag14-closeout-20261029-run01/` – `SUMMARY.private.json`, `LIVE-DEFAULT-PASS.private.json`, `EVIDENCE-VERIFICATION.private.json` und Ledger-Snapshot.
- Paar300 und Rückkehr: `PRIVATE_EVIDENCE/rx-peer-paar300-20261029-run01/` – `RESULT.private.json`, `AUDIT-RECHECK.private.json`, `default-final-20s-01/PASS.private.json`, `CURRENT-STATE.private.json`, Hashmanifest und finale Auswertung.
- Gültige Basis: `PRIVATE_EVIDENCE/rx-peer-basis180-fixedhost-20261029-run01/`.
- Short30: `PRIVATE_EVIDENCE/rx-peer-short30-20261029-run01/`.
- Q900: `PRIVATE_EVIDENCE/rx-peer-q900-20261029-run01/`.
- Negative erste Basis und Untersuchung: `PRIVATE_EVIDENCE/rx-peer-basis180-20261029-run01/` und `PRIVATE_EVIDENCE/rx-basis180-host-gap-offline-20261029-run01/`.
- Hostfix: `PRIVATE_EVIDENCE/rx-host-gap-fix-offline-20261029-run01/`.

Die historischen [Ergebnisse vom 27. Oktober](rx-unified-tag14-result-20261027.md) und der [Expiry-Handoff](rx-peer-expiry-handoff-20261028.md) bleiben als damalige Befunde erhalten. Nächster gesonderter Planungsschritt für den Firmware-Freeze ist der im Projektplan geforderte Stabilitätslauf; eine automatische Fortsetzung ist mit diesem Abschluss nicht verbunden.
