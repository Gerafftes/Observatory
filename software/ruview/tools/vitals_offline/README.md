# Ein-Link-CSI-Vitalwerte: Offline-Import und Amplitudenexperiment

Dieses Werkzeug arbeitet **rein offline und lesend** mit TX1 und genau einem RX.
`csi_import.py` bleibt der VITAL-01-Import samt Datenvertrag und Qualitätsbericht.
`vitals.py` ergänzt in VITAL-02 zeitliche Amplitudensignale, einen Atemzweig und
einen getrennten experimentellen Pulszweig. Fehlende Zeit- oder Signalevidenz
führt zur begründeten Nichtausgabe. **Kein Ergebnis ist physiologisch validiert.**
Firmware, Geräte, Netzwerk, mmWave, Server und Observatory werden nicht geändert.
Es werden keine Messungen gestartet; bestehende TX-Sperren bleiben bestehen.

Benötigt wird Python 3.15 oder neuer. Der Import braucht nur die Standardbibliothek;
die Auswertung zusätzlich NumPy >= 1.24 (bereits RuView-Abhängigkeit), kein SciPy.
Bei Bedarf in einer lokalen virtuellen Python-Umgebung: `python3 -m pip install 'numpy>=1.24'`.

## Start

Vom BLL-Projektverzeichnis aus:

```sh
python3 software/ruview/tools/vitals_offline/csi_import.py \
  software/ruview/tools/vitals_offline/fixtures/synthetic_one_link.raw-csi.v1.jsonl \
  --rx 1 \
  --trusted-tx1-filter-sha256 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  --report /private/tmp/vitals-synthetic-quality.json

python3 -m unittest discover -s software/ruview/tools/vitals_offline -p 'test_*.py' -v
```

Die Fixture ist ausdrücklich **synthetisch**; ihre Hash-Zeichenfolge `aa…aa`
gehört zu keinem nachgewiesenen Gerät. Ausgabeziele müssen neu sein; vorhandene
Berichte werden nicht überschrieben.

Für eine vorhandene August-Aufnahme mit passendem historischen Setup-Beleg:

```sh
python3 software/ruview/tools/vitals_offline/csi_import.py \
  data/recordings/empty-neutral-20260809-01.raw-csi.v1.jsonl \
  --rx 1 \
  --sealed-tx1-setup private/d6-20260809/sealed-setup.json \
  --report private/vitals-offline/quality/mein-neuer-bericht.json
```

`--sealed-tx1-setup` prüft den Definitions-Hash und die Setup-ID, verlangt die
logische TX1-Zuordnung und verlangt dieselbe Setup-Identität im
Aufnahme-Sidecar. Nur der TX-Filter-Hash dient dem Import; Geometrie, mmWave und
Positionsfreigaben werden daraus nicht übernommen. Alternativ kann
`--trusted-tx1-filter-sha256` mit einem **außerhalb der Aufnahme** verifizierten
Hash verwendet werden. Ohne eine solche Referenz zählt die CLI die Datei,
wählt aber **null TX1-Pakete** aus. Ein Hash im Mitschnitt allein ist kein
unabhängiger Beleg. Der Hash ist keine kryptografische Authentisierung des
Funksenders; die Aussage beruht auf der geprüften RX-Filterbindung und der
separaten Referenz.

Der JSON-Bericht enthält nur Qualitätsdaten und gehört wie die Rohdaten in
eine private, ignorierte Ablage. Er wird nicht ins Repository übernommen.
Sein `input_sha256` bindet den Befund an die genaue Eingabedatei.
Python-Code kann `iter_tx1_frames(path, rx_id, trusted_digest)` aus
`csi_import.py` wiederverwenden. Jedes `ImportedFrame` enthält die validierte
Originalzeile, eine `segment_id` mit Trennungsgrund und `complex_samples`
beziehungsweise `samples_for_antenna()`.

## VITAL-02 ausführen

Dieselbe Auswahl und Autorisierung gelten für die Analyse; es gibt keinen
zweiten Rohdatenimport. `analyze_recording()` verwendet `analyze_capture()` für
die Identitäts-/Sidecar-Prüfung und `iter_tx1_frames()` für die Daten. Ungültige
Zeilen brechen die Analyse ab, statt eine reparierte Zeitreihe zu erfinden.

