<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Koordinatorreview Paket 1: Auswertung und Erfassungsübergabe

**Paket 1 ist teilweise bestätigt. Die Radarberechnung und historische Regression bestehen; die neue RX-Auswertung ist wegen zweier direkt reproduzierter Abweichungen NICHT_READY. Die Erfassungsübergabe benennt reale Lücken und ist noch keine vollständige Aufnahmevorbereitung.** Keine Hardware-, Geräte-/Netzabfrage, Installation, Hostaktivierung oder Agentenbeauftragung durch diesen Review.

Lieferung: `./private/abendlaeufe-auswertung-offline-20261015T120658Z/`. Abschlussbericht, tatsächliche Quellen, Verträge, Übergabe und abgeschlossener Agententurn wurden gelesen. Private Reviewreceipts: `./private/paket1-auswertung-koordinatorreview-20261015/`. Originale und versiegelte Lieferung erhalten.

## Getrennte Urteile

| Bereich | Koordinatorurteil |
|---|---|
| RX-Basis-/Peerauswertung | **NICHT_READY**: Layout-Rückkehr wird dauerhaft ausgeschlossen; der Eingangsvertrag kann unterschiedliche TX1-/Peerraster desselben RX nicht getrennt vorgeben |
| Radar-G2-Berechnung | **READY_OFFLINE für gespeicherte vollständige Eingaben**; keine reale G2-Abnahme |
| Historische Vier-RX-Regression | **BESTANDEN**, ausgewählt 1.786/1.793, streng 1.780/1.793 unabhängig erneut reproduziert |
| Erfassungsübergabe | **KONKRETE_BELEGLÜCKEN**, korrekt offen ausgewiesen; bestehende Capturewege erzeugen nicht alle geforderten Dateien/Zeitbelege |
| Originale/Kandidaten | **ERHALTEN**; ältere B1–B4- und Recorderkorrektur bleiben geschlossen |

## Zwei bestätigte RX-Befunde

### 1. Gültige Layout-Rückkehr wird erneut dauerhaft pensioniert

In `scripts/wifi.py` werden verlassene Layouts in `retired_layouts` gespeichert. `returned=layout in retired_layouts` geht anschließend dauerhaft negativ in `suitable` ein. Nach 128→64→128 bleiben wieder zum deklarierten Raster passende, physisch gültige Frames ausgeschlossen. Dies widerspricht dem bereits geschlossenen Layout-Rückkehrvertrag; diese neue Auswertung darf die alte Regel nicht wieder einführen.

Den gespeicherten `layout-return`-Eingang durch die aktuelle reale Datei-CLI erneut ausgewertet: RX2-Basis nur **600 geeignete Frames**, **93/473 Fenster**, **2.280 Formatausschlüsse**. Darunter **2.274 tatsächlich wieder passende und vom aktuellen Rustparser physisch zugelassene Rückkehrframes**, die allein wegen der Pensionierung keine Gutschrift erhalten. Die mitgelieferte Regression erwartet für diesen Fall derzeit FAIL und bestätigt damit die falsche Wirkung, statt die Erholung zu prüfen.

Gezielte Korrektur: Boot-/Quellen-/Konfigurationssicherheit erhalten, Übergangsfenster begrenzen, legitime Rückkehr desselben gültigen Formats wieder zulassen. Regression muss Erholung nach der lokalen Übergangsgrenze prüfen. Keine Vermischung von Layouts oder Lockerung der physischen Maske.

### 2. TX1 und Peer benötigen getrennte Formatverträge

`grid` und `physical_key` werden in `scripts/wifi.py` für TX1- und Peerframes beide aus derselben Empfängerreferenz gelesen. Das Eingangsformat besitzt dafür keine eigene gerichtete Peerreferenz. Damit lässt sich z. B. TX1 mit 128 IQ-Paaren und ein zulässiger LLTF-Peerlink mit 64 IQ-Paaren am selben RX nicht gleichzeitig passend deklarieren. Das ist eine Vertragslücke, kein Beleg für eine reale ungeeignete Funkstrecke.

Neuen rein synthetischen Gesamteingang mit unveränderter 128er-TX1-Basis und Peerframes aus der tatsächlich geprüften C-LLTF-Vorlage gebaut. Der aktuelle Rustparser akzeptiert diese 64er-Peerframes samt physischer Gültigkeit. Alle übrigen Identitäts-/Quellen-/Zeit-/Dateibelege blieben konsistent. Tatsächliche CLI: **je Richtung 1.920 Transportframes, 1.920 Formatausschlüsse, null geeignete Peerframes und null Fenster**, Gesamt FAIL. Der bisherige gute Gesamttest nutzt für TX1 und Peer dieselbe 128er-Vorlage und deckt diese Kombination nicht ab. Der neue synthetische Eingang samt 23 hashgeprüften Datendateien und beiden Ergebnisberichten ist in der privaten Koordinatorreviewablage dauerhaft erhalten; die unveränderte `layout-return`-Eingangsdatei ist im Reviewreceipt genau referenziert.

Gezielte Korrektur: pro gerichteter Strecke einen expliziten gebundenen Raster-/Physicalvertrag ermöglichen und verwenden. TX1-128er-Vertrag unverändert erhalten. Keine pauschale Annahme aller Peerlayouts und keine Formatwahl nach Datenansicht. Fehlende/unbekannte Streckenreferenz bleibt ein eigener offener Befund. Vollständigen gemischten 128er-TX1-/64er-Peerlauf durch Parser und CLI regressieren.

