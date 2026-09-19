import { controlHelperRequest, getControlHelperOrigin } from '../services/control-helper.service.js';

function escapeHTML(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

export class LocalControlPanel {
  constructor(container) {
    this.container = container;
    this.origin = getControlHelperOrigin();
    this.available = false;
    this.info = null;
    this.nodes = [];
    this.ports = [];
    this.busy = false;
    this.message = '';
    this.error = '';
    this._mounted = false;
  }

  mount() {
    if (this._mounted || !this.container) return;
    this._mounted = true;
    this.container.addEventListener('click', (event) => {
      const action = event.target.closest?.('[data-helper-action]')?.dataset.helperAction;
      if (!action || this.busy) return;
      event.preventDefault();
      void this._runAction(action);
    });
    this._render();
    void this.refresh();
  }

  dispose() {
    this._mounted = false;
  }

  async refresh() {
    if (!this._mounted || this.busy) return;
    try {
      this.info = await controlHelperRequest('/api/v1/helper/info');
      this.available = this.info?.capabilities?.includes('node_discovery') === true;
      this.error = this.available
        ? ''
        : 'Der Server-Control-Helper läuft, aber der native Hardware-Helper ist nicht aktiviert.';
    } catch (error) {
      this.available = false;
      this.info = null;
      this.error = error instanceof Error ? error.message : String(error);
    }
    this._render();
  }

  async _runAction(action) {
    if (action === 'refresh') {
      await this.refresh();
      return;
    }

    this.busy = true;
    this.message = action === 'discover' ? 'Suche im lokalen Netz …' : 'Serielle Ports werden gelesen …';
    this.error = '';
    this._render();
    try {
      if (action === 'discover') {
        this.nodes = await controlHelperRequest('/api/v1/nodes/discover?timeout_ms=1500');
        this.message = `${this.nodes.length} Node${this.nodes.length === 1 ? '' : 's'} gefunden.`;
      } else if (action === 'ports') {
        this.ports = await controlHelperRequest('/api/v1/serial/ports');
        this.message = `${this.ports.length} serielle${this.ports.length === 1 ? 'r Port' : ' Ports'} gefunden.`;
      }
    } catch (error) {
      this.error = error instanceof Error ? error.message : String(error);
    } finally {
      this.busy = false;
      this._render();
    }
  }

  _render() {
    if (!this.container) return;
    const state = !this.available ? 'NICHT ERREICHBAR' : 'BEREIT';
    const stateClass = this.available ? 'is-running' : 'is-offline';
    const disabled = this.busy || !this.available;
    const nodeSummary = this.nodes.length
      ? this.nodes.map((node) => `${escapeHTML(node.ip || '—')} · ${escapeHTML(node.mac || 'ohne MAC')}`).join('<br>')
      : 'Noch keine Discovery ausgeführt.';
    const portSummary = this.ports.length
      ? this.ports.map((port) => `${escapeHTML(port.name)}${port.is_esp32_compatible ? ' · ESP32-kompatibel' : ''}`).join('<br>')
      : 'Noch keine Portliste geladen.';
    const feedback = this.error
      ? `<div class="server-control-feedback is-error" role="alert">${escapeHTML(this.error)}</div>`
      : this.message
        ? `<div class="server-control-feedback" role="status">${escapeHTML(this.message)}</div>`
        : '';

    this.container.innerHTML = `
      <section class="local-control-panel server-control-panel" aria-labelledby="localControlTitle">
        <header class="server-control-header">
          <div>
            <div class="server-control-kicker">LOCAL CONTROL HELPER</div>
            <h3 id="localControlTitle">Lokale Hardware-Steuerung</h3>
            <p>Discovery und USB-Seriell-Zugriff bleiben im Browser verfügbar.</p>
          </div>
          <span class="server-control-state ${stateClass}" role="status">${state}</span>
        </header>
        <div class="server-control-actions">
          <button type="button" class="server-control-button is-primary" data-helper-action="discover" ${disabled ? 'disabled' : ''}>Nodes suchen</button>
          <button type="button" class="server-control-button" data-helper-action="ports" ${disabled ? 'disabled' : ''}>Ports laden</button>
          <button type="button" class="server-control-button is-quiet" data-helper-action="refresh" ${this.busy ? 'disabled' : ''}>Helper prüfen</button>
        </div>
        <div class="local-control-columns">
          <div><strong>Nodes</strong><p>${nodeSummary}</p></div>
          <div><strong>Serielle Ports</strong><p>${portSummary}</p></div>
        </div>
        ${feedback}
        <p class="server-control-hint">Helper: <code>${escapeHTML(this.origin)}</code>${this.info ? ` · Version ${escapeHTML(this.info.helper_version)}` : ''}</p>
      </section>
    `;
  }
}
