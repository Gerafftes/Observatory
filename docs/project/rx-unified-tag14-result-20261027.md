<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX-Vereinheitlichung und Tag14: Ergebnis vom 27.10.2026

Alle vier RX sind mit derselben geprüften App installiert. **Tag14 ist nicht
bestanden:** Im Paarmitschnitt fehlen beide gerichteten Peerlinks. Die
vorgesehene Rückkehr ist abgeschlossen; der neue RX-only-Default empfängt von
allen vier RX wieder passende v4-Daten mit fortschreitenden Zählern.

| Nachweis | Tatsächliches Ergebnis |
|---|---|
| Installation RX1–RX4 | Bestanden: frische Sicherungen, gerätegebundener Appwrite, vollständige Slot-/Schutzbereichsvergleiche und Normalstarts |
| Vier-RX-Q900 | Bestanden; RX4s ergänzter Lauf ausdrücklich freigegeben, RX1–RX3 ohne weiteren Neustart an ihre bestehenden Belege gebunden |
| Short, 30 s | Bestanden: 4939 Rohframes, 23/23 gemeinsame Fenster, PCAP-Abgleich; Endabweichung 42,4165 ms |
| Erste Basis, 180 s | Nicht bestanden: Endabweichung 130,473417 ms; Rohdaten und Negativbefund bleiben erhalten |
| Ausdrücklich freigegebene zusätzliche Basis, 180 s | Bestanden: 27735 Rohframes, je Link und gemeinsam 173/173 Fenster; zwei identische native Audits, PCAP-Abgleich; Endabweichung 20,7735 ms, konservative Uhrunsicherheit 2,493584 ms |
| Paar RX1/RX4, 300 s | Nicht bestanden: 44380 Rohframes und alle vier TX-Links mit 293/293 Fenstern; beide Peerlinks mit null Rohframes und null Fenstern statt mindestens 600 geeigneten Frames und 264/293 Fenstern |
| Paar-Abschluss | Recorder/PCAP sauber geschlossen; Endabweichung 25,1835 ms, Uhrunsicherheit 2,154438 ms; vorhandene Rohpayloads im PCAP nachvollziehbar |
| TX-only-/Default-Rückkehr | Bestanden: RX1/RX4 Generation 3, RX2/RX3 Generation 1, alle ohne Peers; 21,797394125 s tatsächliche Abschlussbeobachtung mit NORMAL-Diagnosen, gleichem Boot/Profil und fortschreitenden Geräte- und Hostzählern |

Die App-Datei-SHA256 aller vier RX lautet
`18c54935a5b99a147755e59cce1aeb471287f9687c9fda37eb0891ae948f1fe8`.
Laufender Image-Digest:
`4c99c8cb290192ac825c8246ae490f8fd7ade5cd5cea77c3450a4816c38d03df`.
Firmwarequellen:
`c8697d6175e84b0535970563f602ecd51465abb054cc5ef4e37cb64c94bbf40d`.
Der neue native Host verwendet
`78c64630e1338a3060243068f4ed1a94944fd43a7d8b76b116f015a8aafb45dc`,
`--rx-session-only` und das versiegelte Return-Setup. Der frühere v3-Default
wurde nicht reaktiviert. Der bezeichnete neue Default bleibt absichtlich aktiv.

Der Peerblocker ist durch gespeicherte Gerätebelege eingegrenzt. Bereits bei
den ersten Paarbereitschaftsbeobachtungen um Bootalter 90 s meldeten RX1 und
RX4 `peer_session_ended=true`, deutlich vor dem 900-s-Limit. Während des
300-s-Mitschnitts kamen auf RX1 weitere 21575 und auf RX4 weitere 20156
geräteinterne Peer-Beobachtungen hinzu. Alle wurden abgewiesen; zugelassene,
erfasste und vom Geräte-OS angenommene Peerframes nahmen jeweils um null zu.
Die Rohsendeversuche blieben bei 13 beziehungsweise 41 stehen. Die gespeicherten
WLAN-Diagnosen zeigen keine Disconnects. Das belegt die bereits beendete
Peersitzung und fehlende Peerweitergabe; es beweist noch nicht deren Auslöser.

Ein möglicher Auslöser ist die Zeitordnung im Peerbudget: `rx_budget` beendet
den Versuch dauerhaft bei einem übergebenen Zeitwert kleiner als `last_us`.
Callback und Transport erfassen Zeitwerte vor dem gemeinsamen Peer-Lock.
Eine Task-/Core-Verschachtelung könnte dadurch eine ältere Beobachtung nach
einer neueren verwenden. Dieser konkrete Live-Auslöser ist **nicht bewiesen**;
es fehlt die erste Fehlerursache mit Budgetzeit, übergebenem Zeitwert und
frischer, unter demselben Lock erfasster Zeit. Dafür liegt eine private
Testkarte vor. Es wurde keine ungeprüfte Firmwarekorrektur installiert.

