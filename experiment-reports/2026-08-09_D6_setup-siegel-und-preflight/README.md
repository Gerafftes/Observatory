# D6-Setup-Siegel und versiegelter Preflight — 2026-08-09

## Ergebnis

Das reale 1TX-/4RX-Setup wurde nach Rückkehr des TX auf reine
Stromversorgung versiegelt. Der anschließend mit genau diesem Setup
ausgeführte 25-Sekunden-Preflight ist bestanden:

- Aufnahme: `preflight-neutral-20260809-01`
- Dauer: 25 Sekunden
- gültig geschriebene Frames: 2.545
- Drops: 0
- Status: `completed`
- `incomplete=false`
- Integritätsfehler: keiner
- Label/Ground Truth: nicht vorhanden
- Setup-ID und Setup-Hash: exakt passend

Dieser PASS belegt Transport, RX1-bis-RX4-Vollständigkeit, Datenrate, stabiles
ausgewähltes Raster, Laufzeit-TX-Bindung, Recorderintegrität und
Setupidentität. Er ist noch kein Nachweis der Classification- oder
Positionsgüte.

## Versiegeltes Setup

Die private Spezifikation und das daraus vom freigegebenen Server erzeugte
Siegel liegen mit Dateimodus `0600` unter:

- `private/d6-20260809/setup-spec.json`
- `private/d6-20260809/sealed-setup.json`

Identität des Siegels:

| Merkmal | Wert |
|---|---|
| Setup-ID | `setup-0a49d75f122f9dc9` |
| Setup-SHA-256 | `0a49d75f122f9dc9757aed7e175bb444056e7fdde6889bd8965288d1b9008a4e` |
| Server-SHA-256 | `91feb860f89f094ba16ea9d749e3a1e5378de1a25ceedd08cebeb67f2cd3484b` |
| TX-App-Readback-SHA-256 | `a66a11ad8e299a962572c2bc8a9e4067599a8460c44ae0efb1deae07277994e5` |
| RX1–RX4-Firmware-SHA-256 | `12a119440f47e7bc8175eaad7f0166b916610b78fda385de9d54b68ba6e147cd` |
| TX-Filter-SHA-256 | `60c998af0f5f845bd2afaac558a7da831a3a34ec07544de0efc6d1e747fad86c` |

Das Siegel enthält außerdem:

- Raum: `[4020, 2590, 3440]` mm in
  `[x=Länge, y=Höhe, z=Breite]`
- TX: `[1510, 1190, 390]` mm
- RX1: `[0, 500, 280]` mm
- RX2: `[4020, 870, 970]` mm
- RX3: `[0, 740, 2110]` mm
- RX4: `[4020, 870, 2460]` mm
- Mac: `[4020, 870, 2500]` mm, Bezugspunkt Mitte des Unterteils
- Mac-Revision: 4 cm von RX4 entfernt auf der von RX2 wegführenden Linie
- Tür: geschlossen
- Möbel: normaler statischer Zustand vom 2026-08-09
- Mac-Kabel: normaler Messzustand vom 2026-08-09
- Kanal: 6
- Raster je RX: 2437 MHz, eine Antenne, 64 Subcarrier, PPDU-Typ 0,
  Layout-Flags 0

Jede Änderung an einer dieser Definitionen verlangt ein neues Siegel und eine
neue vollständige Serie.

## Serverstart und Readiness

Ein erster lokaler Start validierte das Setup erfolgreich, konnte wegen der
Anwendungs-Sandbox aber UDP- und WebSocket-Port nicht binden und endete vor
jeder Aufnahme. Danach wurde derselbe unveränderte Release mit den nötigen
lokalen Portrechten gestartet. Der Server meldete:

- Setup aktiv und Hash passend
- Quelle `esp32`
- Status `ready`
- Positionsindex noch inaktiv
- exakt RX1 bis RX4 aktiv
- bei allen RX:
  `source_binding_attested=true`, `filter_enforced=true`,
  `source_matched_filter=true`, `identity_valid=true` und
  `identity_matches_setup=true`
- Binding-Alter vor dem Lauf zwischen 15 und 36 ms
- gemeinsame TX-Bindung über alle RX konsistent

Damit waren die fail-closed Vorbedingungen des Capture-Runners erfüllt.

## Preflight-Aufnahme

Der Runner wurde mit neutraler ID gestartet:

```text
python3 RuView/scripts/capture_position_run.py \
  --server http://127.0.0.1:8080 \
  --kind preflight \
  --recording-id preflight-neutral-20260809-01
```

Per-RX-Ergebnis:

| RX | Frames | mittlere Rate über 25 s | Raster |
|---|---:|---:|---|
| RX1 | 604 | 24,16 Hz | 2437 MHz / 1 / 64 / PPDU 0 / Flags 0 |
| RX2 | 647 | 25,88 Hz | 2437 MHz / 1 / 64 / PPDU 0 / Flags 0 |
| RX3 | 684 | 27,36 Hz | 2437 MHz / 1 / 64 / PPDU 0 / Flags 0 |
| RX4 | 610 | 24,40 Hz | 2437 MHz / 1 / 64 / PPDU 0 / Flags 0 |

Alle RX lagen deutlich über dem Mindestgate von 5 Hz und deckten die volle
Laufdauer ab. Die Summe der RX-Zähler stimmt exakt mit 2.545 geschriebenen
Frames überein.

Erzeugte Dateien:

| Datei | Größe | SHA-256 |
|---|---:|---|
| `data/recordings/preflight-neutral-20260809-01.raw-csi.v1.jsonl` | 3.774.716 Byte | `0bd38597fc59083d1a61a2e752202a7784bacc878ff449ceb7d8a278cfce31a3` |
| `data/recordings/preflight-neutral-20260809-01.raw-csi.v1.meta.json` | 2.407 Byte | `5488fd47ded95bafc786cace4b88acb81c1e09289dc455a95c8c7d129df2280b` |

Die Metadaten tragen exakt die versiegelte Setup-ID und den Setup-Hash. Der
Runner prüfte nach dem Stop erneut Aufnahmezustand, Zähler, Dauer, Drops,
RX-Abdeckung, Raster und Setupbindung und endete mit Exit-Code 0.

## Separater Live-Trust-Hinweis

Unabhängig vom bestandenen Recorder-Preflight meldete der allgemeine
Engine-Trust während des laufenden Servers wiederholt eine RX-
Zeitstempelspreizung oberhalb des 60-ms-Guardintervalls. `/health/ready` blieb
für die Aufnahme `ready`, führte den allgemeinen Trust aber als:

- `demoted=true`
- `effective_class=Restricted`
- `raw_outputs_suppressed=true`

Dieser Zustand ist kein Bestandteil der vorab festgelegten Preflight-Gates
für die verlustfreie setupgebundene Raw-CSI-Aufnahme und ändert daher deren
PASS nicht. Er darf jedoch nicht als bestandene Live-Visualisierung
interpretiert werden. Vor der abschließenden Live-Anzeige muss dieser
Trust-/Zeitstempelpfad erneut geprüft werden.

## Nächster zulässiger Schritt

Das Setup bleibt ab jetzt unverändert. Als Nächstes folgt genau eine neue
65-Sekunden-Leerraumkalibrierung mit neutraler Aufnahme-ID. Sie darf erst nach
ausdrücklicher Bestätigung beginnen, dass während der vollständigen Dauer
keine Person den Raum betritt. Mac, Möbel, Kabel und normale Gegenstände
bleiben dabei im Raum.
