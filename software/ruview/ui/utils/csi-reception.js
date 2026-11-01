const COUNTING_SPAN = 'Kumulativ seit Beginn des RX-/Serverzustands; Neustart bei erkanntem RX-Bootwechsel.';

export const CSI_RATE_HELP = 'Schätzung aus Sync-Sequenzen.';
export const FORMAT_EXCLUSIONS_HELP = `Gültige neue CSI-Frames außerhalb des aktiven Auswerterasters. Prozentnenner: alle gültigen neuen Frames am Rastergate. ${COUNTING_SPAN}`;
export const SEQUENCE_GAPS_HELP = `Fehlende Sequenzpositionen zwischen zugelassenen gültigen CSI-Empfängen, einschließlich Off-Grid-Empfängen. Prozentnenner: beobachtete Vorwärtsübergänge plus fehlende Positionen. Ursache ungeklärt. ${COUNTING_SPAN} Die beiden Quoten haben unterschiedliche Nenner.`;

export function formatCsiReceptionMetric(node, metricName) {
  if (node.status !== 'active') return '-- (keine aktuellen Daten)';
  const metric = node.csi_reception?.[metricName];
  if (typeof metric?.count !== 'number' || !Number.isFinite(metric.count)) {
    return 'nicht verfügbar';
  }
  const percent = metric.valid === true
    && typeof metric.denominator === 'number' && metric.denominator > 0
    && typeof metric.percent === 'number' && Number.isFinite(metric.percent)
    ? `${metric.percent.toFixed(1)}%`
    : '--';
  return `${metric.count} · ${percent}`;
}