```sh
python3 software/ruview/tools/vitals_offline/vitals.py \
  data/recordings/empty-neutral-20260809-01.raw-csi.v1.jsonl \
  --rx 1 --sealed-tx1-setup private/d6-20260809/sealed-setup.json \
  --respiration-band 0.1 0.5 --respiration-window 60 \
  --pulse-band 0.8 2.17 --pulse-window 30 --step 5 --antenna 0 \
  --report private/vitals-offline/meine-neue-vital02-analyse.json
```

Der Bericht enthält den Importbefund, Eingabe- und Implementierungs-SHA-256,
vollständige Konfiguration, Abschnitts- und Fenstergrenzen in Originalzeilen,
tatsächlich beobachtete Ankunftsraten, Lücken, lückenarme Teilstücke und
Entscheidungsgründe. Zeitangaben `start_s`/`end_s` sind relative **aufgezeichnete**
Zeit innerhalb des jeweiligen Importabschnitts. Absolute Zeitzuordnung und
Segmentgrenzen stehen im Importbefund. Roh-I/Q, TX-Filter-Hash, Präsenz-Confidence
und Versuchslabels werden nicht in die Auswertung übernommen. Berichte bleiben
privat und werden niemals überschrieben.

| `status` | Bedeutung | Werte |
| --- | --- | --- |
| `not_evaluable` | Nicht auswertbar: Zeitbasis, Abdeckung, CSI-Qualität oder Periodizität reichen nicht | Alle vier Kandidaten-/Schätzfelder `null` |
| `diagnostic_candidate` | Unterstützte Periodizität auf ausreichend belegter Zeitbasis; mindestens eine Akzeptanzbedingung fehlt | Nur `candidate_hz` und `candidate_bpm` |
| `experimental_estimate` | Technisch akzeptierte experimentelle Schätzung | Zusätzlich `estimate_hz` und `estimate_bpm` |

`*_bpm` ist lediglich `60 * Hz` (Zyklen/min), kein Nachweis echter Atemzüge oder
Herzschläge. `physiologically_validated` bleibt immer `false`. Jede Aktualisierung
wird neu berechnet. Es gibt keine Fortschreibung alter Werte und keine
Umrechnung von Präsenz-Confidence in Vitalwert-Confidence. Die beiden Zweige
haben eigene Fenster und Ergebnisse. Der Pulszweig darf die Atmung nicht sperren.

### Zeit- und Qualitätsregeln

- Nur die vorhandene monotone Hostzeit innerhalb einer Uhrepoche und einer
  bekannten RX-Boot-ID kann eine Frequenzausgabe tragen. Schema-1-Wanduhrdaten
  erhalten deshalb **auch bei einem scheinbar sauberen Spektrum keine Hz/BPM**.
  Mesh-Zeit ist nie die Spektralzeit; ihre Rücksprünge werden ausdrücklich gezählt.
  Importabschnitte werden weder sortiert, zusammengefügt noch zeitlich repariert.
- Es gibt **keine Interpolation und keine angenommene Abtastrate**. Ein Fenster
  benötigt mindestens 16 Pakete, fast die gesamte angeforderte Zeitspanne und
  mindestens vier beobachtete Pakete pro Zyklus am oberen Bandrand.
  Höchstens ein erlaubter Paketabstand darf an jeder Fensterkante fehlen.
- Der maximal erlaubte Abstand ist das Minimum aus 0,5 s, dreifachem Median
  der tatsächlichen positiven Paketabstände und `1 / (4 * obere Bandfrequenz)`.
  Jede größere Lücke sperrt das betroffene Fenster. Die Originalgrenzen und
  lückenarmen Teilstücke bleiben sichtbar. Nach einer Lücke sind ausschließlich
  neue, vollständig ausreichend abgedeckte Fenster wieder zulässig.
  Dies ist eine konservative Untersuchungsregel, kein allgemeines
  Nyquist-Theorem für unregelmäßig abgetastete Daten.
- Doppelte Zeitstempel oder Seriennummern, ein belegter RX-Fehlerzustand und
  beobachtete Gain-Wechsel sperren das Fenster. Mit RX-Callback-Zeit werden
  Host-/Gerätetakt verglichen: maximal 0,5 % Taktdifferenz und höchstens ein
  erlaubter Paketabstand an Restzeitstreuung. Widerspruch sperrt jede Frequenz.
  Fehlende Callback-Evidenz, fehlende v3-Qualitätsflags, nicht bestätigtes
  Rohprofil oder nicht bestätigter RX-Zustand begrenzen auf einen Kandidaten.