Die Paarvorprüfung enthielt außerdem eine belegte lokale Prüflücke: Sie
wertete das bereits gesetzte Sitzungsende nicht als Sperre. Diese Lücke ist
jetzt ausschließlich im Hosthelfer geschlossen. Beendete oder unbekannte
endliche Peersitzungen werden bei Qualifikation und vor dem Reservieren einer
Paaraufnahme abgewiesen; lesende Abfragen für eine sichere TX-only-Rückkehr
bleiben möglich. Zwei neue Gegenfälle scheiterten am alten Stand; insgesamt
69 Python-Tests bestanden anschließend. Die neuen Prüfungen weisen auch die
vier tatsächlich gespeicherten problematischen Paarbeobachtungen zurück.
Das ist kein Fix der vorzeitigen Peersitzungsbeendigung und kein nachträglicher
PASS für den bereits ausgeführten Paarlauf.

Zwei weitere lokale Vorbedingungen wurden im Verlauf korrigiert: en0/macOS
unterstützte die verlangte Nanosekunden-PCAP-Präzision nicht; Mikrosekunden
werden jetzt mit konservativer 1000-ns-Quantisierung berücksichtigt. Die erste
Basisplanung addierte die verspätete Startantwort zur festen Dauer; der
ausgeführte korrigierte Planer nutzt die kalibrierte native Startzeit. Die
tatsächliche 50-ms-Endgrenze wurde unverändert beibehalten. Ausgeführte
Messungen sind an ihre jeweils eingefrorenen Laufzeitkopien gebunden; die
letzte Peer-Prüfkorrektur wurde nicht mehr live eingesetzt.

Die nativen Sequenzlücken bleiben separat ausgewiesen: Short 0, gültige Basis
54 und Paar 91. Sie sind kein RF-Verlustnachweis. Der PCAP-Abgleich der
vorhandenen Rohpayloads ersetzt weder die fehlenden Peerlinks noch deren
physische Qualifikation. Das gemeinsame native Dateiauswertungsbudget wurde
mit 2,732009085 s von höchstens 24 s eingehalten. Nach dem ersten negativen
Paar-Audit unterblieb ein weiterer Abnahmelauf; es gibt keine Behauptung zweier
bestandener Paar-Auswertungen.

Schreibansprüche sind verbraucht: RX2 zwei Appwrites, RX1/RX3/RX4 je einer;
RX1/RX4 je ein Paar- und ein Rückkehr-Profilwrite. Keine weiteren Appwrites,
Messwiederholungen oder Flash-Restores wurden abgeleitet. TX1, Radar,
produktives Host-Netz, Gerätesicherheit und fremde Prozesse wurden nicht
verändert. Backups, der schmutzige Arbeitsbaum und negative Nachweise bleiben
erhalten. Der eigene Wachhalteprozess sowie die abgeschlossenen eigenen
Recorder-, PCAP- und Prüfprozesse sind beendet.

Die Beobachtung erfolgte wie beauftragt über begrenzte direkte Begleitung und
geplante Einmal-Aufgaben. Der Paar-Zwischencheck wurde allerdings erst bei
256,713 s statt bei 150 s durchgeführt, also rund 106,7 s verspätet. Diese
Abweichung ist gespeichert; eine pünktliche Halbzeitbeobachtung wird nicht
behauptet. Die feste Aufnahmedauer und Schließung steuerte der eigene
Prüfprozess unabhängig vom später ausgelösten Chat-Check.

Private Belege liegen ausschließlich unter
`PRIVATE_EVIDENCE/rx-unified-live-20261027-run01/amendment-01/`:
`rx4-q900-supplement-01/`, `short-audit-02/`, `baseline-audit-03/`,
`pair-audit-03/`, `PEER-BLOCKER-TEST-CARD.private.json`,
`OFFLINE-PEER-READINESS-GUARD.private.json`, `default-final-20s-01/` und
`OWNED-PROCESS-CLOSURE.private.json`. Geheimnisse, NVS, Sicherungen und Rohdaten
werden nicht in diesen Bericht kopiert.

Ein vollständiger Tag14-Abschluss, eine Firmware-Freeze-Abnahme oder ein
60-Minuten-Stabilitätsnachweis wird nicht behauptet. G/Gesamt bleibt pausiert.
Weitere Gerätewrites oder ein neuer Messversuch benötigen einen neuen,
konkreten Auftrag; alte Ansprüche und das gemeinsame Ende 23:10 Uhr werden
nicht verlängert.
