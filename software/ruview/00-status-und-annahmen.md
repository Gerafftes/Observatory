# 00 Status und Annahmen

## Ziel der aktuellen Phase

Die aktuelle Phase soll aus dem stabilen 1TX-/4RX-Datenempfang eine belastbare Live-Auswertung entwickeln. Ziel ist:

- CSI-Daten von mehreren ESP32-RX-Nodes empfangen
- leeren Raum, still sitzende Person und Bewegung reproduzierbar unterscheiden
- eine ruhende Person stabil und eine Positionsänderung nachvollziehbar darstellen
- erst nach sauberem CSI-Datenstrom ruhige Atmung experimentell testen
- mmWave erst in einer späteren Phase als Referenz ergänzen

## Hardware aktuell

| Gerät | Rolle im ersten Test | Status |
|---|---|---|
| ESP32 #1 | kontrollierter WLAN-Sender / TX | Aktive Arduino-SoftAP-App zerstörungsfrei ausgelesen und per SHA-256 identifiziert; wieder ausschließlich stromversorgt; versiegelter Preflight bestanden |
| ESP32 #2 | RuView CSI-Empfänger RX1 | Firmware 0.7.0; Einzelboard-Boot, gemeinsamer Binding-Lauf und korrigierte 25-s-Discovery bestanden |
| ESP32 #3 | RuView CSI-Empfänger RX2 | Firmware 0.7.0; Einzelboard-Boot, gemeinsamer Binding-Lauf und korrigierte 25-s-Discovery bestanden |
| ESP32 #4 | RuView CSI-Empfänger RX3 | Firmware 0.7.0; Einzelboard-Boot, gemeinsamer Binding-Lauf und korrigierte 25-s-Discovery bestanden |
| ESP32 #5 | RuView CSI-Empfänger RX4 | Firmware 0.7.0; Einzelboard-Boot, gemeinsamer Binding-Lauf und korrigierte 25-s-Discovery bestanden |
| mmWave-Modul | spätere Referenz für Presence/Atmung/Distanz | vorhanden, aktuell bewusst nicht verwendet |
| Laptop/PC | Server, Logging, Dashboard | CSI-WLAN-Adresse `CSI_HOST_IP` und gemeinsamer Empfang von RX1 bis RX4 live bestätigt; korrigierter Serverbuild vom 2026-08-09 liegt vor |

## Annahmen

- Alle fünf ESP32-Boards sind als ESP32-S3 mit 16 MB Flash und 8 MB PSRAM
  bestätigt.
- Die ersten Tests laufen auf 2,4 GHz.
- Der Laptop/PC befindet sich im selben Netzwerk wie die ESP32-RX-Nodes.
- Für den TX wird eine kleine separate SoftAP-/Sender-Firmware verwendet.
- Für RX1 bis RX4 wird RuView `firmware/esp32-csi-node` verwendet.
- Das mmWave-Modul wird zuerst separat als Referenz betrachtet, nicht als Teil des WLAN-CSI-Systems.

## Fester Raumaufbau vom 2026-07-18

Raummaße:

- Länge: `4,02 m`
- Breite: `3,44 m`
- Höhe: `2,59 m`

Gemessene Positionen in der ursprünglichen Notation `(Breite, Länge, Höhe)`:

| Gerät | Breite | Länge | Höhe |
|---|---:|---:|---:|
| TX | 3,05 m | 2,51 m | 1,19 m |
| RX1 | 3,16 m | 4,02 m | 0,50 m |
| RX2 | 2,47 m | 0,00 m | 0,87 m |
| RX3 | 1,33 m | 4,02 m | 0,74 m |
| RX4 | 0,98 m | 0,00 m | 0,87 m |
| Mac | 0,94 m | 0,00 m | 0,87 m |

Für RuView wurden die Koordinaten in `(x=Länge, y=Höhe, z=Breite)` überführt und entsprechend der gewählten Ansicht gespiegelt:

| Gerät | RuView-Koordinate `[x, y, z]` |
|---|---|
| RX1 | `[0.00, 0.50, 0.28]` |
| RX2 | `[4.02, 0.87, 0.97]` |
| RX3 | `[0.00, 0.74, 2.11]` |
| RX4 | `[4.02, 0.87, 2.46]` |
| TX | `[1.51, 1.19, 0.39]` |
| Mac | `[4.02, 0.87, 2.50]` |

Diese Werte sind untereinander konsistent: `x = 4,02 m - Länge` und `z = 3,44 m - Breite`.

Der Mac-Bezugspunkt ist die Mitte des Unterteils. Er steht auf gleicher Höhe
wie RX4 und 4 cm von RX4 entfernt auf der von RX2 wegführenden Linie. Für die
D6-Serie bleibt die Tür geschlossen. Diese Mac-Position ersetzt für das neue
Setup den historischen Aufbau „Mac mittig“; alte Leerraumreferenzen werden
nicht wiederverwendet.

### Festgelegte Punkte für die diskrete Positionsprüfung

| Punkt | RuView-Koordinate `[x, y, z]` |
|---|---|
| P01 | `[0.75, 0.00, 0.75]` |
| P02 | `[2.01, 0.00, 0.75]` |
| P03 | `[3.27, 0.00, 0.75]` |
| P04 | `[0.75, 0.00, 1.72]` |
| P05 | `[2.01, 0.00, 1.72]` |
| P06 | `[3.27, 0.00, 1.72]` |
| P07 | `[0.75, 0.00, 2.69]` |
| P08 | `[2.01, 0.00, 2.69]` |
| P09 | `[3.27, 0.00, 2.69]` |

Das Positionsmodell darf ausschließlich einen dieser Punkte, `unknown` oder
`ambiguous` ausgeben. Es interpoliert keine scheinpräzise Koordinate zwischen
ungemessenen Punkten.

## RuView-Rolle

RuView wird für diese Teile verwendet:

- ESP32-RX-Firmware zur CSI-Erfassung
- UDP-Streaming der CSI-Daten
- Sensing-Server auf Laptop/PC
- einfache Live-Visualisierung
- erste Presence-/Vitals-Heuristiken

RuView wird im ersten Test nicht als fertige wissenschaftliche Auswertung übernommen. Die Rohdaten und Referenzwerte müssen zusätzlich dokumentiert und später selbst ausgewertet werden.

## Noch offen

- Neue 65-Sekunden-Leerraumkalibrierung ohne Person unter
  `setup-0a49d75f122f9dc9` aufnehmen
- Training an P01 bis P09 aufnehmen und daraus den Positionsindex bauen
- Drei blinde Leerraumtests und je zwei neue blinde Aufnahmen für P01 bis P09
  erfassen; Classification und Position getrennt bewerten
- Live-Positionsanzeige erst aktivieren, wenn beide Qualitätsgrenzen für
  dasselbe versiegelte Setup bestanden sind
- Genaues mmWave-Modul und dessen Schnittstelle: USB oder UART

Die D6-Setupaufnahme und TX-Firmwareinventur vom 2026-08-09 ist unter
[results/2026-08-09_D6_setupaufnahme-und-TX-firmwareidentitaet.md](results/2026-08-09_D6_setupaufnahme-und-TX-firmwareidentitaet.md)
dokumentiert. Das Setup ist dort ausdrücklich noch nicht als versiegelt
markiert; auch der Preflight wurde noch nicht durchgeführt.

Dieser historische Grenzstand wurde anschließend abgeschlossen. Das Setup ist
nun als `setup-0a49d75f122f9dc9` versiegelt; der 25-Sekunden-Preflight bestand
mit 2.545 Frames und 0 Drops. Vollständiger Nachweis:
[results/2026-08-09_D6_setup-siegel-und-preflight.md](results/2026-08-09_D6_setup-siegel-und-preflight.md).

## Aktueller Stand nach dem D5-Livetest vom 2026-07-26

