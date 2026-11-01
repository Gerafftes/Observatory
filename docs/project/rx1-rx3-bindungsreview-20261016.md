<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX1/RX3: begrenzter Bindungsauftrag vom 16. Oktober 2026

**Beide Gerätebindungen bleiben unbestätigt. Genau je ein unauthentisierter `GET /ota/status` wurde an den historisch belegten Endpoints versucht. RX1: Verbindungs-Timeout. RX3: Verbindungsaufbau fehlgeschlagen; der anschließend passiv gelesene Cache ordnet diesen Endpoint einer anderen MAC zu. Keine weiteren Geräteabfragen oder Eingriffe.**

Die unmittelbare menschliche Freigabe galt ausschließlich diesem neuen Bindungsauftrag für RX1/RX3: je eine unauthentisierte Statusabfrage, keine Schreibzugriffe, Hoststarts oder Netzänderungen, danach Bericht. Der zuvor [im Preflight gestoppte Hardwarelauf](rx1-rx4-hardwarelauf-review-20261016.md) wurde nicht fortgesetzt. Kein Agent, anderer Chat oder Termin wurde angelegt.

## Quellen und Auswahl der Endpoints

Arbeitsbaum und wirksame Root-Anweisungen wurden geprüft. `/private/` ist wirksam ignoriert und enthält keine versionierten Dateien. Neue private Ablage mit Verzeichnissen 0700 und Dateien 0600; bestehende Änderungen, Originale und Backups bleiben erhalten.

Die Endpoints wurden aus den übereinstimmenden späteren Capture-Referenzen und tatsächlichen Vor-/Nachstatusantworten des abgeschlossenen Laufs vom 14. Oktober ausgewählt. Die IP-Hinweise in dessen früheren Anfangsreferenzen unterscheiden sich davon und wurden verworfen. Physische MAC, Interface und acht gespeicherte Configwerte stimmen zwischen den Referenzen überein. Die erste strengere Offlinegleichheitsprüfung mit den älteren IP-Hinweisen schlug vor jeglicher Liveaktion fehl; anschließend erfolgte die Auswahl anhand der neueren übereinstimmenden Originale. Keine IP wurde geraten oder gescannt.

Die drei historischen Originaldateien sind im privaten Eingang nach SHA-256 gebunden. Scriptquelle und Syntaxprüfung sind ebenfalls gebunden. Es wurden keine Zugangsdaten gelesen, kein Authorization-Header verwendet und keine Config-/Diagnoseabfrage ausgeführt.

## Tatsächliche Ausführung

Gemeinsamer Beginn dieses isolierten Bindungsauftrags: **18:45:23.832578 Uhr CEST**. Beide Statusversuche waren um **18:45:26.896991 Uhr** abgeschlossen. Ein abschließendes passives Cachelesen erfolgte innerhalb derselben selbst gesetzten 120-Sekunden-Grenze, mit Abschluss bei **T0 + 48,033 s**. Keine neue Uhr und kein neuer Versuch.

Vor jedem GET wurden aktives Interface, erwartete Hostadresse, lokales Subnetz und direkte Route zum bekannten Endpoint geprüft. Curl war an dieses Interface gebunden; Proxy, Wiederholungen und Weiterleitungen waren abgeschaltet, Verbindungsfrist 3 s, Gesamtfrist pro Versuch 9 s, Antwortgrenze 16 KiB. Jeder GET wurde vor Dispatch gezählt und protokolliert. Fehlgeschlagene Versuche wurden nicht wiederholt.

| Ziel | Genau ein Statusversuch | Gespeicherter passiver Befund danach | Urteil |
|---|---|---|---|
| RX1 | Curl Exit 28, Verbindungs-Timeout nach rund 3 s; keine HTTP-Antwort | Bekannte MAC fehlt; keine bestätigte MAC-/Endpointzuordnung | **NICHT_BESTÄTIGT** |
| RX3 | Curl Exit 7, Verbindungsaufbau fehlgeschlagen; keine HTTP-Antwort | Am abgefragten historischen Endpoint steht eine andere MAC; bekannte RX3-MAC fehlt | **NICHT_BESTÄTIGT** |