- Reguläre Aktualisierungen beginnen nach einem vollen Fenster und folgen
  ungefähr alle 5 s. Ein zusätzlicher Abschlussbefund dokumentiert auch eine
  kurze Aufnahme oder ein Ende zwischen den regulären Aktualisierungen;
  `scheduled_update: false` kennzeichnet ihn. Er ist kein fortgeschriebener Wert.

Band, Fenster, Schritt und Antenne sind CLI-Parameter; die weiteren Regeln
stehen als explizite Untersuchungsparameter in `Config` und im JSON-Bericht.
Es sind **unvalidierte technische Kriterien**, keine medizinisch kalibrierten
Grenzen. Fenster müssen mindestens vier Zyklen am unteren Bandrand umfassen;
die Standard-Atemfenster enthalten sechs.

### Signalmethode und Akzeptanz

Für eine fest ausgewählte Antenne wird pro aufgezeichnetem Unterträgerindex die
Amplitudenfolge `abs(complex(i,q))` über viele Pakete gebildet. Unterträger mit
mehr als 5 % Nullamplituden oder mehr als 1 % I/Q-Sättigung werden ausgeschlossen.
Bei `first_word_invalid` entfallen die ersten zwei I/Q-Indizes der ersten
Antenne für das gesamte Fenster. Aus den verbleibenden Spalten entsteht das
einfache Mittel ihrer relativen Amplituden `amplitude / zeitlicher_Mittelwert - 1`.
Die verwendeten und ausgeschlossenen Indizes werden aufgeführt; sie werden
nicht anhand eines gewünschten Atem-/Pulsmaximums gewählt.

An den tatsächlichen Zeitpunkten wird je Frequenz ein Sinus-/Kosinusmodell
zusammen mit Offset und linearem Trend per Least Squares angepasst. Die
Fehlerreduktion gegenüber dem Modell nur mit Offset und Trend ist die erklärte
Varianz. Das entspricht dem Grundprinzip der
[generalisierten Lomb-Scargle-Analyse](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.lombscargle.html),
hier mit zusätzlichem linearem Trend und direkt mit NumPy implementiert.
Das Frequenzraster hat höchstens `1/(4*T)` Abstand; die physische Auflösung
bleibt ungefähr `1/T`. Es wird kein FFT über die Unterträger eines Einzelpakets
als zeitliches Vitalspektrum missverstanden.

Ein Kandidat benötigt mindestens 8 % erklärte Varianz und ein Verhältnis von
mindestens 6 zum spektralen Median außerhalb der Hauptkeule (`±2/T`). Rauschen
oder ein konstantes Signal erhält keinen zufälligen Maximumswert. Für technische
Akzeptanz gelten zusätzlich mindestens 20 % erklärte Varianz, ein Verhältnis
von mindestens 2 zum stärksten konkurrierenden Peak und ein Maximum innerhalb
des Untersuchungsbandes. Peaks exakt am Bandrand bleiben diagnostisch.
Beide zeitlichen Fensterhälften müssen unabhängig unterstützte Frequenzen
innerhalb `2/T` des Gesamtfensters liefern. Diese Metriken sind keine
Wahrscheinlichkeiten und keine Vitalwert-Confidence.
Fehlen außerhalb der Hauptkeule mindestens drei Frequenzrasterpunkte, ist
keine Hintergrundbewertung möglich: Ein zu enges Untersuchungsband kann daher
zur Nichtausgabe oder zu fehlender Stabilitätsevidenz in einer Fensterhälfte führen.

Beim Puls muss zudem ein gleichzeitig endendes, technisch akzeptiertes
Atemfenster verfügbar sein. Der Kandidat wird auf **alle ganzzahligen
Atemobertöne ab Ordnung 2 im Pulsband** geprüft, mit Toleranz
`1/T_puls + Ordnung/(4*T_atmung)`. Ein möglicher Oberton oder eine unklare
Atemgrundfrequenz lässt nur einen diagnostischen Kandidaten zu. Es wird kein
vermuteter Oberton entfernt, um daraus einen Herzschlag abzuleiten.

