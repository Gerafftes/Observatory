<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Neuer RX2 Installationslauf nach wiederhergestellter Hostverbindung

Tag 2, 13. Oktober 2026. Schwierigkeit des Installations-/Messlaufs: **4/5**.
Johann bestätigt die Verbindungen; der rein lokale Koordinatorcheck bestätigt die konfigurierte Host-Zieladresse auf en0 und das vorgesehene IPv4-Projektnetz. Kein passiver Eintrag der erwarteten RX2-MAC war vorhanden. Keine Geräteabfrage oder Netzänderung wurde ausgeführt.

Der vorherige Lauf ist mit 0 GETs, 0 POSTs und 0 Captures abgeschlossen. Seine Ablage und Marker bleiben unverändert. Dieser neue Auftrag verwendet die vorbereiteten lokalen Helfer nach gezielter Anpassung; keine erneute allgemeine Offlineprüfung oder Firmwareentwicklung.

```text
AUFTRAG: Neuer begrenzter RX2-C-Installations-/120-s-CSI-Lauf

Projekt: .

Lies den vollständigen Grundauftrag:
 docs/project/agent-auftrag-rx2-installation-csi-vergleich-20261013.md
und den tatsächlich ohne Gerätezugriff beendeten Vorlauf:
 private/rx2-install-csi-20261013-20261013T052239Z/REPORT.private.md

Alle Grenzen, Kandidatenhashes, Zeit-/Requestbudgets, Prüfungen und
Abschlusskriterien des Grundauftrags gelten. Nur die folgende initiale
Zuordnungsregel ersetzt dessen Verbot jedes Requests ohne Nachbarcache.
Keine weiteren Agenten starten oder Chats anschreiben.

Freigaben A/B/C für diesen neuen Lauf direkt aus Johanns Startprompt binden.
Vorher keine Geräteanfrage. Höchstens 20 Minuten ab erstem aktiven Request,
ein App-OTA-POST ausschließlich mit Kandidat C, 20 GETs insgesamt und ein
passiver 120-s-Capture. Kein serieller Zugriff/Reset/Restore/Hostdienstumbau.

1. Neue private Ablage verwenden; alten Laufmarker nicht löschen oder
   umgehen. Vorbereitete Helfer und 57 gespeicherte Offlinechecks gezielt
   wiederverwenden. C-Dateihash und lokale Eingänge frisch binden. Keine
   unveränderten Helfer mit alten OUT-/Freigabe-/Verbrauchspfaden starten.

2. Ausschließlich lokal erneut prüfen: konfiguriertes Hostziel vorhanden,
   richtige Projekt-IPv4-Verbindung/Route und bekannter RX2-Endpunkt aus
   gebundenem reference.private.json, keine beliebige Adresse oder Subnetzsuche.
   Bei fehlendem Hostpfad stoppen. Besteht bereits eine widersprüchliche
   MAC-Zuordnung am Ziel, keine Anfrage senden.

3. Falls nur der Nachbarcacheeintrag fehlt, ist genau EIN unauthentisierter
   GET /ota/status an den einzigen historisch gebundenen RX2-Endpunkt erlaubt.
   Keine Authorization, Cookies oder sonstige Zugangsdaten mitsenden.
   Kein Redirect/Proxy/Retry, keine ARP-Probe und kein Scan. Connect maximal
   3 s, gesamter Request maximal 7 s, Antwort maximal 16 KiB wie Grundauftrag.
   Die normale Adressauflösung dieser TCP-Verbindung darf stattfinden.
   Dieser GET ist bereits der erste der drei Vorher-GETs und zählt zum
   20-GET-/20-Minuten-Budget; kein zusätzlicher Discovery-Request.

4. Unmittelbar nach diesem ersten GET passive Zuordnung prüfen: erwartete
   RX2-MAC am angefragten Endpunkt und passenden Interface. Status muss
   zusätzlich Node RX2, plausiblen Boot, bekannten vollen Image-Digest sowie
   passenden Slot-/OTA-Zustand liefern. Fehlende Antwort/Zuordnung oder
   Widerspruch endet vor Authentisierung und Schreiben. Cache allein oder
   bloßer Node-Name allein genügt nicht. Fehlender Cache ist kein Ausfallbeweis.

5. Erst nach dieser bestätigten Zuordnung /config authentisiert und den
   zweiten /ota/status lesen. Danach den Grundauftrag fortführen: Config-
   und Sicherungs-/Slotprüfung, höchstens ein C-Upload, Endboot/VALID/Source
   bestätigen, genau ein 120-s-Fenster und getrennte Ergebnisbilanz.
   Vor jeder weiteren authentisierten Anfrage/POST aktuelle Bindung prüfen.
   Der initiale Request ist keine Ausnahme für einen ungeprüften Upload.

Vor Livebeginn den geänderten Zuordnungszweig mit Doubles prüfen: leerer
Cache + richtige Statusantwort/MAC erlaubt Fortsetzung; falsche MAC, fremder
Status, Timeout und weiterhin fehlende Bindung stoppen ohne Token/POST;
Requestzähler und Einmalmarker stimmen. Bestehende erfolgreiche Tests
übernehmen, nur berührte Helferpfade erneut prüfen.

Identische C-App wie im Grundauftrag:
 private/tag2-nullschutz-sync-offline-20261013-v1/build-C/rx/esp32-csi-node.bin
 BIN-SHA256:
 b896d45ad992623f6d56cadc9927824a7869a989d05232f28ad1c5f5ae39ce9e

Keine eigenmächtige Netz-/RX2-Konfigurationsanpassung. Kein zweiter Upload,
Capture, Restore oder zusätzlicher Gerätezugriff nach Abschluss. Installation,
aktuellen Endlaufzustand, Capture und Formatvergleich getrennt berichten;
keinen frühen Stop als gemessene Nullrate ausgeben. Danach stoppen.
```
