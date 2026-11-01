<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Agent-Auftrag: geprüfte BLL-Cacheliste bereinigen – Phase B

Projekt: `.`

Führe diesen begrenzten Auftrag vollständig aus. Starte keine weiteren Agenten, schreibe keine anderen Chats an und richte keine Automation ein. Nach Abschluss berichten und stoppen. **Keine Hardwarezugriffe oder Hostaktivierung.**

## Ziel und ausdrückliche Freigabe

Johann hat für diesen Auftrag direkt freigegeben:

> Phase B für die geprüfte Löschliste bis 70 GiB freiem Platz ist freigegeben. Belege und Programme erhalten, keine Hardwarezugriffe.

Diese Freigabe erlaubt ausschließlich die unten gebundene endliche Liste in ihrer festen Reihenfolge. Sie ersetzt für diese zwei Einträge die bisherige Vorgabe „Noch nichts löschen“. Andere Bereiche bleiben geschützt. Nach erfüllten technischen Gates selbstständig fortfahren; keine erneute allgemeine Bestätigung zwischen den beiden freigegebenen Einträgen einholen.

**Das ist der Fehler beziehungsweise die offene Aufgabe:** Die geprüfte Aufnahme-/Auswertungskette verlangt 70.922.518.528 Byte = 66,051742554 GiB freien Platz. Beim Koordinatorreview am 16. Oktober um 11:04:47 Uhr waren 60,363735 GiB frei. Diese Beobachtung ist historisch. Der Messlauf wartet auf ausreichend Platz und seine separate Liveübergabe.

**Das erwarte ich:** Nur die erforderlichen freigegebenen Compiler-Zwischenstände entfernen, nach jedem Eintrag tatsächlich freie Bytes messen und spätestens bei erreichtem Stopziel weitere Löschungen unterlassen. Fertige Programme, Firmware, Quellen, Originalmessungen, Backups und sämtliche maßgeblichen Belege unverändert erhalten. Reale Deckung und Nachprüfung liefern. Keine neue allgemeine Speicheranalyse oder unnötige Neuübersetzung.

**Stopziel:** 75.161.927.680 Byte = **70 GiB frei** auf dem bereits gebundenen Aufnahme-/Auswertungsdateisystem. Dies ist der freigegebene Puffer; der harte Softwareguard bleibt unverändert 70.922.518.528 Byte. Es handelt sich um ein Stopziel zwischen den unteilbar freigegebenen Teilbäumen, nicht um eine Zusage exakt 70 GiB oder einer maximalen Dateisystemgröße. Bei schon erreichtem Ziel nichts löschen. Die eigenen nachfolgenden Prüfausgaben mit berücksichtigen.

## Verbindliche Eingänge – gezielt lesen

Zuerst Arbeitsbaum und wirksame `AGENTS.md` prüfen, vorhandene Änderungen erhalten. Danach ausschließlich die nötigen Bindungen verfolgen:

1. `./docs/project/cachebereinigung-review-20261016.md`: unabhängiger positiver Review und Grenzen.
2. `./private/cachebereinigung-vorbereitung-20261016-20261016T083636Z/`: `SEAL.private.json`, `REVIEW-MANIFEST.private.json`, `DELETE-ALLOWLIST.private.json`, `PROTECTED-ARTIFACTS.private.json`, `PROTECTED-SUPPLEMENT.private.json`, die zwei referenzierten Inhaltssnapshots und nötigen Prüfreceipts/-skripte. Keine breite Rohdaten-/Backupinventur.
3. `./private/rx-speicheroptimierung-offline-abschluss-v31-20261015T224337Z/`: `BINDINGS.private.json`, `STORAGE-PLAN-v3.private.json`, `verification/verify_v31.py`, `scripts/run-offline` und ihre tatsächlich benötigten gebundenen Datei-/Parserpfade. Die referenzierte v3-Basis nur nach diesen Bindungen lesen.

Die **unverändert zu erhaltende** Allowlist liegt exakt hier:

`./private/cachebereinigung-vorbereitung-20261016-20261016T083636Z/DELETE-ALLOWLIST.private.json`

SHA-256:

`6d1f0f0bf3d2ce32a095753349fe986a369d168f21f0e1d793a2e8ad9c06cd46`