## Unabhängige Wiederholung und Erhaltung

- Netzgesperrter Nur-Lese-Verifier: **4.343 Lieferdateien und 3.642 Originaldateien**, keine Abweichung. Gespeicherter Erhaltungsnachweis zusätzlich acht Originalsymlinks und 52 geschützte Arbeitsbaumdateien; Gitstatus wurde vor den Koordinator-Dokumentationsänderungen unverändert bestätigt.
- Fünf tatsächliche Datei-CLI-Replays in neuen geschützten temporären Outputs: gutes RX-Beispiel PASS, gutes Radarbeispiel PASS, historische Rechnung exakt PASS; beide unvollständigen REAL-Vorlagen korrekt NICHT_BEURTEILBAR. Historische Originalausgaben nicht überschrieben.
- Zwei zusätzliche oben beschriebene Gesamtrepros durch aktuelle Parser/CLI ausgeführt. Sie begründen die abweichende RX-Bereitschaft trotz der gespeicherten 21 Testgruppen. Die 48 berichteten CLI-Auswertungen und 34 C-/Rustkontrollen sind Lieferbelege, keine heute vollständig wiederholte Suite.
- Alle wiederholten Auswerteprozesse liefen unter OS-Netzverbot; echte Python-Netz-/Prozess-/Credential-/Gerätewege sind im gelieferten Dateieinstieg gesperrt, lediglich der hashgebundene lokale stdin-Exporter läuft. Keine Produktivserver gestartet. Keine neuen SDK-/Hostbuilds durch den Koordinator.

Die Roh-/Raumdistanzstatistik ist getrennt, leere Statistik ergibt JSON-null und die festen Nenner der Fenster bleiben erhalten. Radarquellen/-epochen, Transformation, unabhängige Einzelqualifikation und konservative gemeinsame Zuordnung werden am vorhandenen Rustpfad ausgewertet. Der Planbezug für die Rohdistanzgrenzen ist ausdrücklich dokumentiert und darf später nicht anhand der Messwerte geändert werden. Ein synthetischer PASS ist keine physische Referenzqualifikation.

## Erfassungsübergabe und nächster Schritt

Die Übergabe ist als **Lückenliste brauchbar**, nicht als vollständig verdrahteter Erfassungsweg. Besonders fehlen:

- dauerhaft gespeicherte exakte Messgrenzen und belegte Zuordnung der Adapter-/Serveruhren;
- ursprüngliche Recorderantworten, getrennte Start-/Endmetadaten und gebundene TX1-Rohdateien in der Laufübergabe;
- vollständige Radar-Rohpaket-/Quellen-/Hostzeitreihen mit tatsächlicher Annahme-/Verarbeitungsbindung;
- für weitergehende RX-Frischebehauptungen korrelierte Host-/Firmwarezeitbelege. Die vorhandene Firmware liefert diese nicht vollständig; Aggregate oder Quelltests ersetzen sie nicht.

**Nächster konkreter Schritt ist ein gezielter Offlineauftrag für die zwei RX-Auswertebefunde und die minimale Erfassungsanbindung.** Vor einem längeren Hardwarelauf muss feststehen, welche Pflichtbelege der vorhandene Weg tatsächlich speichert, welche kleinen lokalen Datei-/Traceexporte ergänzt werden und welche weitergehenden Firmware-/Ressourcenaussagen ausdrücklich unbekannt bleiben. Keine neue Firmwareinstallation allein aus der Auswertelücke ableiten. Vorhandene gültige Teilnachweise und unbekannte weitergehende Diagnostik getrennt halten. Keine Werte wie `server_admitted=true` manuell zum Lückenfüllen einsetzen.

Für die spätere getrennte Radar-G2-Aufnahme: beide LD2450 mit MMWAVE1/MMWAVE2, stabile Stromversorgung, Mac im bestehenden Projektweg, Maßband/Bodenmarken und eine Person. Für den späteren RX-Paarvergleich zusätzlich TX1 und RX1–RX4 an ihren festen Positionen, normaler Strom und der gebundene Mac-Host. Die bisherigen Pläne haben bis zu 110 bzw. 100 Minuten Budget und eigene neue Freigaben; keine Zusage beider vollständiger Läufe innerhalb ein bis zwei Stunden.

Erst nach Korrektur-/Erfassungsreview den konkreten freigegebenen Hardwareauftrag ausführen. Reines Anschließen erzeugt die fehlenden Belege nicht. Keine neue reale Radar-, Funk-, Ressourcen- oder Positionsabnahme und kein weiteres Tageshäkchen aus Paket 1. Die zwei von fünf erledigten Punkten des 14. Oktober bleiben erhalten: CSI-Lauf und vorhandene technische Adaptervorbereitung. Dieser neue Auswertefehler ist kein Wiederaufrollen der geschlossenen Firmware-/Recorderbefunde.

**Schwierigkeit des nächsten gebündelten Offline-/Erfassungsschritts: 4/5**, unter Berücksichtigung der bisherigen Korrekturschleifen und der nötigen Abstimmung zwischen gespeicherten Daten und tatsächlich erzeugten Belegen. Kein Agent gestartet.
