<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Agent-Auftrag: sichere BLL-Cachebereinigung vorbereiten

Projekt: `.`

**Jetzt ausschließlich Phase A ausführen: prüfen, nötige Programme/Belege zusätzlich absichern und eine konkrete Bereinigungsliste vorlegen. Noch nichts löschen, verschieben oder bereinigen.** Phase B ist lediglich ein gesperrter Vorschlag für eine spätere ausdrücklich erteilte Freigabe. Dieser Auftrag startet weder eine Bereinigung noch einen Hardwarelauf. Keine weiteren Agenten, Chatnachrichten oder Automationen.

## Ziel und bestätigte Ausgangslage

**Das ist der Fehler beziehungsweise die offene Aufgabe:**

Der unabhängig geprüfte RX-Hardwareplan v3.1 verlangt **70.922.518.528 Byte = 66,051742554 GiB freien Platz**. Der Platz schwankt: beim Review etwa 58,18 GiB; bei Erstellung dieses Auftrags etwa 61,04 GiB, also rund 5,01 GiB unter der Startreserve. Diese Werte sind historische Beobachtungen, keine aktuelle Deckungszusage. Die Startschranke darf nicht passend abgesenkt werden.

Fünf Buildcaches melden zusammen rund **118,37 GB**. Ihre Wurzeln sind aus den jeweiligen maßgeblichen Liefermanifesten ausgeschlossen, finale Programme und Belege liegen überwiegend separat. Das beweist nicht automatisch, dass jedes Cachefile entbehrlich ist. Insbesondere alte tatsächlich ausgeführte Testprogramme, Reproduktionseinstiege und direkte oder indirekte Lade-/Pfadbezüge müssen vor Freigabe berücksichtigt werden. APFS-Blockteilung verhindert die Zusage, dass eine angezeigte Ordnergröße vollständig als freier Platz zurückkehrt.

**Das erwarte ich:**

Einen kleinen, unabhängigen prüfbaren Abschluss mit geschützter Artefaktliste, fehlendenfalls zusätzlich gesicherten unveränderten Programmen, positiver Dateiprüfung bei gesperrtem Cachezugriff sowie einer priorisierten **exakten** Lösch-Allowlist für später. Genug Platz für den bestehenden Hardwareplan soll nach gesonderter Freigabe mit möglichst wenig Bereinigung erreichbar werden. Keine Beleg-, Firmware-, Quellen- oder Testdatenlöschung; kein kompletter Projekt- oder System-Cleanup.

## Relevante Eingänge und die fünf Prüfgrenzen

Zuerst Arbeitsbaum und wirksame `AGENTS.md` prüfen. Danach gezielt:

- `./docs/project/rx-speicheroptimierung-review-20261016.md` und `./private/rx-speicheroptimierung-koordinatorreview-20261016/REVIEW.private.json`: geprüfte Startreserve und unabhängiger Stand.
- Finale Speicherlieferung `./private/rx-speicheroptimierung-offline-abschluss-v31-20261015T224337Z/`: `BINDINGS.private.json`, `CANDIDATES.private.json`, `SOURCE-MANIFEST.private.json`, `MANIFEST.private.json`, `SEAL.private.json`, `STORAGE-PLAN-v3.private.json` und der tatsächliche Reproduktions-/Verifikationspfad. Geerbte Produktions-/Lastbasis `./private/rx-speicheroptimierung-offline-20261015-215434Z/` gezielt über diese Bindungen verfolgen.
- `./docs/project/rx-speicheranalyse-offline-20261015.md` und `./private/rx-speicheranalyse-offline-20261015/`: `inventory.private.json`, `FINDINGS.private.json`, `dynamic-dependencies.private.json`. Vorhandene Fakten nutzen, nicht wieder alle Backups oder Rohdaten analysieren.

**Nur diese fünf Buildcache-Wurzeln gehören zur Bereinigungsprüfung:**

1. `./private/rx-budget-vorklammern-offline-20261015T184011Z/build-cache`
2. `./private/rx-radar-export-korrektur-offline-20261015T152415Z/build-cache`
3. `./private/abendlaeufe-auswertung-offline-20261015T120658Z/build-cache`
4. `./private/offline-tagespaket-20261015T061442Z/host-target`
5. `./private/peer-c1-review-korrektur-offline-20261015T050734Z/host-target`

