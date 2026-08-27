# Bklit render specification — D4/D5/D6

Render date: 27 August 2026<br>
Purpose: replace the four result figures with Bklit UI chart components without changing measurements, denominators, thresholds, or classifications.

## Source

- Official repository: <https://github.com/bklit/bklit-ui>
- Rendered source revision: `c57f66bfa7c3198edb677b567ce08cbf364ae159`
- Component package: `@bklitui/ui/charts`
- Studio reference: <https://bklit.com/studio>
- Data source: [`2026-08-23_D4-D5-D6_laufuebersicht.csv`](2026-08-23_D4-D5-D6_laufuebersicht.csv) and [`2026-08-23_D4_RX_diagnostik.csv`](2026-08-23_D4_RX_diagnostik.csv)

The Bklit Studio uses generated demo data and has no CSV import. The figures therefore use the official component API directly with the already audited values. No new recording or threshold change was performed.

## Figure mapping

| Output | Bklit component | Values retained |
|---|---|---|
| `01_globaler_vergleich.png` | `BarChart` + `Bar` + `BarXAxis` + `YAxis` + `Grid` | D4 pooled FPR 75.246%, recall 88.397%; D5-abs 0/0%; D5 replay 0/89.340% |
| `02_D4_RX_leerraum_heatmap.png` | `HeatmapChart` + `HeatmapCells` | E0b/E0c/E0d × RX1–RX4 exact percentages, with table labels retained because Bklit heatmap levels are discrete |
| `03_D5_live_RX_linkwechsel.png` | `LineChart` + `Line` + `YAxis` + `Grid` | E1 n=236 and Persistence n=114 per-RX vote percentages |
| `04_D6_RX_frameraten.png` | `BarChart` + `Bar` + `BarXAxis` + `YAxis` + `Grid` | D6 per-RX raw-frame rates from actual host timestamp spans |

## QA

- All four PNGs were rendered at 2× device scale and visually checked.
- Zero-based axes are used where percentages or rates are shown.
- The D5 live panel explicitly preserves the unpaired-FPR caveat.
- D6 `empty-neutral-01` and `empty-neutral-02` remain separate setup series.
- The original figures are not deleted; this Bklit set is linked from the technical report.

## Credits

[Bklit UI](https://bklit.com) · [`@bklitui/ui/charts`](https://github.com/bklit/bklit-ui)