- Der TX-MAC-Filter ist inzwischen auf RX1 bis RX4 aktiv.
- D4 reduziert die zuvor dominierenden groben Bewegungs-Fehlalarme, löst aber die Still-Präsenz-Klassifikation nicht.
- D5 ergänzt per-RX-Leerraumreferenzen, 10-Sekunden-Fenster und ein absolutes Zwei-RX-Quorum.
- Das D5-Offline-Replay war auf zwei historischen Laufpaaren positiv.
- Eine neue reale D5-Leerraumkalibrierung wurde erfolgreich aktiv; alle vier RX hatten anschließend Referenz und aktuelle Evidenz.
- Der anschließende reale Still-Livetest wurde nicht bestanden: 350 von 350 Samples blieben global `ABSENT`.
- Im ersten Abschnitt reagierte zeitweise nur RX4, im zweiten durchgehend nur RX3. Zwei gleichzeitig ausreichend lange zustimmende RX traten nicht auf.
- D5 ist deshalb weiterhin experimentell und nicht als Standard aktiviert.

Der nächste belastbare Entscheidungsschritt ist eine zusammengehörige blinde Testserie unter derselben Kalibrierung:

1. leerer Raum,
2. still sitzende Person an der bisherigen Position,
3. mindestens eine weitere Still-Position,
4. erst danach eine Änderung der Fusion oder Schwellen.

Ausführliche Auswertung: [results/2026-07-26_D5_realer-still-livetest.md](results/2026-07-26_D5_realer-still-livetest.md)

## Aktueller Entwicklungsstand vom 2026-08-01

Nach dem fehlgeschlagenen D5-Livetest wird Classification nicht durch eine
isolierte Schwellenänderung weitergetrieben. Stattdessen entsteht eine
verlustfreie, aufbaugebundene Offlinepipeline, die Präsenz und neun feste
Positionspunkte getrennt bewertet.

RX1 bis RX4 haben Rollout und Einzelboard-Bootprüfung bestanden. Der TX hat
die zerstörungsfreie Inventur und den stabilen SoftAP-Boot bestanden. Am
2026-08-09 wurden die CSI-Verbindung des Macs und der gemeinsame Empfang von
RX1 bis RX4 nachgewiesen. Die Offlinepipeline, die versiegelte
Setupbindung, der fail-closed Livepfad, die geschützte Rohdatenerfassung und die Sensing-Anzeige sind
softwareseitig zusammengeführt. Gezielte Server-, Parser-, Firmware-Host-,
Runner- und UI-Prüfungen sind bestanden. Die aktuelle Rust-Matrix über Server,
Hardware, CLI und Pointcloud umfasst `1.118` bestandene, `0` fehlgeschlagene und
`3` bewusst ignorierte Tests. Diese Softwarezahl ist kein Nachweis realer
Erkennungs- oder Positionsgüte.

Bei versiegeltem Setup akzeptiert der Server nur Raw-CSI mit einem frischen,
vollständig passenden Laufzeit-TX-Nachweis jedes RX. Ein fehlender,
unvollständiger, falsch gebundener oder veralteter Nachweis wird vor Liveness,
Classification, D4/D5/D6, Position und Recorder abgelehnt. Der Server gibt über
`/api/v1/nodes` nur Zustandsflags und das Alter des Nachweises aus, nicht die
rohe TX-MAC oder ihren Hash. Der Nachweis belegt die kontrollierte
Firmwareausführung, ist aber keine kryptographische Geräteauthentisierung gegen
einen Angreifer.

Discovery ist nur eine unversiegelte Inventur und kein Mess-PASS. Vor dem
Versiegeln werden die dauerhaft beschrifteten physischen RX1 bis RX4 sichtbar
gegen ihre dokumentierten Koordinaten geprüft; eine selbst gemeldete RX-ID kann
eine physische Vertauschung nicht automatisch erkennen.