Die jeweils umgebenden Lieferordner sind **nicht** Löschkandidaten. Ebenso wenig SDK/Toolchains, Cargo-Registry/-Quellcache, Quellbäume, App-/ELF-/Bootloader-/Partition-/Configdateien, NVS-/Flashbackups, Rohmessungen, ursprüngliche Testläufe, Reports, Receipts, Manifeste oder gesperrte Hardwareübergaben. ProjectAtlas nicht verwenden. Keine breite Inventur anderer Projekte/Systemdaten.

## Phase A — jetzt vollständig ausführen, ausschließlich ohne Löschung

### A1. Kleine eigene Prüfablage und aktuelle Deckung

Neue ignorierte und nicht versionierte private Ablage, z. B. `./private/cachebereinigung-vorbereitung-20261016-<UTC>/`. Ignore und bereits versionierte Pfade vor Anlage prüfen; Verzeichnisse 0700, Dateien 0600, nötige eigene Ausführbare 0700. Vorhandene Originale unverändert lassen. Keine Cache- oder vollständigen Lieferkopien erzeugen.

Aktuelle freie Bytes und die tatsächliche Dateisystemzuordnung der geplanten Aufnahme-/Auswertungsziele rein lokal ermitteln. Aus dem gebundenen v3.1-Plan den harten Bedarf erneut berechnen. Exakte Lücke ausweisen, eigene neue Sicherungs-/Prüfausgaben berücksichtigen. Als **Planungsziel** sind etwa 70 GiB frei sinnvoll, um nicht genau an der 66,05-GiB-Grenze zu stehen; dies ist kein neuer Softwareguard und keine garantierte exklusive Reservierung. Wenn die Startreserve bereits gedeckt ist, das ehrlich melden und keine unnötige Bereinigung allein für eine große Einsparzahl vorbereiten. Die noch nötige Livevorbereitung und deren spätere erneute Platzprüfung bleiben getrennt.

### A2. Schutzliste und notwendige Programme belegen

Die aktuellen Kandidaten-, Datei-/Adapter-, Quellen-, Eingangs- und Siegelmanifeste gezielt auswerten. Für die fünf Herkunftslieferungen deren tatsächlich maßgebliche Manifeste und die betroffenen Programm-/Reproduktionsbindungen prüfen. Strukturierte Pfad-/Hashrollen verfolgen; nicht pauschal den gesamten Projektbestand nach beliebigen Texttreffern durchsuchen.

Eine `PROTECTED-ARTIFACTS.private.json` mit mindestens Pfad, Rolle, Herkunft, Größe und SHA-256 erstellen. Darin finale normale Default-/Peerhosts, reine Datei-/Parserprogramme, Firmware-BIN/ELF/Config und tatsächlich erforderliche Testprogramme sowie die zugehörigen Manifest-/Quell-/Build-/Testbelege führen. Historische Textverweise auf einen Buildcache von einer tatsächlich noch benötigten Datei-/Runtimeabhängigkeit unterscheiden. Auch Startskripte, Ladepfade, Konfigurationsreferenzen und Reproduktionseinstiege berücksichtigen; eine fehlende direkte Mach-O-Cacheabhängigkeit allein genügt nicht.

Falls ein benötigtes fertiges oder tatsächlich für maßgebliche Tests benutztes Programm nur im vorgeschlagenen Cachebereich liegt: entweder diesen Pfad samt nötigen Begleitdateien vom Löschplan ausschließen oder eine **zusätzliche kleine unveränderte Sicherung außerhalb des Caches** anlegen. Vorher Platzbedarf kalkulieren, Kopie nach Hash/Bytes/Modus prüfen und eine neue Herkunfts-/Pfadabbildung dokumentieren. Bloßes Kopieren ohne Funktions-/Pfadprüfung ist kein Ersatznachweis. Eincompilierte `CARGO_MANIFEST_DIR`-Schreib-/Lesepfade beachten. Nichts aus Originalen verschieben oder bestehende historische Manifeste umschreiben. Nicht erhaltene/unbekannte historische Artefakte nicht nachträglich als bytegenau gesichert ausgeben.

