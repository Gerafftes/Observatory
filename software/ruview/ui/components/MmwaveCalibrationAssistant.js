const MM_WAVE_STATUS_ENDPOINT = '/api/v1/mmwave/status';
const MM_WAVE_SESSION_START_ENDPOINT = '/api/v1/mmwave/session/start';
const MM_WAVE_SESSION_STOP_ENDPOINT = '/api/v1/mmwave/session/stop';
const STEP_LABELS = ['Link', 'Ausrichtung', 'Fläche', 'Segmente', 'CSI', 'Blindtest', 'Ergebnis'];

export function mmwaveAssistantViewModel(status) {
  const session = status?.session || null;
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
    trainingComplete,
    zoneCount,
    zones,
  };
}

export function mmwaveTransportDiagnostic(status) {
  if (status?.node_status_error) {
    return { state: 'unavailable', message: 'ESP-Status fehlt.' };
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

export class MmwaveCalibrationAssistant {
  constructor(container) {
    this.container = container;
    this.timer = null;
    this.busy = false;
    this.status = null;
    this.error = '';
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
    if (this.busy) return;
    try {
      const response = await fetch(MM_WAVE_STATUS_ENDPOINT, { cache: 'no-store' });
      if (!response.ok) throw new Error(`Statusabfrage: HTTP ${response.status}`);
      this.status = await response.json();
      this.error = '';
    } catch (error) {
      this.error = error.message || 'mmWave-Status ist nicht erreichbar.';
    }
    this._render();
  }

  async _onClick(event) {
    const action = event.target.closest('[data-mmwave-action]')?.dataset.mmwaveAction;
    if (!action || this.busy) return;
    if (action === 'refresh') {
      await this.refresh();
      return;
    }
    const requests = {
      'start-calibration': [MM_WAVE_SESSION_START_ENDPOINT, { kind: 'calibration', policy: { zone_count: 9 } }],
      'start-blind': [MM_WAVE_SESSION_START_ENDPOINT, { kind: 'blind' }],
      stop: [MM_WAVE_SESSION_STOP_ENDPOINT, {}],
    };
    if (!requests[action]) return;
    this.busy = true;
    this.error = '';
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
      this.error = error.message || 'mmWave-Aktion fehlgeschlagen.';
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _onSubmit(event) {
    if (event.target.id !== 'mmwaveTransformForm') return;
    event.preventDefault();
    this.error = 'READ-ONLY: mmWave-Aktionen sind bis zur physischen Sensorprüfung gesperrt.';
    this._render();
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
    error.hidden = !this.error;
    error.textContent = this.error;
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
      const instruction = ({
        empty_calibration: 'Raum 65 s leer lassen.',
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
        <p>${escapeHTML(status.mode)} · Radar ${radarPosition} · Samples ${model.session.aligned_samples}</p>
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
    return `
      <h4>Rundgang</h4>
      <p>Bereiche abgehen. CSI wird mit Radar-X/Z verknüpft; P01–P09 entfällt.</p>
      <dl class="mmwave-facts"><div><dt>Node</dt><dd>${escapeHTML(status.node_id || '--')}</dd></div><div><dt>Modus</dt><dd>${escapeHTML(status.mode || '--')}</dd></div><div><dt>Radar X/Z</dt><dd>${Array.isArray(status.target_position_mm) ? `${(status.target_position_mm[0] / 1000).toFixed(2)} / ${(status.target_position_mm[1] / 1000).toFixed(2)} m` : '--'}</dd></div><div><dt>Alter</dt><dd>${status.packet_age_ms ?? '--'} ms</dd></div></dl>
      ${this._transportFacts(status)}
      <button data-mmwave-action="start-calibration" class="mmwave-primary-button" ${status?.preflight?.ready && !this.busy ? '' : 'disabled'}>Kalibrierung</button>
      ${this._startRequirement(status)}
      <p class="mmwave-helper">Start erst nach 25-s-Preflight. SOFTWARE-ONLY / UNVALIDATED bis Blindtest.</p>
    `;
  }

  _transportFacts(status) {
    const diagnostic = mmwaveTransportDiagnostic(status);
    const counter = (value) => Number.isFinite(Number(value)) ? Number(value).toLocaleString('de-DE') : '--';
    return `
      <p class="mmwave-helper" data-transport-state="${diagnostic.state}">${escapeHTML(diagnostic.message)}</p>
      <dl class="mmwave-facts">
        <div><dt>UDP-Port</dt><dd>${escapeHTML(status.udp_port ?? '--')}</dd></div>
        <div><dt>UART</dt><dd>${counter(status.uart_bytes_received)}</dd></div>
        <div><dt>Radarframes</dt><dd>${counter(status.radar_frames_valid)}</dd></div>
        <div><dt>UDP ESP</dt><dd>${counter(status.udp_packets_sent)}</dd></div>
        <div><dt>UDP Server</dt><dd>${counter(status.packets_received)}</dd></div>
        <div><dt>UDP-Fehler</dt><dd>${counter(status.udp_send_failures)}</dd></div>
        <div><dt>Verworfen</dt><dd>${counter(status.packets_rejected)}</dd></div>
      </dl>
    `;
  }

  _startRequirement(status) {
    if (!status.configured) {
      return '<p class="mmwave-helper">Node-URL und Token fehlen.</p>';
    }
    if (!status.setup_sealed) {
      return '<p class="mmwave-helper">Radar ausrichten, Setup-v2 versiegeln, Server neu starten.</p>';
    }
    const gates = Array.isArray(status?.preflight?.gates) ? status.preflight.gates : [];
    if (!status?.preflight?.ready) {
      const blockers = gates.filter((gate) => !gate.pass);
      return `<div class="mmwave-helper"><strong>Preflight:</strong><ul>${blockers.map((gate) => `<li>${escapeHTML(gate.id)} – ${escapeHTML(gate.detail)}</li>`).join('')}</ul></div>`;
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
