import {
  getServerControlOrigin,
  serverControlRequest,
} from '../services/server-control.service.js';

function escapeHTML(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

export class ServerControlPanel {
  constructor(container, { onServerAction = null } = {}) {
    this.container = container;
    this.onServerAction = onServerAction;
    this.origin = getServerControlOrigin();
    this.status = null;
    this.available = false;
    this.busy = false;
    this.message = '';
    this.error = '';
    this._mounted = false;
    this._pollTimer = null;
    this._refreshInFlight = false;
  }

  mount() {
    if (this._mounted || !this.container) return;
    this._mounted = true;
    this.container.addEventListener('click', (event) => {
      const action = event.target.closest?.('[data-server-action]')?.dataset.serverAction;
      if (!action) return;
      event.preventDefault();
      if (action === 'refresh') {
        void this.refresh();
      } else {
        void this._runAction(action);
      }
    });
    this._render();
    void this.refresh();
    this._pollTimer = setInterval(() => void this.refresh({ quiet: true }), 3000);
  }

  dispose() {
    this._mounted = false;
    if (this._pollTimer) clearInterval(this._pollTimer);
    this._pollTimer = null;
  }

  async refresh({ quiet = false } = {}) {
    if (!this._mounted || this._refreshInFlight) return;
    this._refreshInFlight = true;
    try {
      this.status = await serverControlRequest('status');
      this.available = true;
      if (this.status?.error) {
        this.error = this.status.error;
        this.message = '';
      } else {
        if (!quiet) this.message = '';
        this.error = '';
      }
    } catch (error) {
      this.available = false;
      this.status = null;
      if (!quiet || !this.error) {
        this.error = error instanceof Error ? error.message : String(error);
      }
    } finally {
      this._refreshInFlight = false;
      this._render();
    }
  }

  async _runAction(action) {
    if (this.busy || this.status?.busy === true || !this.available) return;
    this.busy = true;
    this.message = action === 'restart'
      ? 'Server wird neu gestartet …'
      : action === 'stop'
        ? 'Server wird beendet …'
        : 'Server wird gestartet …';
    this.error = '';
    this._render();
    try {
      const result = await serverControlRequest(action);
      this.message = result.message || 'Aktion abgeschlossen.';
      await this.refresh({ quiet: true });
      if (['start', 'restart'].includes(action)) {
        await this.onServerAction?.(action, result);
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
    const running = this.status?.running === true;
    const controllerLabel = !this.available
      ? 'NICHT ERREICHBAR'
      : this.status?.busy || this.busy
        ? 'AKTION LÄUFT'
        : running
          ? `LÄUFT · PID ${this.status.pid ?? '—'}`
          : 'GESTOPPT';
    const stateClass = !this.available ? 'is-offline' : running ? 'is-running' : 'is-stopped';
    const actionDisabled = this.busy || this.status?.busy === true || !this.available;
    const offlineHint = !this.available
      ? `<p class="server-control-hint">Der Browser-Control-Helper ist nicht erreichbar. Starte den Sensing-Server einmal mit der aktuellen Binary; danach bleiben diese Buttons auch beim Stoppen verfügbar.</p>`
      : `<p class="server-control-hint">Control-Helper: <code>${escapeHTML(this.origin)}</code> · UI: <code>${escapeHTML(this.status?.ui_url || '—')}</code></p>`;
    const feedback = this.error
      ? `<div class="server-control-feedback is-error" role="alert">${escapeHTML(this.error)}</div>`
      : this.message
        ? `<div class="server-control-feedback" role="status">${escapeHTML(this.message)}</div>`
        : '';

    this.container.innerHTML = `
      <section class="server-control-panel" aria-labelledby="serverControlTitle">
        <header class="server-control-header">
          <div>
            <div class="server-control-kicker">LOCAL RUNTIME</div>
            <h3 id="serverControlTitle">Serversteuerung</h3>
            <p>Start, Stopp und Neustart direkt aus dem Browser.</p>
          </div>
          <span class="server-control-state ${stateClass}" role="status" aria-live="polite">${escapeHTML(controllerLabel)}</span>
        </header>
        <div class="server-control-actions">
          <button type="button" class="server-control-button is-primary" data-server-action="start" ${actionDisabled || running ? 'disabled' : ''}>Server starten</button>
          <button type="button" class="server-control-button" data-server-action="restart" ${actionDisabled || !running ? 'disabled' : ''}>Neu starten</button>
          <button type="button" class="server-control-button is-danger" data-server-action="stop" ${actionDisabled || !running ? 'disabled' : ''}>Server stoppen</button>
          <button type="button" class="server-control-button is-quiet" data-server-action="refresh" ${this.busy ? 'disabled' : ''}>Status prüfen</button>
        </div>
        ${feedback}
        ${offlineHint}
      </section>
    `;
  }
}