Keine vollständige alte Toolchain oder Cachebibliothek als „Sicherung“ kopieren. Wenn eine notwendige Datei nicht sicher abtrennbar ist, deren engster betroffener Bereich bleibt gesperrt. Kein Neubau als Ersatz für ein bereits gebundenes fertiges Programm: ein neuer Build ist nicht automatisch bytegleich zum alten geprüften Artefakt.

### A3. Engste geeignete Cacheteile auswählen

Zuerst ausschließlich die existierenden **`debug/incremental`-Unterbäume** dieser fünf Wurzeln beurteilen. Sie enthalten wiederherstellbare Compiler-Zwischenstände; dort sind typischerweise weniger fertige Programme betroffen als unter `debug/deps`. Nicht anhand des Ordnernamens allein freigeben. `debug/deps`, vollständige Cachewurzeln oder weitere Unterteile erst als separat begründete Alternativen aufführen, wenn die enge Auswahl voraussichtlich nicht genügt; keine automatische Erweiterung.

Pro konkret vorgeschlagenem Teilbaum erheben:

- Exakten absoluten normalisierten Pfad und erlaubte Wurzel; kein Glob, `..`, Symlinkziel, Verzeichnisalias, fremdes Dateisystem oder eingebetteter Mount.
- Geräte-/Inodebindung, Dateizahl, Metadatenaufnahme und logische/gemeldete Allokationsgröße. Hardlinks/Mehrfachreferenzen und APFS-Teilung kenntlich machen; keine vorgetäuschte exakte Freigewinnprognose.
- Schnittmenge mit Schutzliste, maßgeblichen Manifestdateien, benötigten Programm-/Reproduktionspfaden und versionierten Dateien. Jede echte Kollision sperrt den betroffenen Kandidaten bis zu einer überprüften zulässigen Auflösung.
- Aktive lokale Buildnutzung ausschließlich bezogen auf diese Caches prüfen: Cargo-/Rustc-/Linkeraktivität und passende offene Cachedateien/Buildlocks. Keine vollständigen Prozessargumente/Umgebungen mit möglichen Geheimnissen protokollieren. Eine vorhandene `.cargo-lock` beweist keine aktive Nutzung, ein einzelner leerer Snapshot garantiert keine dauerhafte Ruhe. Bei unklarer oder aktiver Nutzung nur „gesperrt“ melden; keinen Prozess beenden, keinen Lock entfernen, keinen Build auslösen.

Die spätere Priorität nach kleinstem Eingriff und geprüftem Wiederverwendungswert festlegen. Nach Möglichkeit eine geeignete vorhandene Abhängigkeitsbasis für kommende Builds behalten. Die bisherigen rund 118 GB sind kein Ziel, das vollständig entfernt werden muss.

### A4. Entbehrlichkeit ohne Löschung ausführbar gegenprüfen

Die maßgeblichen vorhandenen Programme und Prüfeinstiege **mit OS-seitig gesperrten Lesezugriffen auf die vorgeschlagenen Cachepfade** prüfen. So wird fehlender Cachezugriff simuliert, ohne Cachedateien zu ändern. Zusätzlich Netzwerk, Gerätezugriffe und Schreibzugriffe auf Originale sperren. Ausschließlich neue eigene kleine Prüfausgaben zulassen.

Mindestens:

1. Finale v3.1-Verifikation samt referenzierter v3-Produktions-/Lastbasis und ihren geschützten Kandidaten-/Quell-/Dateibindungen.
2. Ein tatsächlicher kleiner Datei-Parser-/Auswerterlauf mit den bereits gebundenen Programmen und gespeicherten synthetischen Eingängen; Ergebnis mit bestehendem Befund vergleichen. Kein neuer voller Lastlauf und keine große Originalkopie nötig.
3. Neu zusätzlich gesicherte benötigte Programme über den geplanten zulässigen Reproduktionseinstieg prüfen, soweit dies ohne Hardware und ohne Originalschreibzugriff möglich ist. Normale Hosts nicht starten; bei deren nicht ausführbarem Livepfad statische Bindungen/Ladepfade getrennt von der ausgeführten Datei-Probe ausweisen.
4. Eigene Schutzliste und Hashbindungen nach der Prüfung erneut prüfen. Der Zugriffsentzug darf nicht dadurch „bestanden“ werden, dass benötigte Prüfungen still ausgelassen, alte Quellstände eingesetzt oder Pflichtbelege entfernt werden.