Kein aktuelles Image, Boot, Slot oder VALID wurde ausgelesen. Der MAC-Widerspruch bei RX3 sperrt dessen historische Endpointzuordnung für einen späteren authentisierten Zugriff. Eine konkrete Ursache des fehlgeschlagenen Verbindungsaufbaus ist damit nicht bewiesen. Fehlende Cacheeinträge oder ein Timeout beweisen keinen Geräteausfall. Ein DHCP-Wechsel ist eine mögliche Erklärung, kein bestätigter Befund.

| Verbrauch | Tatsächlich |
|---|---:|
| Unauthentisierte RX-GET-Versuche | **2**, je einer RX1/RX3 |
| Passive lokale Abfragen | **5**: Interface einmal, Cache vor/nachher, direkte Route je Ziel einmal |
| Geräteschreibzugriffe / POSTs | **0** |
| Config-/Diagnose-/weitere Statusabfragen | **0** |
| Hoststarts/-wechsel, Captures, Recordersteuerungen | **0** |
| Netzwerk-/Sicherheitsänderungen, Reset-/serielle Zugriffe | **0** |

## Folgerung und nächster Schritt

Der Bindungsauftrag ist ausgeführt und beendet. Eine Fortsetzung des Basis-/Installations-/Paarlaufs ist durch diese Ergebnisse nicht freigegeben oder technisch vorbereitet. Die beiden erlaubten Statusversuche sind verbraucht.

Als nächster enger Schritt müssen die **aktuellen Endpoints zu den bekannten physischen MACs von RX1 und RX3 unabhängig belegt** werden, beispielsweise anhand einer bereits vorhandenen AP-/DHCP-Clientliste. Diese Liste wurde in diesem Auftrag nicht abgefragt. Anschließend ist vor authentisierten Zugriffen die frische MAC-/Endpoint-/Interface-/Routen-/Nodebindung erforderlich. Ein künftiger Auftrag muss dafür neue, konkrete Lesegrenzen festlegen. Kein Firmware-Neubau, Reset, Flash oder Netzumbau ist durch die vorhandenen Belege begründet.

## Private Originale und Integrität

`./private/rx1-rx3-bindung-20261016T164410Z/`

- `INPUT.private.json`: direkte Freigabe, Auswahl der belegten Endpoints und Hashes der historischen Originale.
- `bind_once.py`, `SOURCE.private.json`, `ONE-SHOT.consumed`: ausgeführte Quelle, Quellenhash und einmaliger Verbrauch mit monotone Zeitanker.
- `command-*.stdout.private`, `command-*.stderr.private`, `command-*.receipt.private.json`: ursprüngliche Interface-/Cache-/Routen-/Curlantworten einschließlich Fehlern und Hashes.
- `RX1.binding.private.json`, `RX3.binding.private.json`, `RESULT.private.json`: ursprüngliche Ergebnisse des Statusabschnitts.
- `post-cache.stdout.private`, `post-cache.stderr.private`, `PASSIVE-POSTCHECK.private.json`, `EVALUATED.private.json`: genau ein passiver Nachcheck und Offlineeinordnung ohne weitere GETs.
- `events.private.jsonl`: dauerhaft geflushte Dispatch-, Zeit- und Verbrauchsbelege. Der Statusabschnitt endet vor dem ausdrücklich protokollierten passiven Nachcheck.
- `CLOSED-FILES.private.json`: vorläufiges Manifest vor dem passiven Nachcheck; dessen damaliger Ereignisdateistand wird als gehashter Präfix erhalten.
- `FINAL-MANIFEST.private.json`: finale Hashbindung der geschlossenen eigenen Dateien nach dem Nachcheck und der Berichtserstellung.

Keine Gerätewerte, MACs, IPs oder originalen Status-/Netzantworten wurden in diesen öffentlichen Bericht übernommen.
