# D4/D5/D6 chart map and QA

| Figure | Decision question | Source | Denominator / transform | Visual form | QA result |
|---|---|---|---|---|---|
| `figures/01-globaler-vergleich.png` | Did lower false presence preserve still recall? | D4 raw samples, D5 replay output, D5 live and D5-abs raw samples | eligible samples; replay after full 10-s window | Bklit grouped bar chart with shared percentage axis | inspected at 1876×1294; zero bars and live-FPR caveat legible |
| `figures/02-d4-rx-leerraum-heatmap.png` | Which local radio paths caused D4 empty presence? | E0b/E0c/E0d `node_features.classification.presence` | 237 samples per run/RX | Bklit heatmap plus exact-value matrix | inspected; all 12 cells and exact percentages legible |
| `figures/03-d5-live-rx-linkwechsel.png` | Why did the real D5 quorum fail? | D5 E1 and E1 Persistenz `d5_presence.vote` | 236 and 114 samples | Bklit line chart to show path switching between the two states | inspected; RX3/RX4 switch and global 0/350 boundary visible |
| `figures/04-d6-rx-frameraten.png` | Are all receivers represented at plausible throughput? | D6 raw host timestamps and frame counts | per-RX frames divided by whole-file raw span | Bklit grouped bar chart for five setup-bound recordings | inspected; five recordings and four RX series visible |

Design constraints: zero-based axes, restrained neutral palette from Bklit tokens, direct labels where interpretation matters, no smoothed trend lines, no hidden denominators. The current figures were rendered with the official Bklit UI source and visually inspected as PNGs; the maintained exports live beside this report in `figures/`.

## Historischer Exportkontext

Die vier ursprünglichen Bklit-Dateinamen waren `01_globaler_vergleich.png`, `02_D4_RX_leerraum_heatmap.png`, `03_D5_live_RX_linkwechsel.png` und `04_D6_RX_frameraten.png`; sie entsprechen in dieser Reihenfolge den vier oben genannten Paketbildern. [Bklit-Exporte](../../software/ruview/results/2026-08-27_D4-D5-D6_bklit_figures/) und [ursprüngliche Software-Abbildungen](../../software/ruview/results/2026-08-23_D4-D5-D6_figures/) bleiben unverändert erhalten.

Die Tabellenformulierung „five setup-bound recordings“ gehört zum damaligen Diagrammvertrag: Die detaillierte Berichtstabelle unterscheidet tatsächlich vier setupgebundene Aufnahmen und eine Discovery-Inventur ohne Setupbindung. Bildwerte und ursprüngliche QA-Angaben werden dadurch nicht rückwirkend zu einer neuen Messung.
