import { experimentService } from '../services/experiment.service.js';

export const SYNTHETIC_FIXTURE_ID = 'mmwave-synthetic-pass-status-v1';

export function experimentConsoleViewModel(status, runs = [], selectedRun = null) {
  const available = status?.available === true && status?.status === 'READY';
  return {
    available,
    statusLabel: available ? 'PERSISTENCE READY' : 'PERSISTENCE UNAVAILABLE',
    canStart: available,
    runCount: Number.isFinite(Number(status?.run_count))
      ? Number(status.run_count)
      : runs.length,
    selectedRun: selectedRun || runs[0] || null,
    validationLabel: selectedRun?.validation_status || 'UNVALIDATED',
    livePositionApproved: selectedRun?.live_position_approved === true,
  };
}

function escapeHTML(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

function timestampLabel(value) {
  if (!value) return '--';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString('de-DE');
}

function runStatusClass(run) {
  if (run?.state === 'completed') return 'is-complete';
  if (run?.state === 'failed') return 'is-error';
  if (run?.state === 'running') return 'is-running';
  return 'is-created';
}

export class ExperimentConsole {
  constructor(container) {
    this.container = container;
    this.status = null;
    this.runs = [];
    this.selectedRun = null;
    this.error = '';
    this.loading = false;
    this.busy = false;
    this.label = 'Synthetic replay before mmWave';
    this._mounted = false;
  }

  mount() {
    if (this._mounted) return;
    this._mounted = true;
    this.container.innerHTML = this._shell();
    this.container.addEventListener('click', (event) => this._onClick(event));
    this.container.addEventListener('submit', (event) => this._onSubmit(event));
    void this.refresh();
  }

  dispose() {
    this._mounted = false;
  }

  _shell() {
    return `
      <section class="experiment-console" aria-labelledby="experimentConsoleTitle">
        <div class="experiment-console-header">
          <div>
            <div class="experiment-eyebrow">OBSERVATORY CONTROL CENTER / SOFTWARE LAYER</div>
            <h3 id="experimentConsoleTitle">Replay-Experimente</h3>
            <p>Ein reproduzierbarer Lauf für heute: SQLite katalogisiert den Run, die Rohdaten bleiben außerhalb der Datenbank.</p>
          </div>
          <div class="experiment-persistence-status is-loading" id="experimentPersistence" role="status" aria-live="polite">PERSISTENCE WIRD GEPRÜFT</div>
        </div>

        <div class="experiment-console-grid">
          <div class="experiment-command-panel">
            <div class="experiment-panel-kicker">RUNNER</div>
            <h4>Synthetic Replay starten</h4>
            <p>Nur Fixture-Verarbeitung. Keine CSI-Aufnahme, keine mmWave-Steuerung, keine Live-Freigabe.</p>
            <form id="experimentReplayForm">
              <label class="experiment-field">
                <span>Versuchsname</span>
                <input id="experimentLabel" name="label" maxlength="120" required>
              </label>
              <label class="experiment-field">
                <span>Fixture</span>
                <select id="experimentFixture" name="fixture_id" disabled>
                  <option value="${SYNTHETIC_FIXTURE_ID}">Synthetic contract v1</option>
                </select>
              </label>
              <button type="submit" class="experiment-primary-button" id="experimentReplayButton">Synthetic Replay starten</button>
            </form>
            <p class="experiment-helper" id="experimentCommandMessage">SQLite-Status wird geladen.</p>
          </div>

          <div class="experiment-scope-panel">
            <div class="experiment-panel-kicker">EVIDENCE BOUNDARY</div>
            <dl class="experiment-facts">
              <div><dt>Ausführung</dt><dd>SOFTWARE REPLAY</dd></div>
              <div><dt>Validierung</dt><dd>UNVALIDATED</dd></div>
              <div><dt>Live-Position</dt><dd>LOCKED</dd></div>
              <div><dt>Hardware</dt><dd>NO LIVE HARDWARE</dd></div>
            </dl>
            <p class="experiment-scope-note">Ein grüner Replay-Status ist ein Software-Nachweis. Er ersetzt keine Messung und keinen Blindtest.</p>
          </div>
        </div>

        <div class="experiment-result-panel" id="experimentResult" aria-live="polite"></div>

        <div class="experiment-history-panel">
          <div class="experiment-history-header">
            <div>
              <div class="experiment-panel-kicker">VERSUCHSVERLAUF</div>
              <h4>Letzte Runs</h4>
            </div>
            <button type="button" data-experiment-action="refresh" class="experiment-secondary-button">Aktualisieren</button>
          </div>
          <div id="experimentRunList"></div>
        </div>
      </section>
    `;
  }

  async refresh() {
    if (!this._mounted || this.loading) return;
    this.loading = true;
    this.error = '';
    this._render();
    try {
      const status = await experimentService.getStatus();
      this.status = status;
      if (status?.available) {
        this.runs = await experimentService.listRuns(50);
      } else {
        this.runs = [];
      }
      if (this.selectedRun) {
        this.selectedRun = this.runs.find((run) => run.id === this.selectedRun.id) || this.selectedRun;
      } else {
        this.selectedRun = this.runs[0] || null;
      }
    } catch (error) {
      this.error = error?.message || 'Experiment-API ist nicht erreichbar.';
    } finally {
      this.loading = false;
      this._render();
    }
  }

  async _onSubmit(event) {
    if (event.target.id !== 'experimentReplayForm') return;
    event.preventDefault();
    if (this.busy || !this.status?.available) return;

    const data = new FormData(event.target);
    const label = String(data.get('label') || '').trim();
    if (!label) {
      this.error = 'Bitte einen Versuchsname eintragen.';
      this._render();
      return;
    }

    this.label = label;
    this.busy = true;
    this.error = '';
    this._render();
    try {
      const created = await experimentService.createRun({
        label,
        fixtureId: SYNTHETIC_FIXTURE_ID,
      });
      this.selectedRun = created;
      this.selectedRun = await experimentService.replayRun(created.id);
    } catch (error) {
      this.error = error?.message || 'Synthetic Replay konnte nicht ausgeführt werden.';
    } finally {
      this.busy = false;
      await this.refresh();
    }
  }

  async _onClick(event) {
    const action = event.target.closest('[data-experiment-action]')?.dataset.experimentAction;
    if (action === 'refresh') {
      await this.refresh();
      return;
    }
    const runId = event.target.closest('[data-experiment-run-id]')?.dataset.experimentRunId;
    if (!runId || this.busy) return;
    const run = this.runs.find((candidate) => candidate.id === runId);
    if (run) {
      this.selectedRun = run;
      this._render();
    }
  }

  _render() {
    if (!this._mounted) return;
    const model = experimentConsoleViewModel(this.status, this.runs, this.selectedRun);
    const persistence = this.container.querySelector('#experimentPersistence');
    const button = this.container.querySelector('#experimentReplayButton');
    const input = this.container.querySelector('#experimentLabel');
    const fixture = this.container.querySelector('#experimentFixture');
    const commandMessage = this.container.querySelector('#experimentCommandMessage');
    const runList = this.container.querySelector('#experimentRunList');
    const result = this.container.querySelector('#experimentResult');

    if (!persistence || !button || !input || !fixture || !commandMessage || !runList || !result) return;

    persistence.textContent = this.loading && !this.status
      ? 'PERSISTENCE WIRD GEPRÜFT'
      : model.statusLabel;
    persistence.className = `experiment-persistence-status ${this.loading ? 'is-loading' : (model.available ? 'is-ready' : 'is-unavailable')}`;
    button.disabled = this.busy || !model.canStart;
    input.disabled = this.busy || !model.canStart;
    fixture.disabled = true;
    input.value = this.label;

    if (this.error) {
      commandMessage.className = 'experiment-helper is-error';
      commandMessage.textContent = this.error;
    } else if (!model.available) {
      commandMessage.className = 'experiment-helper is-error';
      commandMessage.textContent = this.status?.message || 'SQLite ist nicht verfügbar. Der Control Center bleibt gesperrt.';
    } else if (this.busy) {
      commandMessage.className = 'experiment-helper is-running';
      commandMessage.textContent = 'Run wird geschrieben, Report wird gehasht und in SQLite registriert.';
    } else {
      commandMessage.className = 'experiment-helper';
      commandMessage.textContent = `${model.runCount} Run(s) katalogisiert. Fixture ist fest vorgegeben.`;
    }

    if (this.loading && this.runs.length === 0) {
      runList.innerHTML = '<div class="experiment-empty-state is-loading">Runs werden geladen.</div>';
    } else if (this.runs.length === 0) {
      runList.innerHTML = '<div class="experiment-empty-state">Noch kein Run. Starte den Synthetic Replay oben.</div>';
    } else {
      runList.innerHTML = this.runs.map((run) => `
        <button type="button" class="experiment-run-row ${run.id === this.selectedRun?.id ? 'is-selected' : ''}" data-experiment-run-id="${escapeHTML(run.id)}">
          <span class="experiment-run-state ${runStatusClass(run)}"></span>
          <span class="experiment-run-main"><strong>${escapeHTML(run.label)}</strong><small>${escapeHTML(run.kind)} · ${escapeHTML(timestampLabel(run.created_at))}</small></span>
          <span class="experiment-run-meta"><strong>${escapeHTML(run.execution_status)}</strong><small>${escapeHTML(run.validation_status)}</small></span>
        </button>
      `).join('');
    }

    result.innerHTML = this._resultMarkup(model.selectedRun);
  }

  _resultMarkup(run) {
    if (!run) {
      return '<div class="experiment-result-empty">Nach dem ersten Replay erscheinen hier Zustand, Artefakt und Hash.</div>';
    }
    const artifact = Array.isArray(run.artifacts) ? run.artifacts[0] : null;
    const failed = run.state === 'failed';
    return `
      <div class="experiment-result-header">
        <div>
          <div class="experiment-panel-kicker">AUSGEWÄHLTER RUN</div>
          <h4>${escapeHTML(run.label)}</h4>
        </div>
        <span class="experiment-result-badge ${failed ? 'is-error' : 'is-pass'}">${escapeHTML(run.execution_status)}</span>
      </div>
      <div class="experiment-result-facts">
        <div><span>Phase</span><strong>${escapeHTML(run.phase)}</strong></div>
        <div><span>Ausführung</span><strong>SOFTWARE REPLAY</strong></div>
        <div><span>Validierung</span><strong>UNVALIDATED</strong></div>
        <div><span>Live-Freigabe</span><strong>LOCKED</strong></div>
      </div>
      ${failed ? `<p class="experiment-result-error">${escapeHTML(run.error_message || 'Replay fehlgeschlagen.')}</p>` : ''}
      ${artifact ? `
        <dl class="experiment-artifact">
          <div><dt>Artefakt</dt><dd>${escapeHTML(artifact.relative_path)}</dd></div>
          <div><dt>SHA-256</dt><dd>${escapeHTML(artifact.sha256)}</dd></div>
        </dl>
      ` : '<p class="experiment-helper">Noch kein Report-Artefakt vorhanden.</p>'}
    `;
  }
}