Diese einfache Amplitudenmittelung kann gegenphasige Unterträger auslöschen,
schwachen Puls bei dominanter Atmung verwerfen und eine echte Pulsfrequenz nahe
einem Atemoberton zurückhalten. Periodische Bewegung oder Gain-Artefakte können
dennoch dieselben mathematischen Kriterien erfüllen. Es gibt weder LSTM noch
Modellsuche, keine Personenklassifikation und keinen physiologischen Nachweis.

## Datenvertrag

- Eingabe: `*.raw-csi.v1.jsonl`, Zeilen mit Schema 1 oder 2. Der Dateisuffix
  blieb auch bei Schema 2 gleich. Das Aufnahme-Sidecar heißt
  `*.raw-csi.v1.meta.json`. Der Recorder nimmt gültige UDP-CSI-Pakete aller RX
  auf; der Import filtert danach ausdrücklich genau einen RX.
- `rx_id` ist die vom RX gemeldete Knotenkennung. Die UDP-Transportadresse
  wird in JSONL **nicht** gespeichert. `source_binding` bezeichnet dagegen
  den im RX geprüften WLAN-Senderfilter: Version 1, Flags exakt `0x07` und
  SHA-256 der sechs konfigurierten MAC-Bytes. Eine Aufnahme wird nie aufgrund
  von RX-ID, Dateiname oder Transportquelle automatisch TX1 zugeordnet.
- `iq_pairs` sind bereits als vorzeichenbehaftete `{i,q}`-Werte normalisiert.
  Der Import verwendet `complex(i,q)` ohne erneutes Vertauschen, Skalieren oder
  Glätten. Die Reihenfolge ist antennenweise; `subcarrier_count` gilt je
  Antenne. Physische Unterträgernummern und genaue LTF-Grenzen werden nicht
  mitgespeichert. `ppdu_type`, `flags` und `csi_config_flags` bleiben als
  Format- und Akquisitionshinweise erhalten; sie ersetzen keine vollständige
  Unterträgerkarte. Bei `first_word_invalid` hat die Firmware bereits bis zu
  vier anfängliche Rohbytes durch null ersetzt; diese Werte sind nicht
  rekonstruierbar. Der Bericht fasst RSSI zusammen; Rauschboden, RX-Zustand,
  Gain und Sync-Alter nur dann, wenn die jeweiligen v3-Metadaten deren
  Gültigkeit oder Verfügbarkeit belegen. Unbestätigte Werte bleiben in den
  importierten Originalzeilen erhalten.
- `host_timestamp_unix_ns` ist die Host-Wanduhr beim Empfang, also die
  gespeicherte UTC-Zuordnung. Schema 2 ergänzt `host_monotonic_ns` und
  `clock_epoch_id`; innerhalb einer Epoche ist dies die bevorzugte Basis für
  Paketabstände. `callback_started_us` und `driver_rx_timestamp_us` sind
  RX-Gerätezeiten, keine TX-Sendezeit. Letztere ist ein 32-Bit-Zähler und kann
  überlaufen. `mesh_timestamp_us` ist optional synchronisierte Gerätezeit,
  **keine** UTC-Zeit. Schema 1 hat keine monotone Hostzeit und keine
  RX-Boot-ID; seine Raten beruhen nur auf der schwächeren Host-Wanduhr.
- Die RX-`sequence` entsteht bei der CSI-Serialisierung. Lücken zwischen
  beobachteten Zählerständen werden als solche ausgegeben. Ohne belegte Zahl
  gesendeter TX-Pakete gibt es **keine Paketverlustquote**. Auch
  `dropped_frames` im Sidecar beschreibt nur den Aufnahmeweg, nicht den
  Funkverlust.
- Ein neuer Abschnitt beginnt bei geändertem Grid/Protokoll, RX-Boot,
  Host-Uhrepoche, Zeitrücksprung, nicht erklärbarem Zählerrücksprung,
  UTC-Abbildungssprung über 500 ms oder einem verworfenen Paket desselben RX.
  `mesh_timestamp_us` ist keine gewählte Zeitbasis; seine Rücksprünge werden
  gezählt, ohne den Host-Zeitabschnitt zu zerlegen. Normale Abstände und Lücken
  bleiben innerhalb eines Abschnitts sichtbar.
  Der Bericht enthält pro Abschnitt Beobachtungsrate aus Zeitspanne,
  Median/P05/P95/Maximum der Ankunftsabstände und Intervalle über dem
  Dreifachen des Abschnitt-Medians. Das sind **keine Sollraten**.
  Die beobachtete Ankunftsrate zählt alle ausgewählten Zeilen; doppelte
  Zeitstempel und RX-Sequenzen werden zusätzlich separat ausgewiesen.

