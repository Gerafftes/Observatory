<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Koordinatorreview: Startvorbereitung bestanden, VALID-Zeitprüfung noch offen

**Die gezielte Pfad-/Startvorbereitung und der Lieferprüfer bestehen den unabhängigen Offlinecheck. Für einen verlässlich nutzbaren neuen Hardwareversuch bleibt ein konkreter VALID-Fehlalarm zu korrigieren.** Keine Hardwareaktion oder Agentenbeauftragung durch den Koordinator.

Geprüfte Lieferung: `./private/rx-kurzpilot-startvorbereitung-offline-20261016-20261016T201632Z/`. Eigene neue Prüfergebnisse: `./private/rx-startprep-koordinatorreview-20261016T204218Z/`. Originale unverändert; keine Firmware-/Hostneubauten.

## Frisch ausgeführte unabhängige Prüfungen

| Bereich | Ergebnis |
|---|---|
| Lieferverifier | PASS: 2.899 Produktionsbindungen, 35.515 unveränderliche Originalprüfungen; genau die zwei historischen Koordinator-Planungsdateien separat ausgewiesen. Keine allgemeine Dokumentausnahme. 121 ursprüngliche Testberechtigungen abgelaufen. |
| Kurz-/native Endregressionen | 32/32 frisch bestanden. |
| Vollständiger Binder-/Runnerweg mit echten Dateien | PASS, anfangs fehlende Root/Hostverzeichnisse über Produktionsvorbereitung erzeugt. Drei Hoststarts, 94 GETs einschließlich Vorlauf, zwei modellierte Uploads, volle 180/300-s-Captures, vier Recordersteuerungen und bestätigter Default. |
| Langsamer vollständiger Ablauf | PASS bei **2.093,579 s**, Vorlauf **144,219 s**. Neue echte Pfad-/Binder-/Quellprüfkosten eingerechnet; unveränderte Gesamtgrenze 2.100 s und Original-T0. |
| Default-Abbruchpfad | Nach modelliertem Capture-/Uploadfehler jeweils begrenzt CONFIRMED_DEFAULT und bestätigtes Abbruchende; kein zusätzlicher Upload. Originale Fehlerpräfixe erhalten. |
| Tatsächliche Hostprogramme | Default und Peer frisch bis zum echten HTTP-Statushandler mit HTTP 200 geprüft. Unveränderte Binaries, Permit/Signierer korrekt, eigene Kinder beendet; nur unter OS-Netz-/Gerätesperre über testweise anonyme Socketpaare. |
| Pfadregressionen / übrige Bindungen | 16 gespeicherte Pfadfälle, Integritätsgegenfälle und alte Langbasis nach Hash gebunden; nicht als frisch neu ausgeführte vollständige Suiten ausgewiesen. |

Die Verzeichnisursache des vorherigen Liveabbruchs ist damit geschlossen. Frische reservierte Root, feste Pfadrollen, einmalige Bindung und Prüfung unmittelbar vor Popen sind in die Produktionsaufrufer integriert. Der neue künftige Einstieg ist `scripts/prepare_entry.py` mit anschließendem eigenem `wrapper/entry_preflight.py` und `wrapper/entry_launch.py`; alte verbrauchte Wrapper bleiben ausgeschlossen.

## Verbleibender konkreter Befund: gültige Antworten werden fälschlich abgelehnt

Der unveränderte [Sessionkern](./private/rx-kurzpilot-startvorbereitung-offline-20261016-20261016T201632Z/adapter/session_core.py:286) sammelt konsistente VALID-Antworten, prüft ihren Mindestabstand aber ausschließlich für benachbarte Elemente über `zip(consistent, consistent[1:])`. Das kann zwei gültige, ausreichend weit auseinanderliegende Beobachtungen übersehen.

Der Koordinator hat die tatsächliche unveränderte `Session.execute` mit ausschließlich unterer synthetischer Uhr/Antwortschnittstelle geprüft. Alle drei RX1-Antworten haben identisches erwartetes Image, Slot und Boot sowie VALID=true. Abfragen erfolgen unverändert bei 0/60/120 Sekunden:

| Antwortzeiten | Beobachtete Nachbarabstände | Abstand erste ↔ dritte | Tatsächliches Ergebnis |
|---|---|---|---|
| 3 / 2 / 1 ms | 59,999 / 59,999 s | **119,998 s** | Falscher Abbruch: `two independent consistent VALID confirmations absent` |
| 1 / 2 / 3 ms | 60,001 / 60,001 s | 120,002 s | Vollständiger synthetischer Ablauf |
| 100 / 50 / 10 ms | 59,95 / 59,96 s | **119,91 s** | Derselbe falsche Abbruch |

Der Fehlalarm tritt nach einem bereits verbrauchten RX1-Upload auf, verhindert RX4 und Paaraufnahme und führt nur noch in den begrenzten Endpfad. Er beweist keinen Gerätefehler. Ein echter Firmware-/Funklauf wurde damit nicht simuliert oder nachträglich bestanden erklärt.

Der Agent hat einen entsprechenden Zwischenfall bereits offengelegt. Die finalen positiven integrierten Tests setzen kontrolliert ansteigende Antwort-/Protokollierzeiten ein; damit wird dieser Fall nicht als erfolgreicher Ablauf geprüft. Ein grüner Lieferverifier widerlegt den unabhängig reproduzierten Gegenfall nicht.

## Genau nächster Schritt

Eine eng begrenzte **Offlinekorrektur der VALID-Paarauswahl**: alle tatsächlich geeigneten konsistenten Beobachtungspaare berücksichtigen, einschließlich erster und dritter Antwort. Mindestabstand von 60 Sekunden, Image-/Boot-/Slot-/VALID-Konsistenz, drei Abfragen 0/60/120, Einzel-/Phasen-/Gesamtfristen und sämtliche Aktionsbudgets erhalten. Keine pauschale Zeit-Toleranz und kein zusätzlicher GET oder Upload. Fälle mit weniger als zwei geeigneten Beobachtungen bleiben gesperrt.

Die erhaltenen Gegenbeispiele auf neuer kleiner Kopie, beide Zielgeräte, wechselnde Antwortlatenzen und negative Identitäts-/Zeitfälle regressieren. Danach echte integrierte Normal-/Langsam-/Abbruchpfade erneut prüfen und nur Quellen-/Test-/Handoffbindungen aktualisieren. Bestehende Firmware-/Hostprogramme bleiben unverändert, **Neubauten 0**. Keine allgemeine neue Vorbereitungsschleife; die jetzt positiv geprüften Pfad- und Lieferkorrekturen übernehmen.

Bis zu dieser Korrektur keine Startempfehlung für den nächsten einmaligen Hardwareversuch. Die technischen 35-Minuten-Obergrenzen sind bestätigt, eine erfolgreiche Durchführung trotz des bekannten Fehlalarms ist damit nicht zugesichert. Freier Speicher besteht aktuell mit rund 77,6 GB gegenüber 70,92 GB Startbedarf; vor einem späteren Start frisch prüfen. Tagesstatus unverändert 2/5, kein Funk-/Tracking-PASS und keine neue Automation.

Eigener erhalten gebliebener Gegenbeleg: [VALID-LATENCY.private.json](./private/rx-startprep-koordinatorreview-20261016T204218Z/VALID-LATENCY.private.json). Eigene Gesamtmatrix und Hashbindungen: [REVIEW.private.json](./private/rx-startprep-koordinatorreview-20261016T204218Z/REVIEW.private.json).
