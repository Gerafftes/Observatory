<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Kurztest nach Peer-Zeitfix und spätere reine Tag14-Aufnahme

**Stand: pausiert auf ausdrücklichen Nutzerwunsch.** Messung und sichere
TX-only-/Default-Rückkehr sind beendet. Für morgen ist nichts geplant.

Alle vier RX verwenden denselben geprüften Kandidaten
`20550def4f3649620bcd39d397a62c4ccf39d064baa4c1b89677648a8691bdb2`.
Der Zeitordnungsfehler wurde am alten Produktionscode reproduziert und mit
frischer Budgetzeit unter dem gemeinsamen Lock korrigiert. Zwei unabhängige
Firmwarebuilds sind bytegleich; C-Suite mit ASan/UBSan und 69 Python-Tests
bestanden. Der zusätzliche, ausdrücklich getrennte Kurzdiagnosepfad besteht
mit 71 Python-Tests und nativen Gegenfällen für echte Masken und fehlende
Peerrichtungen. Das ersetzt keine Live-Peer-Abnahme.

| Gefragter Nachweis | Ergebnis dieses kurzen Versuchs |
|---|---|
| TX1-Layout, Masken und Zeitvertrag | 30-s-Basis bestanden: 4931 Rohframes, 23/23 gemeinsame Fenster, zwei identische native Auswertungen, Endabweichung 31,788584 ms unter 50 ms |
| Basis-Datenbilanz | PCAP/Rohdaten-Abgleich bestanden; sechs Sequenzlücken separat ausgewiesen, kein RF-Verlustnachweis |
| RX1↔RX4 und physische Peerqualifikation | Nicht erreicht: Die Vorbereitung meldete LOCAL_TRANSPORT; deshalb wurde keine neue Paaraufnahme gestartet und kein Peer-Masken-PASS behauptet |
| Fortschritt gegenüber dem vorzeitigen Sitzungsende | Beide Peersitzungen waren im kurzen Vorcheck weiterhin aktiv; Geräte-OS-Annahmen für Peerframes waren positiv (234/244 kumulativ) |
| Verbleibender Blocker | RX1/RX4 meldeten 161/177 abgelaufene Peer-Captures. Zähler allein beweisen keine physische Peerqualität; die kurze Paarfreigabe wurde korrekt gesperrt |
| Rückkehr | Bestanden: alle TX-only, RX1/RX4 Generation 5, RX2/RX3 Generation 1; 21,990089375 s Abschlussbeobachtung mit NORMAL, gleichen Boots/Profilen und fortschreitenden Geräte-/Hostzählern |

Der ursprüngliche lange Q900-Anlauf ist kein bestandener Vier-RX-Nachweis:
RX3 wurde nach seiner ersten Bestätigung noch an den Messplatz umgesetzt und
dabei vom Strom getrennt. Seine alten Bootbelege sind dadurch ungültig.
Anschließend wurde der Auftrag ausdrücklich auf einen kurzen Funktionstest
geändert. Die Kurzdiagnose benutzt eine eigene Bereitschaftsschemata und darf
nicht als Q900 oder vollständige Tag14-Abnahme umetikettiert werden.

Die wiederholt beobachtete SOS-Anzeige war TX1_STALE. Manuelle normale Starts
stellten den Betrieb wieder her; DHCP-Adressen wechselten dabei tatsächlich
zwischen den RX. Der Host bindet sie jetzt über die bekannte physische MAC und
die authentisierte Geräte-ID. Eine Ursache dieser separaten SOS-/Netzstörung
wird dadurch nicht als behoben behauptet. Es gab keine Netz-, TX1-, Radar- oder
Sicherheitsänderungen.

**Nächste Schritte, noch nicht ausgeführt oder geplant:**

1. Den verbleibenden Ablauf-/Transportfehler offline eingrenzen: Welche
   Peer-Captures laufen an der 100-ms-Grenze ab, und sind Budget, Queueablauf
   oder Fehlerklassifikation die Ursache? Gegen den tatsächlich installierten
   Code reproduzieren; keine Alters-, Masken-, Raten- oder Zeitgrenze zum
   Erzeugen eines PASS absenken.
2. Nur falls nötig einen weiteren geprüften Kandidaten vorbereiten. Neue
   Firmwarewrites benötigen neue konkrete Freigaben; alle heutigen Appwrite-
   und Profilansprüche sind verbraucht. Jetzt erfolgen keine weiteren Fixes.
3. Erst nach behobenem beziehungsweise eindeutig qualifiziertem Blocker eine
   **reine Aufnahmesitzung** beauftragen: feste Versionen und Auswerter bereit,
   keine Entwicklung, Builds, Fehlersuche oder spontane Wiederholungen darin.

Für die spätere ursprüngliche Tag14-Abnahme sind die langen Datenblöcke
**180 s Basis und 300 s RX1↔RX4-Paar** maßgeblich. Dazu gehören vorher die
frischen Image-/Source-/Geräte-/Profil-/Geometriebindungen und Q900 im
tatsächlich verwendeten Basisboot: erste Beobachtung 800–850 s, letzte
900–920 s, mindestens 60,2 s VALID-Abstand. Die vorgeschriebene 30-s-Startprüfung
bleibt beim unveränderten Abnahmeplan ein kurzes Eingangstor; sie ist keine
neue Entwicklungsrunde. Die heutigen Kurzdiagnosen können wegen anderer
Boots/Profile und Bereitschaftsart diese Startbelege nicht ersetzen.

Der neue Messauftrag braucht einen konkreten Zeitpunkt und ein neues gemeinsames
Ende, Betriebsrecht für die vorgesehenen Starts/Hostwechsel/Aufnahmen sowie
nur die nötigen eigenen Paar-/Rückkehr-Profilwrites für RX1/RX4. Ohne neue
Firmwareänderung sind weder USB-Installation noch Appwrite erforderlich.
Beim aktuellen Endzustand wären die nächsten Profilgenerationen Basis 5,
Paar 6, Rückkehr 7 auf RX1/RX4; RX2/RX3 bleiben unverändert in Generation 1.
Neue versiegelte Setups müssen dazu aus tatsächlichen Antworten gebunden werden.

Während der reinen Aufnahme bleiben die ursprünglichen Regeln bestehen:
Basis mindestens 156/173 und Paar mindestens 264/293 gültige Fenster, beide
gerichteten Peerlinks mindestens 600 geeignete Frames und 2 Hz, echte
HT-TX-128-/Non-HT-Peer-64-Masken, 50-ms-End- und Uhrvertrag, vollständige
Geräte-/Host-/PCAP-Bilanz und zwei deterministische native Auswertungen im
gebundenen Dateibudget. Anschließend TX-only-/Default-Rückkehr und mindestens
20 s echter Abschlussnachweis. Ein 60-Minuten-Stabilitätslauf ist ein eigener
Umfang und wird nicht automatisch als Teil dieser 180-/300-s-Abnahme gestartet.

Private Belege liegen unter
`PRIVATE_EVIDENCE/rx-unified-live-20261027-run01/amendment-02/`, insbesondere
`functional-baseline-capture-01/`, `functional-baseline-audit-01/`,
`FUNCTIONAL-PAIR-BLOCKER.private.json`, `return-profile-transition-01/` und
`default-final-20s-01/`. Rohdaten, Sicherungen, NVS und Geheimnisse bleiben privat.
Der bezeichnete neue Default bleibt aktiv; weitere Messungen und Automationen
sind nicht angesetzt. G/Gesamt und der übrige Auftrag bleiben pausiert.
