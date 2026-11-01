# RX2 Kandidat C Installation und CSI Vergleich geprüft

Stand: 13. Oktober 2026. Der Koordinator hat ausschließlich gespeicherte Belege ausgewertet; keine Geräteabfrage, Installation, Wiederholungsmessung oder Agentenstarts.

**Exakt Kandidat C ist im dokumentierten Endzustand auf ota_0 / VALID bestätigt.** Der passende CSI-Durchsatz ist deutlich gestiegen. Der strenge Formatvergleich bleibt wegen eines einzigen Endfensters formal FAIL.

| Nachweis | Unabhängig bestätigtes Ergebnis |
|---|---|
| Artefakte | Alle 312 Manifestdateien unverändert; Original-HTTP-Antworten nach Größe/Hash geprüft |
| Installation | Ein POST, curl 0/HTTP 200; voller C-Image-Digest und Diagnose-Source-SHA passen |
| Boot/OTA | Neuer Boot, drei konsistente ota_0/VALID-Belege mit 3,220/3,291 s Abstand; gleicher Boot und C/VALID im Endstatus nach Capture |
| Konfiguration | Alle acht Exportfelder vor/nach nach Typ und Wert gleich der Referenz |
| Aufnahme | Ein Capture, 120,010 s; vollständige PCAP, 0 gemeldete Kernel-Drops; keine Geräteanfrage während des Messfensters |
| CSI | 3.825 gültige einzigartige Frames, alle passend zum unveränderten 128er-Raster; 31,872/s, 0 Formatausschlüsse |
| Fenster | 112/113 nach Einschwingen bestanden; letzte 3-s-Scheibe allein wegen Abdeckung abgelehnt |
| Sequenzen | 13 ungeklärte innere Positionen / 3.837 Übergänge; keine bewiesene WLANverlustquote |

Vergleichsbezug bleibt die alte Aufnahme: 231 passende Frames, 1,925/s, 89,49 % Formatausschlüsse und 0/113 Fenster. Der passende Durchsatz steigt damit rund 16,56-fach. Das ist kein isolierter HT20-A/B-Kausalnachweis; C enthält die bereits gebundenen Diagnose-/Basisänderungen.

Der Diagnosezeitraum klammert die Aufnahme mit 120,238 s ein. Er enthält keine Queueverwerfungen, Sendefehler oder Abbrüche, aber 14 späte Ratengate-Verwerfungen. Diese Population ist nicht identisch mit dem PCAP-Fenster; weder die 14 Verwerfungen noch die 3.856 SEND_OK werden unmittelbar gegen die 13 offenen Sequenzpositionen beziehungsweise 3.825 Beobachtungen verrechnet.

## Endkante im selben Review geprüft

Der Decoder wurde aus der Original-PCAP erneut ausgeführt; Kernzahlen und Fenster stimmen mit der Lieferung überein. Im letzten Fenster [117,120) liegen 68 passende Frames mit 31,903 Hz und maximal 79,106 ms innerer Lücke. Die beobachtete Abdeckung beträgt aber nur 2,100084 s statt mindestens 2,5 s. Das Fenster bleibt abgelehnt; es wurde nicht entfernt, ergänzt oder nachträglich anders bewertet.

Letzte CSI-Ankunft: 119,131778 s nach Beginn, 878,896 ms vor dem Messende. Der Helfer fordert SIGINT nur 0,126 ms nach der Endmarke an und beendet den Prozess innerhalb 4,045 ms. Er enthält keinen ausdrücklichen Nachlauf zum Auslesen letzter Eingangspuffer. Die verwendete Option -U betrifft laut lokalem tcpdump-Handbuch den Dateiausgabepuffer; daraus folgt keine Zusicherung vollständig geleerter BPF-Eingangspuffer.

**Capturepufferung ist eine plausible Erklärung, keine bewiesene Ursache dieses konkreten Endfensters.** Das lokale macOS-BPF-Handbuch beschreibt gepufferte Reads; die [libpcap-Dokumentation zum Abbruch](https://www.tcpdump.org/manpages/pcap_breakloop.3pcap.html) erläutert mögliche noch nicht verarbeitete Pakete. Die aktuelle Upstream-Dokumentation ersetzt keinen Ablaufnachweis der konkret verwendeten Apple-Version. Die gespeicherten Daten allein schließen einen echten Transportaussetzer nicht aus. Auch 0 Kernel-Drops und ein vollständiger PCAP-Tail lösen diese Unterscheidung nicht.

Eine neue Messung soll den Eingangspuffer ausdrücklich berücksichtigen, z.B. mit geprüftem begrenztem Capture-Nachlauf außerhalb des festen Auswerteintervalls. Nachlaufpakete dürfen keine Gutschrift außerhalb dieses Intervalls erhalten. Änderung zunächst mit Fixture für verzögert ausgelieferte, aber innerhalb des Fensters gestempelte Pakete prüfen. Kein rückwirkendes PASS für den alten Lauf und keine RX2-Firmwareänderung aus dieser offenen Messgrenze.

## Nächster Schritt im Gesamtprojekt

Johann bestätigt RX1, RX3 und RX4 ebenfalls als mit Strom versorgt. Das ist Hardwareverfügbarkeit, kein Nachweis ihrer Identität, Firmware oder Empfangsqualität.

**Als Nächstes einen gemeinsamen rein lesenden TX1→RX1–RX4-Basislauf vorbereiten und separat freigeben:** Capture-Endbehandlung in derselben Vorbereitung absichern, alle vier RX eindeutig binden, pro RX Format/Rate/Quelle/Boot getrennt und gemeinsame gültige Fenster über 30 Minuten prüfen. RX2 bleibt auf C; keine weiteren OTA-/Flash-/Reset-/Konfigurationsaktionen aus diesem Vorschlag. Damit lässt sich RX2 zugleich länger bestätigen und die tatsächliche Datenbasis der drei anderen RX feststellen. Kein weiterer RX2-Einzel-Flashzyklus oder pauschaler Offlineaudit.

Physische LTF-/Bin-/Maskenbindung, tatsächliche Serverannahme und Laufzeitressourcen bleiben offen. Der Host-Sync-Fix ist noch nicht aktiviert. RX↔RX-Entwicklung und beide Radare sind eigene Arbeitspakete; der vorgeschlagene Basislauf bestätigt zunächst ausschließlich die vier TX1-Strecken. Vier-RX-/Tracking-Abnahme ist noch nicht erreicht.

Schwierigkeit des nächsten reinen Prüf- und Messpakets: **3/5**. Die wiederholten Installations-/Formatversuche sind berücksichtigt; die C-Installation ist nun abgeschlossen, die Vier-RX-Datenbasis noch unbelegt.

Belege: [Agentenabschluss](../../private/rx2-install-csi-new-20261013-20261013T143100Z/REPORT.private.md), [Originalmanifest](../../private/rx2-install-csi-new-20261013-20261013T143100Z/FINAL-MANIFEST.private.json), Original-PCAP/HTTP-Receipts, Captureproof, Decoder und Diagnoseklammer in derselben unveränderten privaten Ablage.
