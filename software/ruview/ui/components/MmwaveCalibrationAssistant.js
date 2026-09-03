const MM_WAVE_STATUS_ENDPOINT = '/api/v1/mmwave/status';
const MM_WAVE_SESSION_START_ENDPOINT = '/api/v1/mmwave/session/start';
const MM_WAVE_SESSION_STOP_ENDPOINT = '/api/v1/mmwave/session/stop';
const STEP_LABELS = ['Link', 'Ausrichtung', 'Fläche', 'Segmente', 'CSI', 'Blindtest', 'Ergebnis'];
const EMPTY_REFERENCE_MIN_SECONDS = 60;
const EMPTY_REFERENCE_DEFAULT_SECONDS = 65;
const CALIBRATION_MIN_LEAD_SECONDS = 5;
const CALIBRATION_DEFAULT_LEAD_SECONDS = 20;
const CALIBRATION_TIMER_MS = 250;

export function mmwaveAssistantViewModel(status) {
  const rawSession = status?.session || null;
  const sessionLifecycle = rawSession?.lifecycle || 'active';
  const session = rawSession && [
    'active',
    'complete',
    'error',
    'interrupted',
  ].includes(sessionLifecycle)
    ? rawSession
    : null;
  const zones = Array.isArray(status?.zones) ? status.zones : [];
  const zoneCount = Number(status?.zone_count) || 9;
  const trainingComplete = zones.length === zoneCount
    && zones.every((zone) => Number(zone.training_blocks) >= 6);
  const blindComplete = zones.length === zoneCount
    && zones.every((zone) => Number(zone.blind_visits) >= 2);
  const connected = status && !['disconnected', 'stale', 'invalid'].includes(status.state);

  let activeStep = 0;
  if (connected) activeStep = 1;
  if (status?.transform) activeStep = 2;
  if (Number(status?.coverage_cells) > 0) activeStep = 3;
  if (zones.length === zoneCount) activeStep = 4;
  if (trainingComplete) activeStep = 5;
  if (blindComplete) activeStep = 6;

  const phase = session?.phase || null;
  if (phase === 'coverage') activeStep = 2;
  if (phase === 'training') activeStep = 4;
  if (phase === 'blind') activeStep = 5;
  if (phase === 'complete' && session?.kind === 'blind') activeStep = 6;

  return {
    activeStep,
    blindComplete,
    connected,
    phase,
    session,
    sessionInterrupted: session?.lifecycle === 'interrupted',
    sessionErrored: session?.lifecycle === 'error',
    trainingComplete,
    zoneCount,
    zones,
  };
}

export function mmwaveTransportDiagnostic(status) {
  if (status?.node_control?.reachable === false) {
    const errorLabel = ({
      timeout: 'Status-Timeout',
      invalid_json: 'ungültiges Status-JSON',
      http_error: 'HTTP-Fehler beim Status',
      unreachable: 'Verbindung fehlgeschlagen',
    })[status.node_control.last_error_kind] || 'Statusabfrage fehlgeschlagen';
    return {
      state: 'unavailable',
      message: `ESP nicht erreichbar: ${errorLabel}. WLAN, Node-URL und Versorgung prüfen.`,
    };
  }
  if (status?.node_status_error) {
    return { state: 'unavailable', message: 'ESP-Status fehlt.' };
  }
  if (status?.state === 'stale') {
    return { state: 'radar_interrupted', message: 'Radar verbunden, aber Datenstrom unterbrochen.' };
  }
  if ([status?.uart_bytes_received, status?.radar_frames_valid, status?.udp_packets_sent]
    .some((value) => value === null || value === undefined)) {
    return { state: 'unavailable', message: 'ESP-Diagnose fehlt.' };
  }
  const uartBytes = Number(status.uart_bytes_received);
  const validFrames = Number(status.radar_frames_valid);
  const udpSent = Number(status.udp_packets_sent);
  if (![uartBytes, validFrames, udpSent].every(Number.isFinite)) {
    return { state: 'unavailable', message: 'ESP-Diagnose fehlt.' };
  }
  if (uartBytes === 0) {
    return { state: 'uart_idle', message: 'Keine UART-Bytes. Versorgung, TX→RX, GPIO20, Baudrate prüfen.' };
  }
  if (validFrames === 0) {
    return { state: 'invalid_frames', message: 'Bytes da, aber kein LD2450-Frame. Leitung/Baudrate prüfen.' };
  }
  if (udpSent === 0) {
    return { state: 'udp_blocked', message: 'Radarframes da, aber kein UDP vom ESP.' };
  }
  return { state: 'streaming', message: 'UART, Parser und UDP liefern Daten.' };
}