## Vorhandene Daten und Grenzen

Unter `data/recordings/` liegen fünf Aufnahmen vom 9. August 2026 mit insgesamt
18.720 gültigen Schema-1-Zeilen. Jede enthält RX1–RX4 und 64 I/Q-Paare pro
Paket. Vier Sidecars lassen sich mit lokal vorhandenen historischen
Setup-Artefakten verknüpfen; `discovery-neutral-20260809-01` hat keinen solchen
Setup-Beleg und wird nicht als bestätigte TX1-Aufnahme verwendet. Diese alten
Aufnahmen besitzen weder monotone Hostzeit noch RX-Boot-ID oder v3-Qualitätsflags.
Sie sind Leeraum-/Preflight-Daten, keine unabhängig beschrifteten
Vitalwerte-Versuche. RX1 ist für die Beispielberichte nur eine ausdrücklich
gewählte Importspur; daraus folgt noch keine Empfehlung für den physischen
Versuchs-RX. Die neue TX1-Migration steht getrennt weiterhin auf
**7B NO-GO**; dieses Werkzeug ändert daran nichts.

### VITAL-02: historische Diagnose vom 23. September 2026

Erneut mit dem bestehenden Import, ausdrücklich RX1 und den passenden
historischen TX1-Siegeln ausgewertet. Für die `*-01`-Aufnahmen wurde
`private/d6-20260809/sealed-setup.json`, für `*-02` das Siegel unter
`private/d6-20260809-sidecar-fix/` verwendet. Es gab keine neue Messung.

| Aufnahme | RX1-Pakete | Dauer (s) | Ankunftsrate (Hz) | Größter Abstand (ms) | Längstes lückenarmes Teilstück (s) | Verworfene Fenster Atmung/Puls |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| empty-neutral-20260809-01 | 1.138 | 64,943 | 17,508 | 1.280,372 | 1,080 | 2 / 8 |
| empty-neutral-20260809-02 | 1.436 | 64,994 | 22,079 | 447,849 | 4,602 | 2 / 8 |
| preflight-neutral-20260809-01 | 604 | 24,930 | 24,188 | 238,128 | 2,204 | 1 / 1 |
| preflight-neutral-20260809-02 | 608 | 24,848 | 24,428 | 397,218 | 1,798 | 1 / 1 |

Alle **6 Atem- und 18 Pulsfenster sind `not_evaluable`**; kein Kandidat und
keine Schätzung werden ausgegeben. Allen fehlen monotone Hostzeit und RX-Boot-ID;
alle enthalten zu große Abstände. Die beiden Preflights sind zusätzlich kürzer
als beide Untersuchungsfenster. Die längsten Teilstücke oben beziehen sich
auf die jeweilige Abschnitts-Abstandsregel (99,5 / 108,5 / 96,9 / 87,3 ms),
**nicht** auf eine physiologisch auswertbare Dauer. Sie sind sämtlich kürzer
als 30 beziehungsweise 60 Sekunden; auch diese Teilstücke besitzen keine
hinreichend belegte Zeitbasis. Pro Fenster wird der Median erneut berechnet.

Die Zahl der Mesh-Rücksprünge beträgt 119 / 150 / 62 / 44; diese werden weder
geglättet noch als Frequenzzeit verwendet. v3-Qualitätsflags und unabhängige
Vitalreferenzen fehlen ebenfalls. Dateinamen und Labels hatten keinen Einfluss.
Die durchschnittlichen Ankunftsraten allein machen die Aufnahmen nicht geeignet.

Die privaten JSON-Berichte samt Manifest liegen unter
`private/vitals-offline/vital02-20260923-final/`. Jeder Bericht enthält die
Originalzeilen-Grenzen aller untersuchten Fenster und Lücken, Ausschlussgründe,
Konfiguration und Eingabe-/Implementierungs-Hashes. Diese Dateien sind ignoriert
und gehören nicht in Git. Die Tabelle enthält ausschließlich aggregierte Befunde.