Bestehende Reproduktionslauncher nicht blind mit einer weiteren äußeren Sandbox verschachteln: die finale Speicherlieferung betritt die OS-Sperre bereits selbst beziehungsweise erbt eine bestehende. Bei Anpassung ausschließlich eigene Launcher-/Ausgabepfade ändern und die genaue Abweichung dokumentieren. Kein normales Host-/Testprogramm mit ungeprüftem eincompiliertem Originalschreibpfad ausführen. Keine vollständigen SDK-/Rust-/Firmware-Neubauten, keine neuen Abhängigkeiten oder Installationen. Keine Lizenz-/Toolchainänderungen.

Ein Fehlschlag wegen einer echten Cacheabhängigkeit ist ein Befund und sperrt genau den Kandidaten. Nicht durch Rücknahme des Leseverbots ein falsches „entbehrlich“ erzeugen.

### A5. Konkrete gesperrte Bereinigungsliste und Abschluss liefern

`DELETE-ALLOWLIST.private.json` erstellen, **standardmäßig gesperrt**. Pro Eintrag: genaue gebundene Pfade/Identität, erfasster Inhalt beziehungsweise prüfbarer Inhaltssnapshot, Größen, Schutzlistenprüfung, Ruhe-/Aktivitätsurteil, Entbehrlichkeitstest und Reihenfolge. Gesamte Liste samt Schutzliste/Receipts hashen. Ein zum Freigabezeitpunkt geänderter Inhalt muss neu geprüft werden; eine pauschale Freigabe künftiger Dateien in denselben Ordnern wird nicht vorbereitet.

`CLEANUP-PROPOSAL.NICHT-AUSFUEHREN.private.md` mit konkreten Einträgen, begründetem Umfang, Hardware-Speicherziel und gesperrter Phase B liefern. Wegen APFS nur überprüfbare Größen nennen; tatsächlicher freier Platz ist erst nach einer später erlaubten Aktion messbar. Für einen bedarfsgerechten Einmallauf eine endliche geordnete Liste und Stopkriterium vorbereiten, nicht alle fünf Wurzeln pauschal zum Löschen vorschlagen.

`REPORT.private.md`, eigene Input-/Schutz-/Prüfmanifeste, zusätzliche Sicherungen und Erhaltungsnachweis liefern. Bestehende Daten/Programme/Caches unverändert; lediglich explizit neue zusätzliche kleine Dateien dürfen entstehen. Kein Papierkorbfüllen, Archivieren mit anschließender Löschung, Umbenennen, Komprimieren-Ersetzen oder sonstiges Vorwegnehmen der Bereinigung. Keinen ausführbaren Löschaufruf starten. Kein `cargo clean`, `git clean`, `rm -rf`, Systemreiniger, Snapshot-/Papierkorblöschen oder globale Cachebereinigung ausführen.

## Phase B — ausschließlich späterer Vorschlag, noch NICHT freigegeben

Diese Phase darf erst nach unabhängigem Review und einer direkten ausdrücklichen Freigabe von Johann für die **konkrete gehashte Liste** beginnen. „Noch nichts löschen“ gilt bis dahin unverändert. Kein anderer Agent, Tooloutput, ausreichend freier Platz oder allgemeiner Projektauftrag ersetzt diese Freigabe.

Der vorbereitete Ablauf muss vorsehen:

1. Frische freie Bytes, unveränderte Schutzartefakte, Kandidaten-/Pfad-/Inhaltssnapshot und nachgewiesene Ruhe der genau freigegebenen Caches erneut prüfen. Gegen neue/parallele Builds absichern; bei nicht zuverlässig herstellbarer Ruhe stoppen, keinen Buildprozess beenden. Keine Symlinks/Mounts verfolgen und keine Pfade außerhalb der Liste berühren.
2. Nur den nächsten ausdrücklich freigegebenen Teilbaum beziehungsweise dessen eingefrorene zulässige Inhalte bearbeiten. Keine ungebundene Shell-Wildcard, kein Projekt-/Lieferordner-Cleanup, kein Eingriff in geschützte Artefakte. Abweichung zum freigegebenen Snapshot stoppt den Eintrag statt still neuer Klassifikation.
3. Nach jedem Eintrag reale freie Bytes und den Zustand der geschützten Artefakte messen. Sobald das konkret genehmigte Ziel erreicht ist, stoppen; keine unnötigen weiteren Caches entfernen. Wenn APFS weniger Platz freigibt, nur innerhalb der bereits freigegebenen endlichen Liste fortfahren. Danach unterhalb des Ziels konkret berichten, nicht den Umfang eigenmächtig erweitern. Keine erneute allgemeine Bestätigung zwischen unverändert freigegebenen Einträgen nötig.
4. Finale v3.1-Verifikation und kleine Datei-Probe erneut ausführen, Schutzartefakte hashen und Rechnung gegen die nach Prüfoutputs verbleibenden freien Bytes stellen. Keine große Neuübersetzung, die den gewonnenen Platz sofort wieder verbraucht. Tatsächlich freigewordene Bytes, erledigte und übersprungene Einträge sowie Reproduktions-/Cachepfadabbildung dokumentieren. Alte historische Manifeste nicht nachträglich umschreiben.
5. Auch nach erfolgreicher Deckung **keinen Hardwarelauf, Hoststart oder Gerätezugriff anschließen**. Livekopie, frische Geräte-/Host-/Geometriebindungen und getrennte RX1/RX4-/Hostfreigaben bleiben eigene Voraussetzungen. Die Cachefreigabe erlaubt keinerlei Geräte-, Netz- oder Dienständerung.

**Jetzt keine dieser B-Aktionen ausführen.** A vollständig abschließen und anschließend stoppen; die spätere Freigabe ist ein eigener menschlicher Schritt mit der konkreten geprüften Liste.

## Harte Grenzen und Abschlusskriterien für Phase A

Keine Löschung, Verschiebung, Überschreibung, Bereinigung oder Lockentfernung an vorhandenen Dateien/Ordnern. Keine aktiven Prozesse oder Dienste stoppen/starten. Kein Git-Reset/Clean/Staging/Commit/Push. Keine SDK-/Firmware-/Hoständerung oder unnötigen Builds. Keine echten Credentials, NVS-/Securityänderungen, externen Downloads oder Installationen.

Keine Hardware-/Livezugriffe: seriell/USB/ROM/Reset/Flash/OTA/Restore/Provisionieren, Geräte-HTTP/Ping/Scan/Capture, Projekt-API, ARP/WLAN/Routing. Lokale Metadaten-, Freiplatz-, Cache-Ladepfad- und ausschließlich cachebezogene Buildaktivitätsprüfung ist erlaubt; keine breite produktive Prozess-/Socketinventur. Echte Geräte-/Configwerte und Rohdaten bleiben privat und aus Toolausgaben. Keine Agenten oder anderen Chats anschreiben. Koordinatorplan/Tracker nicht ändern.

```text
PHASE_A: VOLLSTAENDIG_GEPRUEFT / KONKRETE_LUECKE
GESCHUETZTE_BELEGE_UND_PROGRAMME: GEBUNDEN_UND_ERHALTEN / KONKRETE_LUECKE
ZUSAETZLICHE_PROGRAMMSICHERUNGEN: NICHT_NOETIG / HASHGLEICH_GEPRUEFT / KONKRETE_LUECKE
DATEIPRUEFUNGEN_BEI_GESPERRTEM_CACHEZUGRIFF: BESTANDEN / KONKRETE_ABHAENGIGKEIT
DELETE_ALLOWLIST: KONKRETE_PFADE_SNAPSHOT_UND_HASH / NICHT_FREIGABEFAEHIG
BEDARF: 70922518528 Byte; FREI=<frische Beobachtung>; LUECKE=<Bytes>
ERWARTETER_PLATZGEWINN: ALS_SCHAETZUNG_MIT_APFS_GRENZE_AUSGEWIESEN
LOESCHUNGEN: 0; VERSCHIEBUNGEN: 0; HARDWAREAKTIONEN: 0
PHASE_B: NICHT_FREIGEGEBEN_UND_NICHT_AUSGEFUEHRT
```

Einziger nächster Schritt nach Phase A: unabhängiger Koordinatorreview der geschützten Artefakte, der Entbehrlichkeitsprüfungen und der konkreten gesperrten Bereinigungsliste. Danach stoppen. Eine mögliche spätere Freigabe richtet sich genau auf diese Liste; kein automatischer Hardwarebeginn.
