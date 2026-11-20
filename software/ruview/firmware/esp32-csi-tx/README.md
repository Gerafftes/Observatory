# ESP32-S3 CSI transmitter (TX1)

Die gepflegte Umsetzung verwendet ESP-IDF 5.4. Die Arduino-Referenz bleibt ein eigener historischer Stand. Diese öffentliche Einordnung übernimmt weder private Geräteprofile noch operative Migrationsakten.

## Bestehender TX1 und neues Board

| Thema | Bestehender TX1 | Neues Board |
| --- | --- | --- |
| Quelle | Bereits eingefrorenen Kandidaten und zugehörigen Receipt erhalten; ein späterer Build ist keine stille Ersatzquelle. | Einen bewusst gewählten Quellstand mit eigener neuer Buildidentität verwenden. |
| Layout | Gesicherte Partitionen und Daten erhalten; nur konkret autorisierte Bereiche schreiben. | Tatsächlichen Flash, Pins, Versorgung und initiales Layout vor Erstinbetriebnahme prüfen. |
| Identitäten | Private bestehende Profil-/NVS-Bindung erhalten. | Neue Board-/Netzwerte erzeugen; TX1-Identität nicht kopieren. |
| NVS | Der Importer ist kein allgemeiner Editor und ergänzt nur vorgesehene Namespaces. | Eine geprüfte initiale NVS-Erstellung für ein leeres Board ist gesondert offen. |
| Rückweg | Gerätegebundenen verifizierten Rückweg erhalten. | Eigenes Anfangsbild und Recovery festlegen. |

Das gespeicherte TX1-Profil verwendet ein logisches 4-MiB-Layout auf physisch 16 MiB. Gebaute Bootloader-/Partitionstabellen sind dafür keine Migrations-Schreibeingaben. Generisches `idf.py flash`, merged-image flashing und `erase-flash` sind kein zulässiger Ersatz für diesen gebundenen Weg.

## Reproducible rebuild from a clean checkout

Der dokumentierte Quellvertrag bindet ESP-IDF-Commit `67c1de1eebe095d554d281952fde63c16ee2dca0` (Tag `v5.4`) und `espressif/led_strip` 3.0.3 über den committed Dependency-Lock. Für die Ermittlung der Git-Identität muss `git -C <TX-Verzeichnis> rev-parse HEAD` den bewusst gewählten Repository-Commit auflösen können. Der veröffentlichte Helper prüft dabei weder eine vollständige Historie noch die erwartete Repositorywurzel. Ein isoliertes Quellarchiv ohne Git-Kontext genügt dafür nicht; innerhalb eines anderen Git-Arbeitsbaums könnte dagegen dessen HEAD aufgelöst werden. Die Commit-/Quellzuordnung muss deshalb zusätzlich bewusst geprüft werden. Werkzeuge und Abhängigkeiten können bei der ersten Einrichtung Netz- und Speicherzugriff benötigen; sie sind nicht in diesem Dokumentationssatz enthalten.

Im vollständigen vorbereiteten Quellcheckout lautet der dokumentierte Offline-Buildvertrag:

```sh
(
  cd software/ruview/firmware/esp32-csi-tx
  idf.py reconfigure
)
python software/ruview/firmware/esp32-csi-tx/tools/build_release.py \
  --output artifacts/tx1-candidate-UNIQUE --build
```

Ein frischer Ausgabename ist erforderlich. Der beschriebene Helper bindet Quellen, Repositorycommit, Dependency-Lock und aufgelöste Komponente; er verweigert fehlende oder abweichende Grundlagen und baut nur. Er scannt keine seriellen Ports, öffnet kein Gerät und flasht/provisioniert nichts. Diese Dokumentationsarbeit führt den Befehl nicht aus und wiederholt keinen Build.

Ein eingefrorener Kandidat wird aus seinem eigenen Archiv samt `sources.json` reproduziert. Änderungen auch an Dokumentation/Operatorwerkzeugen können die neue Quellidentität ändern; ein heutiger Checkout ersetzt nicht den bestehenden TX1-Nachweis. Vollständiger BIN-Hash und ESP-Image-Digest sind verschiedene Werte. Die Receipt-/Konfigurationsprüfung und ein unabhängiger Build aus dem tatsächlich veröffentlichten vollständigen Snapshot bleiben eigenständige Prüfungen.

Weiter: [Firmwareübersicht](../../../../docs/setup/firmware.md), [Hardware](../../../../docs/hardware/overview.md) und [Projektstatus](../../../../docs/project/status.md). Die komponenteneigenen Lizenz-/Notice-Bindungen bleiben beim vollständigen Quellpaket erhalten.