Der historische Listenstatus `LOCKED_NOT_AUTHORIZED_NOT_EXECUTED` wird nicht umgeschrieben. Die aktuelle direkte Freigabe wird separat in der neuen Phase-B-Ablage dokumentiert. Weder eine geänderte Liste noch neue Inhalte unter gleichnamigen Pfaden sind mit freigegeben.

## Genau zwei erlaubte Löschkandidaten, in dieser Reihenfolge

1. `./private/peer-c1-review-korrektur-offline-20261015T050734Z/host-target/debug/incremental`
   - Snapshot: `./private/cachebereinigung-vorbereitung-20261016-20261016T083636Z/snapshots/5-incremental.private.jsonl`.
   - Snapshot-SHA-256: `fbdc0ca4de0b4bffd0efe20c6a1148c4ba5aa1b6f800053202179e3bbdbab83e`.
   - 10.994 reguläre Dateien; gebundene Wurzel und Verzeichnisse eingeschlossen.
2. `./private/offline-tagespaket-20261015T061442Z/host-target/debug/incremental`
   - Nur bearbeiten, wenn das Stopziel nach Eintrag 1 noch nicht erreicht ist.
   - Snapshot: `./private/cachebereinigung-vorbereitung-20261016-20261016T083636Z/snapshots/4-incremental.private.jsonl`.
   - Snapshot-SHA-256: `79d8359136ddccba18919cc20f6419c95db28a6bee51c8948137d67c6442f885`.
   - 13.328 reguläre Dateien; gebundene Wurzel und Verzeichnisse eingeschlossen.

Freigegeben sind ausschließlich die eingefrorenen vollständigen Inhalte dieser zwei Teilbäume einschließlich ihrer Wurzeln. Übergeordnete `debug`-/Cache-/Lieferordner, alle `debug/deps` und die drei übrigen Buildcaches sind ausgeschlossen. Höchstens 11,478592 GiB zurechenbare Allokation wurden ermittelt; APFS kann weniger bis null freigeben. Keine Einspargarantie und keine automatische Erweiterung, falls beide Einträge nicht genügen.

## Durchführung

### 1. Kleine neue Ablage, Integrität und frische Deckung

Neue private, ignorierte, nicht versionierte Ablage etwa `./private/cachebereinigung-phase-b-20261016-<UTC>/` anlegen. Vorher wirksame Git-Ausnahme und bereits versionierte Pfade prüfen. Verzeichnisse 0700, Dateien 0600, nötige eigene ausführbare Helfer 0700. Eigene Ausgaben klein halten; keine großen Cache-/Programm-/Lieferkopien anlegen. Bestehende Phase-A-/Speicherlieferungen niemals als Ausgabeziel verwenden.

Listenhash, Siegel, Snapshothashes und Schutzkatalogbindungen prüfen. Tatsächliche Ziel-Dateisystem-/4096-Byte-Allokationsbindung und freie Bytes erneut lokal erheben, Plan unverändert nachrechnen. Keine Liveverzeichnisse oder produktiven Hostprozesse dafür anlegen/starten. Wenn bereits 70 GiB frei sind, Löschungen überspringen, nachprüfen und berichten.

### 2. Unmittelbar vor jedem Eingriff technische Gates erfüllen

- Alle geschützten Dateien samt ergänzten Programmen/Quelllinks frisch mit den gebundenen Hashes und Identitäten prüfen. Keine Kollision mit Kandidatenpfaden/-inodes oder versionierten Dateien. Bekannte historische Warnungen nach dem bestehenden Review behandeln; keine alten Manifeste reparieren oder fehlende historische Ausführungshashes erfinden.
- Den betreffenden vollständigen Snapshot erneut gegen exakten Eintragssatz, Typ, SHA-256, Geräte-/Inode-/Rechte-/Link-/Zeitmetadaten prüfen. Wurzel und Vorfahren dürfen keine Symlinks/Aliase sein; keine Mount-/Dateisystemabweichung. Ein später neu angelegter oder veränderter Inhalt ist nicht freigegeben. Bei Abweichung vor dem Eingriff stoppen, nicht die Allowlist passend neu schreiben.
- Neue/parallele Builds während Prüfung und Entfernung zuverlässig ausschließen. Die vorhandenen zwei leeren Prozess-/Dateiaufnahmen genügen nicht. Zulässig ist eine vorübergehende kompatible exklusive Sicherung **bestehender einschlägiger Cache-Buildlocks**, sofern deren Wirksamkeit für die tatsächlich vorhandene Buildkette belegt ist, oder ein gleichwertiger belegter Ausschluss. Die betroffenen gemeinsam verlinkten Cacheinodes mit berücksichtigen. Keine Lockdatei entfernen/ersetzen, keine Rechteänderung, keinen Build starten und keinen fremden Prozess beenden. Lokale Aktivitätsabfragen ausschließlich auf die betroffenen Buildcaches beschränken; keine vollständigen Prozessargumente, Umgebungen oder produktiven Sockets protokollieren. Bei nicht verlässlich herstellbarer Ruhe STOP.