const PREFLIGHT_GATE_LABELS = {
  node_control_configured: 'ESP-Steuerung',
  setup_and_transform_sealed: 'Setup und Ausrichtung',
  radar_stream_fresh: 'Radar-Transport frisch',
  radar_sequence_loss_free: 'Radar-Sequenz lückenfrei',
  csi_v2_clock: 'CSI-Zeitstempel',
  node_diagnostics_streaming: 'ESP-Diagnose',
};

function preflightGateLabel(id) {
  if (PREFLIGHT_GATE_LABELS[id]) return PREFLIGHT_GATE_LABELS[id];
  if (id?.startsWith('rx') && id.endsWith('_25s_ready')) {
    return `${id.slice(0, 3).toUpperCase()}-Stream 25 s`;
  }
  return id || 'Unbekanntes Gate';
}

function escapeHTML(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

function stateLabel(state) {
  return ({
    disconnected: 'OFFLINE',
    stale: 'ALT',
    no_target: 'KEIN ZIEL',
    multi_target: 'MEHRERE',
    invalid: 'UNGÜLTIG',
    valid: 'BEREIT',
  })[state] || 'PRÜFE …';
}

function formatClockTime(value) {
  if (!Number.isFinite(value)) return '--:--:--';
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? '--:--:--'
    : date.toLocaleTimeString('de-DE', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
}

export class MmwaveCalibrationAssistant {
  constructor(container, calibrationContextProvider = () => null) {
    this.container = container;
    this.calibrationContextProvider = calibrationContextProvider;
    this.timer = null;
    this.busy = false;
    this.refreshInFlight = false;
    this.status = null;
    this.error = '';
    this.actionError = '';
    this.statusError = '';
    this.calibrationPlan = null;
    this.calibrationTimer = null;
  }

  mount() {
    this.container.innerHTML = this._shell();
    this.container.addEventListener('click', (event) => this._onClick(event));
    this.container.addEventListener('submit', (event) => this._onSubmit(event));
    this.refresh();
    this.timer = window.setInterval(() => this.refresh(), 1000);
  }

  dispose() {
    if (this.timer !== null) {
      window.clearInterval(this.timer);
      this.timer = null;
    }
    this._clearCalibrationTimer();
  }

  _syncLegacyError() {
    this.error = [this.statusError, this.actionError].filter(Boolean).join(' · ');
  }

  _setActionError(message) {
    this.actionError = message || '';
    this._syncLegacyError();
  }

  _clearActionError() {
    this._setActionError('');
  }

  _setStatusError(message) {
    this.statusError = message || '';
    this._syncLegacyError();
  }

  _shell() {
    return `
      <section class="mmwave-assistant" aria-labelledby="mmwaveAssistantTitle">
        <div class="mmwave-assistant-header">
          <div>
            <div class="mmwave-eyebrow">RADAR-REFERENZ</div>
            <h3 id="mmwaveAssistantTitle">mmWave-Kalibrierung</h3>
            <p>Radar labelt Kalibrierung und Blindtest. Live nutzt nur CSI.</p>
          </div>
          <div class="mmwave-state is-loading" id="mmwaveState" role="status" aria-live="polite">
            PRÜFE LINK
          </div>
        </div>
        <ol class="mmwave-steps" id="mmwaveSteps" aria-label="Kalibrierungsfortschritt"></ol>
        <div class="mmwave-assistant-grid">
          <div class="mmwave-guidance" id="mmwaveGuidance">
            <div class="mmwave-skeleton mmwave-skeleton-wide"></div>
            <div class="mmwave-skeleton"></div>
          </div>
          <div class="mmwave-zone-panel">
            <div class="mmwave-zone-heading">
              <span>SEGMENTE</span>
              <span id="mmwaveCoverage">0 Zellen</span>
            </div>
            <div class="mmwave-zone-grid" id="mmwaveZones"></div>
          </div>
        </div>
        <div class="mmwave-inline-error" id="mmwaveError" hidden></div>
        <details class="mmwave-transform-panel">
          <summary>Radar ausrichten</summary>
          <form id="mmwaveTransformForm" class="mmwave-transform-form">
            <label>Ursprung X in mm<input name="origin_x_mm" type="number" required disabled></label>
            <label>Ursprung Z in mm<input name="origin_z_mm" type="number" required disabled></label>
            <label>Drehung in mdeg<input name="yaw_mdeg" type="number" min="-360000" max="360000" required disabled></label>
            <label class="mmwave-checkbox"><input name="raw_x_inverted" type="checkbox" disabled> Sensor-X spiegeln</label>
            <button type="submit" class="mmwave-secondary-button" disabled>Ausrichtung speichern</button>
          </form>
          <p class="mmwave-helper">READ-ONLY · Sensorprüfung fehlt.</p>
        </details>
      </section>
    `;
  }

  async refresh() {
    if (this.busy || this.refreshInFlight) return;
    this.refreshInFlight = true;
    try {
      const response = await fetch(MM_WAVE_STATUS_ENDPOINT, { cache: 'no-store' });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error(payload.error || `Statusabfrage: HTTP ${response.status}`);
      }
      this.status = payload;
      const session = this.status?.session;
      if (session?.lifecycle && session.lifecycle !== 'active') {
        this.calibrationPlan = null;
      } else if (session?.phase && session.phase !== 'empty_calibration') {
        this.calibrationPlan = null;
      } else if (!this.status?.session && ['starting', 'collecting'].includes(this.calibrationPlan?.phase)) {
        this.calibrationPlan = null;
      }
      this._setStatusError('');
    } catch (error) {
      this._setStatusError(error.message || 'mmWave-Status ist nicht erreichbar.');
    } finally {
      this.refreshInFlight = false;
      this._render();
    }
  }

  async _onClick(event) {
    const action = event.target.closest('[data-mmwave-action]')?.dataset.mmwaveAction;
    if (!action || this.busy) return;
    if (action === 'refresh') {
      await this.refresh();
      return;
    }
    if (action === 'prepare-calibration') {
      this._prepareCalibration();
      return;
    }
    if (action === 'cancel-calibration-preparation') {
      this._cancelCalibrationPreparation();
      return;
    }
    const requests = {
      'start-blind': [MM_WAVE_SESSION_START_ENDPOINT, { kind: 'blind' }],
      stop: [MM_WAVE_SESSION_STOP_ENDPOINT, {}],
    };
    if (!requests[action]) return;
    this.busy = true;
    this._clearActionError();
    this._render();
    try {
      const [url, body] = requests[action];
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(payload.error || `Aktion fehlgeschlagen: HTTP ${response.status}`);
      this.status = payload;
    } catch (error) {
      this._setActionError(error.message || 'mmWave-Aktion fehlgeschlagen.');
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _onSubmit(event) {
    if (event.target.id === 'mmwaveCalibrationPrepareForm') {
      event.preventDefault();
      this._scheduleCalibration(event.target);
      return;
    }
    if (event.target.id !== 'mmwaveTransformForm') return;
    event.preventDefault();
    this._setActionError('READ-ONLY: mmWave-Aktionen sind bis zur physischen Sensorprüfung gesperrt.');
    this._render();
  }

  _prepareCalibration() {
    this._clearCalibrationTimer();
    this.calibrationPlan = {
      phase: 'form',
      durationSeconds: EMPTY_REFERENCE_DEFAULT_SECONDS,
      leadSeconds: CALIBRATION_DEFAULT_LEAD_SECONDS,
    };
    this._clearActionError();
    this._render();
  }

  _cancelCalibrationPreparation() {
    if (!['form', 'countdown'].includes(this.calibrationPlan?.phase)) return;
    this._clearCalibrationTimer();
    this.calibrationPlan = null;
    this._clearActionError();
    this._render();
  }

  _scheduleCalibration(form) {
    const read = (name) => Number(form.querySelector(`[name="${name}"]`)?.value);
    const durationSeconds = read('duration_seconds');
    const leadSeconds = read('lead_seconds');
    if (!Number.isInteger(durationSeconds) || durationSeconds < EMPTY_REFERENCE_MIN_SECONDS) {
      this._setActionError(`Die Leerdauer muss eine ganze Zahl von mindestens ${EMPTY_REFERENCE_MIN_SECONDS} Sekunden sein.`);
      this._render();
      return;
    }
    if (!Number.isInteger(leadSeconds) || leadSeconds < CALIBRATION_MIN_LEAD_SECONDS) {
      this._setActionError(`Der Vorlauf muss eine ganze Zahl von mindestens ${CALIBRATION_MIN_LEAD_SECONDS} Sekunden sein.`);
      this._render();
      return;
    }

    this._clearActionError();
    this.calibrationPlan = {
      phase: 'countdown',
      durationSeconds,
      leadSeconds,
      startsAtMs: Date.now() + leadSeconds * 1000,
      displaySeconds: leadSeconds,
    };
    this._startCalibrationTimer();
    this._render();
  }

  _clearCalibrationTimer() {
    if (this.calibrationTimer !== null) window.clearInterval(this.calibrationTimer);
    this.calibrationTimer = null;
  }

  _startCalibrationTimer() {
    this._clearCalibrationTimer();
    this.calibrationTimer = window.setInterval(
      () => this._tickCalibrationPreparation(),
      CALIBRATION_TIMER_MS,
    );
  }

  _tickCalibrationPreparation() {
    const plan = this.calibrationPlan;
    if (plan?.phase !== 'countdown' || this.busy) return;
    if (Date.now() >= plan.startsAtMs) {
      this._clearCalibrationTimer();
      void this._startCalibration(plan);
      return;
    }
    const remaining = this._calibrationRemainingSeconds();
    if (plan.displaySeconds !== remaining) {
      this.calibrationPlan = { ...plan, displaySeconds: remaining };
      this._render();
    }
  }

  _calibrationRemainingSeconds() {
    if (this.calibrationPlan?.phase === 'countdown') {
      return Math.max(0, Math.ceil((this.calibrationPlan.startsAtMs - Date.now()) / 1000));
    }
    const remaining = Number(this.status?.session?.empty_remaining_seconds);
    return Number.isFinite(remaining) ? Math.max(0, Math.ceil(remaining)) : 0;
  }

  _calibrationSafeReturnAt() {
    const plan = this.calibrationPlan;
    if (plan?.phase === 'countdown') {
      return plan.startsAtMs + plan.durationSeconds * 1000;
    }
    if (this.status?.session?.phase === 'empty_calibration') {
      return Date.now() + this._calibrationRemainingSeconds() * 1000;
    }
    return null;
  }

  _calibrationSafeReturnMarkup() {
    const safeReturnAt = this._calibrationSafeReturnAt();
    return Number.isFinite(safeReturnAt)
      ? `<small class="mmwave-safe-return">Sicher zurück ab ${formatClockTime(safeReturnAt)} Uhr</small>`
      : '';
  }

  _calibrationPreparationMarkup() {
    const plan = this.calibrationPlan;
    if (plan?.phase === 'form') {
      return `
        <div class="mmwave-preparation">
          <div class="mmwave-eyebrow">VORBEREITUNG</div>
          <h4>Dauer und Vorlauf festlegen</h4>
          <p>Nach dem Countdown startet zuerst die Leermessung. Danach beginnt der geführte Rundgang.</p>
          <form id="mmwaveCalibrationPrepareForm" class="mmwave-preparation-form">
            <label><span>Leerdauer (Sekunden) · mindestens 60</span><input name="duration_seconds" type="number" min="${EMPTY_REFERENCE_MIN_SECONDS}" max="3600" step="1" value="${plan.durationSeconds}" required></label>
            <label><span>Vorlauf bis Start (Sekunden)</span><input name="lead_seconds" type="number" min="${CALIBRATION_MIN_LEAD_SECONDS}" step="1" value="${plan.leadSeconds}" required></label>
            <div class="mmwave-preparation-actions">
              <button type="submit" class="mmwave-primary-button">Countdown starten</button>
              <button type="button" data-mmwave-action="cancel-calibration-preparation" class="mmwave-secondary-button">Abbrechen</button>
            </div>
          </form>
        </div>`;
    }
    if (plan?.phase === 'countdown') {
      return `
        <div class="mmwave-preparation is-countdown">
          <div class="mmwave-eyebrow">RAUM VERLASSEN</div>
          <h4>Kalibrierung startet in ${this._calibrationRemainingSeconds()} s</h4>
          <p>Es werden noch keine Kalibrierungsdaten gesammelt.</p>
          ${this._calibrationSafeReturnMarkup()}
          <button type="button" data-mmwave-action="cancel-calibration-preparation" class="mmwave-secondary-button">Countdown abbrechen</button>
        </div>`;
    }
    return `
      <div class="mmwave-preparation is-countdown">
        <div class="mmwave-eyebrow">START</div>
        <h4>Kalibrierung wird gestartet …</h4>
        <p>Bitte außerhalb des Raums bleiben.</p>
        ${this._calibrationSafeReturnMarkup()}
      </div>`;
  }

  async _startCalibration(plan) {
    this.busy = true;
    this._clearActionError();
    this.calibrationPlan = { ...plan, phase: 'starting', displaySeconds: 0 };
    this._render();
    try {
      const calibrationContext = this.calibrationContextProvider();
      if (!calibrationContext?.profile_id) {
        throw new Error('Im Control Center muss ein unveränderliches Setup-Profil ausgewählt sein.');
      }
      const response = await fetch(MM_WAVE_SESSION_START_ENDPOINT, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          kind: 'calibration',
          calibration_context: calibrationContext,
          policy: {
            zone_count: 9,
            empty_calibration_seconds: plan.durationSeconds,
          },
        }),
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(payload.error || `Aktion fehlgeschlagen: HTTP ${response.status}`);
      this.status = payload;
      this.calibrationPlan = { ...plan, phase: 'collecting' };
    } catch (error) {
      this._setActionError(error.message || 'mmWave-Kalibrierung konnte nicht gestartet werden.');
      this.calibrationPlan = { ...plan, phase: 'form' };
    } finally {
      this.busy = false;
      this._render();
    }
  }

  _render() {
    const model = mmwaveAssistantViewModel(this.status);
    const state = this.container.querySelector('#mmwaveState');
    const steps = this.container.querySelector('#mmwaveSteps');
    const guidance = this.container.querySelector('#mmwaveGuidance');
    const zones = this.container.querySelector('#mmwaveZones');
    const coverage = this.container.querySelector('#mmwaveCoverage');
    const error = this.container.querySelector('#mmwaveError');
    if (!state || !steps || !guidance || !zones || !coverage || !error) return;

    state.textContent = this.busy ? 'AKTION LÄUFT' : stateLabel(this.status?.state);
    state.className = `mmwave-state is-${this.busy ? 'loading' : (this.status?.state || 'disconnected')}`;
    steps.innerHTML = STEP_LABELS.map((label, index) => `
      <li class="${index < model.activeStep ? 'is-done' : ''} ${index === model.activeStep ? 'is-current' : ''}">
        <span>${index + 1}</span><strong>${label}</strong>
      </li>
    `).join('');

    coverage.textContent = `${Number(this.status?.coverage_cells || 0)} Zellen`;
    zones.innerHTML = model.zones.length > 0
      ? model.zones.map((zone, index) => `
          <div class="mmwave-zone ${zone.training_blocks >= 6 ? 'is-trained' : ''} ${zone.id === this.status?.recommended_zone_id ? 'is-next' : ''}">
            <strong>Segment ${String(index + 1).padStart(2, '0')}</strong>
            <span>${(zone.center_mm[0] / 1000).toFixed(2)} / ${(zone.center_mm[1] / 1000).toFixed(2)} m</span>
            <small>CSI-Fenster ${zone.training_blocks}/6 · Kontrolle ${zone.blind_visits}/2</small>
          </div>
        `).join('')
      : '<div class="mmwave-zone-empty">Rundgang erzeugt Segmente. Radar bleibt kontinuierlich.</div>';

    guidance.innerHTML = this._guidance(model);
    const visibleError = [this.statusError, this.actionError].filter(Boolean).join(' · ');
    error.hidden = !visibleError;
    error.textContent = visibleError;
    this._fillTransformForm();
  }

  _guidance(model) {
    const status = this.status;
    if (!status) {
      return `
        <h4>Server fehlt</h4>
        <p>Sensing-Server starten, dann erneut prüfen.</p>
        <button data-mmwave-action="refresh" class="mmwave-primary-button">Prüfen</button>
      `;
    }
    if (model.session) {
      const lifecycle = model.session.lifecycle || 'active';
      if (lifecycle === 'interrupted') {
        return `
          <h4>Sitzung unterbrochen</h4>
          <p>Der Server-Neustart wurde erkannt. Die Messung wurde nicht automatisch fortgesetzt.</p>
          ${model.session.error ? `<p>${escapeHTML(model.session.error)}</p>` : ''}
          ${this._emptyCalibrationValidityMarkup(model.session.empty_validity)}
          ${this._transportFacts(status)}
        `;
      }
      if (lifecycle === 'error') {
        return `
          <h4>Sitzung mit Fehler beendet</h4>
          <p>${escapeHTML(model.session.error || 'Die Aufzeichnung konnte nicht sicher fortgesetzt werden.')}</p>
          ${this._emptyCalibrationValidityMarkup(model.session.empty_validity)}
          ${this._transportFacts(status)}
        `;
      }
      const diagnostic = mmwaveTransportDiagnostic(status);
      if (status.state === 'stale' || diagnostic.state === 'radar_interrupted') {
        const heading = diagnostic.state === 'unavailable'
          ? diagnostic.message
          : 'Radar verbunden, aber Datenstrom unterbrochen.';
        return `
          <div class="mmwave-live-line"><span></span>${escapeHTML(model.session.kind)} · ${escapeHTML(model.session.phase)}</div>
          <h4>${escapeHTML(heading)}</h4>
          <p>Die Sitzung läuft bis zum konfigurierten Ende weiter. Der aktuelle Zustand wird vollständig protokolliert und danach bewertet.</p>
          <p>${escapeHTML(status.reason || diagnostic.message)}</p>
          ${this._emptyCalibrationValidityMarkup(model.session.empty_validity)}
          ${this._transportFacts(status)}
          <button data-mmwave-action="stop" class="mmwave-secondary-button" ${this.busy ? 'disabled' : ''}>Stoppen</button>
        `;
      }
    }
    if (!model.connected) {
      return `
        <h4>Warte auf Radar</h4>
        <p>${escapeHTML(status.reason)}</p>
        ${this._transportFacts(status)}
      `;
    }
    if (status.state === 'multi_target') {
      return '<h4>Nur ein Ziel</h4><p>Mehrere Ziele werden nicht gelabelt.</p>';
    }
    if (model.session) {
      const recommendedZone = model.zones.find((zone) => zone.id === status.recommended_zone_id);
      const recommendedInstruction = recommendedZone
        ? `Gehe zum noch dünn erfassten Bereich bei ${(recommendedZone.center_mm[0] / 1000).toFixed(2)} / ${(recommendedZone.center_mm[1] / 1000).toFixed(2)} m und bleibe fünf Sekunden ruhig.`
        : null;
      const emptyRemaining = Number(model.session.empty_remaining_seconds);
      const instruction = ({
        empty_calibration: Number.isFinite(emptyRemaining)
          ? `Raum noch ${Math.max(0, Math.ceil(emptyRemaining))} s leer lassen.`
          : 'Raum für die Leermessung leer lassen.',
        coverage: 'Alle erreichbaren Bereiche abgehen.',
        training: recommendedInstruction || 'Bereiche abgehen, dünne Stellen kurz halten.',
        blind: recommendedInstruction || 'Alle Bereiche erneut besuchen.',
        complete: 'Fertig. Sitzung beenden.',
      })[model.session.phase] || model.session.next_instruction;
      const radarPosition = Array.isArray(status.target_position_mm)
        ? `${(status.target_position_mm[0] / 1000).toFixed(2)} / ${(status.target_position_mm[1] / 1000).toFixed(2)} m`
        : '--';
      return `
        <div class="mmwave-live-line"><span></span>${escapeHTML(model.session.kind)} · ${escapeHTML(model.session.phase)}</div>
        <h4>${escapeHTML(instruction)}</h4>
        <p>${escapeHTML(status.mode)} · Radar ${radarPosition} · Samples ${model.session.aligned_samples}${status.state === 'invalid' ? ` · ${escapeHTML(status.reason)}` : ''}</p>
        ${this._emptyCalibrationValidityMarkup(model.session.empty_validity)}
        ${model.session.phase === 'empty_calibration' ? this._calibrationSafeReturnMarkup() : ''}
        ${this._transportFacts(status)}
        <button data-mmwave-action="stop" class="mmwave-secondary-button" ${this.busy ? 'disabled' : ''}>Stoppen</button>
      `;
    }
    if (model.blindComplete) {
      const verdict = status.blind_verdict || 'WIRD AUSGEWERTET';
      const activation = status.position_live_approved
        ? 'Der Positionsindex ist für die Live-Anzeige freigegeben.'
        : 'Der Positionsindex bleibt für die Live-Anzeige gesperrt.';
      return `<h4>Blindtest ${escapeHTML(verdict)}</h4><p>Prediction vor Truth versiegelt. ${activation}</p><p>SHA-256: ${escapeHTML(status.blind_report_sha256 || '--')}</p>`;
    }
    if (model.trainingComplete) {
      return `
        <h4>Kalibrierung fertig</h4>
        <p>Modell eingefroren. Blindtest sammelt neue Besuche.</p>
        <button data-mmwave-action="start-blind" class="mmwave-primary-button" ${status?.preflight?.ready && !this.busy ? '' : 'disabled'}>Blindtest</button>
      `;
    }
    if (this.calibrationPlan) {
      return this._calibrationPreparationMarkup();
    }
    return `
      <h4>Rundgang</h4>
      <p>Bereiche abgehen. CSI wird mit Radar-X/Z verknüpft; P01–P09 entfällt.</p>
      <dl class="mmwave-facts"><div><dt>Node</dt><dd>${escapeHTML(status.node_id || '--')}</dd></div><div><dt>Modus</dt><dd>${escapeHTML(status.mode || '--')}</dd></div><div><dt>Radar X/Z</dt><dd>${Array.isArray(status.target_position_mm) ? `${(status.target_position_mm[0] / 1000).toFixed(2)} / ${(status.target_position_mm[1] / 1000).toFixed(2)} m` : '--'}</dd></div><div><dt>Alter</dt><dd>${status.packet_age_ms ?? '--'} ms</dd></div></dl>
      ${this._transportFacts(status)}
      <button data-mmwave-action="prepare-calibration" class="mmwave-primary-button" ${status?.preflight?.ready && !this.busy ? '' : 'disabled'}>Kalibrierung vorbereiten</button>
      ${this._startRequirement(status)}
      <p class="mmwave-helper">Start erst nach 25-s-Preflight. SOFTWARE-ONLY / UNVALIDATED bis Blindtest.</p>
    `;
  }

  _emptyCalibrationValidityMarkup(validity) {
    if (!validity || !['valid', 'invalid'].includes(validity.verdict)) return '';
    const valid = validity.verdict === 'valid';
    const reasons = Array.isArray(validity.reasons) ? validity.reasons : [];
    const reasonMarkup = reasons.length > 0
      ? `<ul>${reasons.map((reason) => `<li>${escapeHTML(reason)}</li>`).join('')}</ul>`
      : '<p>Keine Gültigkeitsverletzung aufgezeichnet.</p>';
    const outside = Number(validity.outside_room_targets) || 0;
    const csiFrames = Number(validity.csi_frames) || 0;
    return `
      <section class="mmwave-validity ${valid ? 'is-valid' : 'is-invalid'}" aria-label="Leermessungs-Ergebnis">
        <strong>Leermessung: ${valid ? 'GÜLTIG' : 'UNGÜLTIG'}</strong>
        <p>${csiFrames.toLocaleString('de-DE')} CSI-Frames · ${outside.toLocaleString('de-DE')} Außenraum-Ziele ignoriert</p>
        ${reasonMarkup}
      </section>`;
  }

  _transportFacts(status) {
    const diagnostic = mmwaveTransportDiagnostic(status);
    const counter = (value) => Number.isFinite(Number(value)) ? Number(value).toLocaleString('de-DE') : '--';
    const duration = (value) => Number.isFinite(Number(value)) ? `${counter(value)} ms` : '--';
    const nodeControl = status.node_control || {};
    let nodeStatus = 'noch nicht geprüft';
    if (nodeControl.reachable === true) {
      nodeStatus = `erreichbar · letzter Status vor ${duration(nodeControl.last_success_age_ms)}`;
    } else if (nodeControl.reachable === false) {
      nodeStatus = `${nodeControl.last_error_kind || 'nicht erreichbar'}${nodeControl.last_error ? ` · ${nodeControl.last_error}` : ''}`;
    } else if (!nodeControl.url_configured || !nodeControl.token_configured) {
      nodeStatus = 'Konfiguration unvollständig';
    }
    const rejectReasons = Object.entries(status.reject_reasons || {})
      .filter(([, value]) => Number(value) > 0)
      .map(([category, value]) => `${escapeHTML(category)} ${counter(value)}`)
      .join(' · ') || '--';
    const lastRejection = status.last_rejection
      ? `${status.last_rejection.category}: ${status.last_rejection.reason} (${duration(status.last_rejection.age_ms)} alt)`
      : '--';
    const lastGap = status.last_sequence_gap
      ? `${status.last_sequence_gap.expected_sequence} → ${status.last_sequence_gap.received_sequence} (${counter(status.last_sequence_gap.missing_packets)} fehlen, ${duration(status.last_sequence_gap.age_ms)} alt)`
      : '--';
    return `
      <p class="mmwave-helper" data-transport-state="${diagnostic.state}">${escapeHTML(diagnostic.message)}</p>
      <dl class="mmwave-facts">
        <div><dt>UDP-Port</dt><dd>${escapeHTML(status.udp_port ?? '--')}</dd></div>
        <div><dt>ESP-Status</dt><dd>${escapeHTML(nodeStatus)}</dd></div>
        <div><dt>UDP roh</dt><dd>${counter(status.raw_udp_packets)}</dd></div>
        <div><dt>UART</dt><dd>${counter(status.uart_bytes_received)}</dd></div>
        <div><dt>Radarframes</dt><dd>${counter(status.radar_frames_valid)}</dd></div>
        <div><dt>UDP ESP</dt><dd>${counter(status.udp_packets_sent)}</dd></div>
        <div><dt>UDP Server</dt><dd>${counter(status.packets_received)}</dd></div>
        <div><dt>UDP-Fehler gesamt</dt><dd>${counter(status.udp_send_failures)}</dd></div>
        <div><dt>UDP-Fehler zuletzt</dt><dd>${counter(status.udp_send_failures_window)}</dd></div>
        <div><dt>Verworfen</dt><dd>${counter(status.packets_rejected)}</dd></div>
        <div><dt>Queue / Peak</dt><dd>${counter(status.transport?.queue_length)} / ${counter(status.transport?.queue_peak)}</dd></div>
        <div><dt>Verarbeitung</dt><dd>${duration(status.transport?.last_receive_to_process_delay_ms)}</dd></div>
      </dl>
      <p class="mmwave-helper">Verworfen nach Grund: ${rejectReasons}</p>
      <p class="mmwave-helper">Letzte Ablehnung: ${escapeHTML(lastRejection)}</p>
      <p class="mmwave-helper">Letzte Sequenzlücke: ${escapeHTML(lastGap)}</p>
    `;
  }

  _startRequirement(status) {
    const nodeControl = status.node_control;
    if (!status.configured || nodeControl && (!nodeControl.url_configured || !nodeControl.token_configured)) {
      return '<p class="mmwave-helper">Node-URL oder Token fehlen.</p>';
    }
    if (!status.setup_sealed) {
      return '<p class="mmwave-helper">Radar ausrichten, Setup-v2 versiegeln, Server neu starten.</p>';
    }
    const gates = Array.isArray(status?.preflight?.gates) ? status.preflight.gates : [];
    if (!status?.preflight?.ready) {
      const blockers = gates.filter((gate) => !gate.pass);
      if (blockers.length === 0) {
        return '<p class="mmwave-helper">Preflight ist noch nicht bereit; der Server liefert dafür keine einzelnen Gate-Details. Status aktualisieren.</p>';
      }
      return `<div class="mmwave-helper"><strong>Preflight:</strong><ul>${blockers.map((gate) => `<li>${escapeHTML(preflightGateLabel(gate.id))} – ${escapeHTML(gate.detail)}</li>`).join('')}</ul></div>`;
    }
    return '<p class="mmwave-helper">25-s-Preflight bestanden. Setup, Radar und RX1–RX4 bereit.</p>';
  }

  _fillTransformForm() {
    const transform = this.status?.transform;
    const form = this.container.querySelector('#mmwaveTransformForm');
    if (!transform || !form || form.dataset.filled === 'true') return;
    form.elements.origin_x_mm.value = transform.origin_x_mm;
    form.elements.origin_z_mm.value = transform.origin_z_mm;
    form.elements.yaw_mdeg.value = transform.yaw_mdeg;
    form.elements.raw_x_inverted.checked = transform.raw_x_inverted;
    form.dataset.filled = 'true';
  }
}
