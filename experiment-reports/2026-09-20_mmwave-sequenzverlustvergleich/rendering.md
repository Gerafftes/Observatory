# Bklit-Render-Spezifikation — mmWave-Sequenzverlustvergleich

Renderdatum: 20. September 2026  
Quellbericht: [`README.md`](README.md)  
Maschinenlesbare Werte: [`data/vergleich.csv`](data/vergleich.csv)

## Zweck

Die zwei bereitgestellten 60-Sekunden-Fenster werden als drei reproduzierbare
PNG-Dateien dargestellt. Der Renderer verwendet die bestehende neutrale
Bklit-Sprache des Projekts: weiße Fläche, abgerundete Balken, Nullachsen,
direkte Wertelabels und getrennte Skalen für unterschiedliche Einheiten.

## Zuordnung

| Export | Darstellung | Beibehaltene Werte |
|---|---|---|
| `figures/01-sequenzverlust.png` | Balken plus Messumfang-Panel | Verlustquote, verlorene und neue Sequenzen |
| `figures/02-ack-zustellung.png` | zwei getrennte Balkendiagramme | bestätigte Pakete und vollständige ACK-Timeouts |
| `figures/03-duplikate-und-gate.png` | Balkendiagramm plus Statuskarten | Duplikate und `radar_sequence_loss_free` |

## Schutz vor Fehlinterpretation

- Prozentwerte und absolute Anzahlen haben getrennte Nullskalen.
- Beide Firmwarestände werden ausschließlich innerhalb ihrer gleich langen
  60-Sekunden-Fenster verglichen.
- ACK-Zähler, Timeout-Zähler und Sequenzzahlen werden nicht zu einer
  künstlichen Identität verrechnet.
- `0,00 %` und das bestandene Gate gelten für das beobachtete Fenster, nicht
  als allgemeine Verlustfreiheitsgarantie.
- Der Transportvergleich enthält keine Aussage zur Positionsgenauigkeit.

## Reproduktion

```bash
python3 project_tools/build_mmwave_sequence_loss_bklit.py
```

Der Renderer schreibt ausschließlich in
`experiment-reports/2026-09-20_mmwave-sequenzverlustvergleich/figures/`.

## QA

- Drei PNGs mit jeweils `1876 × 1294` Pixeln.
- Alle quantitativen Balken beginnen bei null.
- Nullwerte bleiben als Punkt auf der Grundlinie sichtbar.
- Werte, Einheiten, Firmwarestände und 60-Sekunden-Umfang stimmen mit der CSV
  und dem Quellbericht überein.
- Die drei Exporte wurden nach dem Rendern in Originalauflösung visuell auf
  abgeschnittene oder überlappende Texte geprüft.