Falls ein eigener kleiner Löschhelfer nötig ist, ausschließlich in der neuen Ablage erstellen. Identitäts-/Snapshotbindung und Grenzen vor der ersten echten Löschung mit kleinen synthetischen Dateien prüfen: geänderter Inhalt, zusätzliche Datei, Symlink und Pfad außerhalb der Liste müssen vor einer Mutation zurückgewiesen werden. Dafür keine echten Caches verändern und keine neue umfangreiche Toolentwicklung oder Abhängigkeit beginnen.

### 3. Nur den nächsten unverändert freigegebenen Eintrag bearbeiten

Nur die gebundenen Einträge des jeweiligen Snapshots entfernen, reguläre Dateien und anschließend leere erfasste Verzeichnisse bis zur gebundenen Wurzel. Identitätsgebunden vorgehen; keine Symlinks/Mounts verfolgen und keinen ungeprüften rekursiven Shell-Aufruf verwenden. Änderungen während des Eingriffs führen zum sofortigen Stop und ehrlichem Teilabschluss. Bereits erfolgte Löschungen nicht verschweigen oder durch Neubau scheinbar rückgängig machen.

Nach jedem Eintrag tatsächlichen freien Platz und Erhalt der Schutzartefakte prüfen; bearbeitete/übersprungene Einträge und entfernte Namen/Identitäten nachvollziehbar privat protokollieren. APFS-Gewinn nur als tatsächlich beobachtete Freiplatzänderung ausweisen, systemweite Schwankungen kenntlich machen.

Bei mindestens 70 GiB frei keine weiteren Cachelöschungen. Sonst ausschließlich mit dem nächsten bereits freigegebenen unveränderten Eintrag fortfahren. Nach Eintrag 2 unter Ziel: verbleibende Lücke melden, keine weiteren Caches, Papierkörbe, Snapshots oder Belege anfassen. Wenn erst nach abschließenden Prüfausgaben erneut Platz fehlt, nur innerhalb eines noch nicht bearbeiteten freigegebenen Eintrags und nach denselben frischen Gates fortfahren; keine zusätzliche Freigabe für dieselbe unveränderte Liste nötig.

### 4. Abschließend mit vorhandenen Programmen nachprüfen

Die vorhandene finale v3.1-Verifikation einschließlich referenzierter v3-Basis sowie dieselbe kleine tatsächliche Dateiauswertung und die vier positiven C-Wire-/Parser-Rundläufe wiederholen. Bericht und zwölf Fenstertabellen mit den bestehenden gebundenen Kontrollbefunden vergleichen. Der dünne synthetische Kontrollfall bleibt fachlich `NICHT_BEURTEILBAR`; Parser-/Integritäts-PASS ist keine Funkabnahme.

Nur eigene neue Launcher-/Policy-/Ausgabepfade anpassen, falls die Phase-A-Helfer Ausgaben fest an ihre alte Ablage binden. Netzwerk, Gerätezugriffe und Schreibzugriffe auf sämtliche Originale in dieser Prüfphase OS-seitig sperren; eigene neue Ausgaben zulassen. Verbliebene/geplante Cachepfade weiterhin für Lesezugriff sperren. Genau ein Sandboxeintritt beziehungsweise geerbte Sperre; keinen historischen Launcher blind verschachteln. Normale Hosts und alte Rust-Testharnesses mit ungeprüften eingebauten Schreibpfaden nicht starten. Keine vollständige Lastproduktion oder Neuübersetzung nötig.

