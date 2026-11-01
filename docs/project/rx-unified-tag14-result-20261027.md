# RX-Vereinheitlichung und Tag14: Ergebnis vom 27.10.2026

Alle vier RX verwendeten dieselbe geprüfte App. **Der Tag14-Paarversuch ist
nicht bestanden:** Im Paarmitschnitt fehlen beide gerichteten Peerlinks.
Die anschließende TX-only-Rückkehr empfing wieder passende Daten aller vier RX.
Dies ist ein historischer Befund; die späteren [Ergebnisse vom 29. Oktober](rx-unified-tag14-result-20261029.md)
werden getrennt ausgewiesen.

## Messergebnisse

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


## Eingrenzung des Peerfehlers

Bereits vor dem Paarmitschnitt meldeten RX1/RX4 eine beendete Peersitzung,
obwohl die 900-s-Grenze noch nicht erreicht war. Während der 300 s kamen
21.575 beziehungsweise 20.156 geräteinterne Peer-Beobachtungen hinzu; keine
wurde an den Host weitergegeben. Die WLAN-Diagnosen zeigten keine Disconnects.
Diese Beobachtung grenzt den Fehler ein, beweist aber noch nicht den konkreten
Auslöser des ersten Liveabbruchs. Die [Offline-Reproduktion der Zeitordnung](rx-unified-peer-clock-fix-20261027.md)
beschreibt die anschließende technische Untersuchung.

Die Paarvorprüfung erkannte das gesetzte Sitzungsende zunächst nicht als Sperre.
Die entsprechende Hostprüfung wurde mit negativen Gegenfällen ergänzt;
69 Python-Tests bestanden. Das macht den bereits fehlgeschlagenen Paarlauf
nicht nachträglich erfolgreich.

## Grenzen

Die Sequenzlücken betrugen 0 im Shortlauf, 54 in der gültigen Basis und 91 im
Paarlauf. Der Raw-/PCAP-Abgleich vorhandener Payloads ersetzt weder die
fehlenden Peerlinks noch eine Positionsqualifikation. Es wird kein vollständiger
Tag14-Abschluss oder 60-Minuten-Stabilitätsnachweis für diesen Lauf behauptet.
Die Originalaufzeichnungen sind nicht Bestandteil des öffentlichen Repositorys.