Reale Messungen beginnen erst, nachdem der Mac an seiner normalen
Betriebsposition steht und dieser vollständige Alltagsaufbau im Setup-Artefakt
festgehalten wurde. Mac, Kabel, Möbel und andere statische Gegenstände gehören
zur Leerraumreferenz; „leer“ bedeutet nur „ohne Person“. Ändert sich diese
statische Umgebung nach der Kalibrierung wesentlich, werden Referenz und
gegebenenfalls Positionsindex neu aufgenommen. Ohne realen, blind validierten
Index bleibt der Livepfad absichtlich `uncalibrated`.

ESP-IDF v5.4 ist inzwischen lokal unter `.toolchains/` installiert. Die
aktuelle Firmware 0.7.0 wurde als ESP32-S3-8-MB- und ESP32-S3-4-MB-Variante
erfolgreich kompiliert; auch der CI-Forschungstarget ESP32-C6 bestand den
Target-Build. Die geprüften S3-Flashartefakte liegen außerhalb des Git-
Repositories unter `artifacts/ruview-firmware-0.7.0-2026-08-01/` samt
SHA-256-Prüfsummen. RX1 bis RX4 wurden jeweils als ESP32-S3 mit 16 MB
physischem Flash erkannt, absichtlich mit dem geprüften 8-MB-Layout geflasht
und erfolgreich gebootet. Node-IDs 1 bis 4, Kanal 6, Edge-Tier 0 und aktiver
TX-Filter blieben im jeweiligen NVS erhalten. Weil TX beziehungsweise CSI-AP dabei
ausgeschaltet waren, war der WLAN-Verbindungsfehler erwartbar und noch kein
Live-CSI-Test. Der TX wurde anschließend ohne Schreibzugriff als ESP32-S3 mit
16 MB Flash und 8 MB PSRAM inventarisiert. Seine unveränderte Senderfirmware
bootete stabil, startete den SoftAP auf `CSI_AP_IP` und zeigte weder Brownout
noch Reset-Schleife. Der Mac erhielt per DHCP zunächst `CSI_NODE_IP_2`; Gateway
und Broadcastempfang wurden damit bestätigt. Anschließend wurde das
CSI-Interface passend zu RX1 bis RX4 wieder auf `CSI_HOST_IP` gesetzt. In
einem 10-Sekunden-Lauf kamen 32-Byte-Pakete mit `45,5 Hz` an. Der gemeinsame
Livebetrieb muss Kanal und per-RX-Datenqualität noch bestätigen; ein regulärer
TX-Flash ist nicht vorgesehen.

Der TX bleibt auf seiner separaten Arduino-SoftAP-Firmware und darf nicht mit
`esp32-csi-node` 0.7.0 überschrieben werden. Die neue Quellbindung benötigt
keine TX-Änderung: RX1 bis RX4 prüfen die TX-AP-Identität und erzeugen den
Laufzeitnachweis selbst. Vor jedem nur im Fehlerfall erwogenen TX-Reflash wird
zuerst sein vollständiger aktueller Flash privat gesichert.

Der aktuell versiegelte Release-Server für die reale Serie liegt
read/execute-only unter
`artifacts/live-position-2026-08-09-sidecar-fix/sensing-server`. Er ist
`5.954.256` Byte groß und besitzt SHA-256
`6554c5101bc7e920e9ce52ea5d845d2afd62b97f09d0c31917d1b1b61d14f8b5`.
Er löst den Build `live-position-2026-08-09` ab, dessen Recorder gültige Daten
erzeugte, dessen Offline-Inspektor aber legitime Sidecarfelder ablehnte. Das
aktive Setup ist `setup-2beda4496ccfb547`; der neue versiegelte Preflight
bestand mit 2.701 Frames und 0 Drops. Ältere Builds und Siegel bleiben als
Historie erhalten, sind aber für die neue zusammenhängende Messserie gesperrt.
Jede weitere Codeänderung erfordert wieder ein neues Artefakt, ein neues
Setup-Siegel und eine vollständig neue Messserie.

Verbindlicher Wiedereinstieg und offene Schritte:
[`08-aktueller-arbeitsstand-d6-und-position.md`](08-aktueller-arbeitsstand-d6-und-position.md)