Schutzartefakte vollständig erneut hashen. Alte Cache-Snapshotprüfer nicht unverändert als Nachherprüfung aller fünf vorhandenen Bäume verwenden: absichtlich entfernte freigegebene Inhalte müssen exakt zum eigenen Löschprotokoll passen, alle übrigen Kandidaten bleiben außerhalb des Eingriffs. Keine historischen Nachweise umschreiben, um absichtlich fehlende Caches zu kaschieren. Vorher-/Nachher-Arbeitsstatus vergleichen; vorhandene Änderungen erhalten. Tatsächlich verbleibende freie Bytes nach eigenen Outputs berichten; 70-GiB-Stopziel und 66,051743-GiB-Startdeckung getrennt beurteilen.

## Grenzen und Abschluss

Kein `cargo clean`, `git clean`, pauschales `rm -rf`, Glob-Cleanup, Systemreiniger, Snapshot-/Papierkorblöschen, Verschieben/Archivieren-Ersetzen oder Eingriff außerhalb der zwei gebundenen Teilbäume. Keine weiteren Programme, Quellen, Logs, Rohdaten, Firmware-/Config-/NVS-/Flashsicherungen oder Belege löschen. Keine neuen SDKs/Abhängigkeiten/Lizenzannahmen/Downloads. Kein Git-Staging/Commit/Push/Reset. Keine Sicherheitsänderungen oder ProjectAtlas-Nutzung.

Keine seriellen/USB-/ROM-/Geräte-HTTP-/Ping-/Scan-/Reset-/Flash-/OTA-/Restore-/Provisionierungs-/Capturezugriffe. Keine Projekt-API, keine Gerätecredentials lesen, kein ARP-/WLAN-/Routing-/Dienstumbau und kein produktiver Hoststart/-stopp. **Auch erfolgreiche Speicherdeckung erlaubt keinen anschließenden Hardwarelauf.**

Privaten Abschlussbericht, Freigabebeleg mit exaktem Listenhash, Vorher-/Nachher-Receipts, genauen Lösch-/Überspring-/Abbruchnachweis, Schutz- und Dateiprüfungen sowie gehashte eigene Lieferung ablegen. Keine Rohmessungen oder echten Geräte-/Configwerte in öffentliche Ausgaben übernehmen. Koordinatorplan/Tracker nicht verändern.

```text
PHASE_B: ABGESCHLOSSEN / TEILABSCHLUSS_MIT_KONKRETEM_GRUND / OHNE_LOESCHUNG_ZIEL_SCHON_ERREICHT
AUTORISIERTE_ALLOWLIST_SHA256: 6d1f0f0bf3d2ce32a095753349fe986a369d168f21f0e1d793a2e8ad9c06cd46
BUILD_RUHE: BELEGT / NICHT_BELEGT
EINTRAG_1: ENTFERNT / UEBERSPRUNGEN / GESTOPPT
EINTRAG_2: ENTFERNT / UEBERSPRUNGEN / GESTOPPT
GESCHUETZTE_BELEGE_UND_PROGRAMME: HASHGLEICH_ERHALTEN / KONKRETE_ABWEICHUNG
DATEIPRUEFUNGEN_V31_V3_PARSER: BESTANDEN / KONKRETER_FEHLER
FREI_VORHER_BYTE: ...
FREI_NACHHER_BYTE_NACH_PRUEFAUSGABEN: ...
STOPZIEL_75161927680_BYTE: ERREICHT / NICHT_ERREICHT
STARTBEDARF_70922518528_BYTE: GEDECKT / LUECKE_..._BYTE
LOESCHUMFANG: NUR_AUTORISIERTE_SNAPSHOTINHALTE / KONKRETE_ABWEICHUNG
HARDWAREAKTIONEN: 0; HOSTAKTIVIERUNGEN: 0; BUILDS: 0; AGENTEN: 0
```

Danach stoppen. Als nächsten Schritt nur die unabhängige Abschlussprüfung und bei belegter Startdeckung den separaten gebündelten RX-Hardwareauftrag benennen. Nicht selbst mit der Livevorbereitung oder Messung fortfahren.