## Prüfungen

```sh
python3 -m unittest discover -s software/ruview/tools/vitals_offline -p 'test_*.py' -v
python3 -m compileall -q software/ruview/tools/vitals_offline
ruff check software/ruview/tools/vitals_offline/vitals.py software/ruview/tools/vitals_offline/test_vitals.py
```

Stand 23. September 2026: **38 Tests bestanden** (8 bestehende Importtests und
30 Auswertungstests); Python-Kompilierung und Ruff-Prüfung der beiden neuen
Python-Dateien bestanden. Zusätzlich wurden alle vier historischen Berichte
und ein synthetischer Positivbefund über die CLI erzeugt und geprüft.

Die Tests benötigen keine privaten Aufnahmen. Sie prüfen den unveränderten
VITAL-01-Vertrag sowie bekannte Frequenzen (0,27 und 1,23 Hz) bei 12,5 / 17,51 /
24,43 / 53,7 Paketen/s, langsamere Abtastung, unregelmäßige Zeitpunkte, einzelne
fehlende Pakete, lange Lücken und anschließende Erholung. Eine verdoppelte
aufgezeichnete Zeitachse halbiert die erkannte Frequenz. Weitere Fälle umfassen
Uhr-/Boot-/Grid-/Quellwechsel, Mesh-Rücksprünge, Duplikate, Taktdrift,
Metadatenlücken, fünf Rauschrealisierungen, konstante Signale, Frequenzwechsel,
konkurrierende Peaks, Bandränder, Atemobertöne, fehlenden Atemkontext,
Gain-/RX-Fehler, Null-/Sättigungswerte und ungültige Konfigurationen.
Es gibt ausdrückliche Tests gegen Label-/Dateiname-/Präsenz-Confidence-Einfluss,
alte BPM, fehlende TX-Autorität und das Überschreiben eines Berichts.

## Anforderungen an VITAL-03-Aufnahmen

VITAL-02 erteilt keine Aufnahme-, TX-, Geräte- oder Netzwerkfreigabe. Nach
separater Freigabe muss eine neue Aufnahme mindestens Folgendes ermöglichen:

1. Genau ein ausdrücklich ausgewählter RX mit unabhängig belegter TX1-Bindung;
   unverändertes CSI-Grid/Antennenlayout, stabile Akquisitionskonfiguration und
   dokumentierte physische Unterträger-/LTF-Zuordnung, soweit verfügbar.
2. Monotone Hostzeit mit Uhrepoche, UTC-Zuordnung und RX-Boot-ID; RX-Callback-
   und Driver-Zeit zur Prüfung des Empfangs-/Transporttakts. Keine Rücksprünge,
   unbemerkten Resets oder langen Ausfälle innerhalb auswertbarer Fenster.
   Die tatsächlichen Abstände müssen die konfigurierten Kriterien erfüllen;
   für den aktuellen Pulszweig insbesondere höchstens etwa 115 ms, gegebenenfalls
   strenger durch den gemessenen Median. Eine nominale Senderate genügt nicht.
3. Vollständige v3-Qualitätsmetadaten einschließlich belegtem Rohprofil,
   `first_word_invalid`, gültigem RX-Zustand und nachvollziehbarer Gain-/Rausch-
   Verfügbarkeit. Nicht verfügbare Felder ehrlich als unbekannt kennzeichnen.
4. Mehrere vollständige 60-s-Atem-/30-s-Pulsfenster, vorzugsweise mindestens
   zwei Minuten pro ruhigem Zustand, für Aktualisierungen alle 5 s. Eine ruhige
   Einzelperson sowie getrennte Leeraum- und Bewegungs-Kontrollabschnitte;
   Labels ausschließlich für die spätere Auswertung, niemals als Schätzprior.
5. Unabhängige, zeitgestempelte Atem- und Pulsreferenzen mit dokumentiertem
   Zeitabgleich und Unsicherheit. Vorab festgelegte Auswertung einschließlich
   Nichtausgaberate, Fehlern und Fehlalarmen; getrennte Entwicklungs- und
   unabhängige Prüfabschnitte. Erst diese Gegenprüfung kann physiologische
   Aussagen ermöglichen. Die synthetischen Tests ersetzen sie nicht.
