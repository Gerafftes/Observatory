const MM_WAVE_STATUS_ENDPOINT = '/api/v1/mmwave/status';
const MM_WAVE_SESSION_START_ENDPOINT = '/api/v1/mmwave/session/start';
const MM_WAVE_SESSION_STOP_ENDPOINT = '/api/v1/mmwave/session/stop';
const STEP_LABELS = [
  'Verbindung',
  'Ausrichtung',
  'Abdeckung',
  'Segmente',
  'CSI-Kopplung',
  'Blindtest',
  'Ergebnis',
];

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
    return { state: 'unavailable', message: 'ESP-Status nicht erreichbar.' };
  }
  if ([status?.uart_bytes_received, status?.radar_frames_valid, status?.udp_packets_sent]
    .some((value) => value === null || value === undefined)) {
    return { state: 'unavailable', message: 'ESP-Diagnose noch nicht verfügbar.' };
  }
  const uartBytes = Number(status.uart_bytes_received);
  const validFrames = Number(status.radar_frames_valid);
  const udpSent = Number(status.udp_packets_sent);
  if (![uartBytes, validFrames, udpSent].every(Number.isFinite)) {
    return { state: 'unavailable', message: 'ESP-Diagnose noch nicht verfügbar.' };
  }
  if (uartBytes === 0) {
    return { state: 'uart_idle', message: 'Keine UART-Bytes: Versorgung, TX→RX, GPIO20 und Baudrate prüfen.' };
  }
  if (validFrames === 0) {
    return { state: 'invalid_frames', message: 'UART empfängt Bytes, aber kein gültiges LD2450-Frame: Leitung oder Baudrate prüfen.' };
  }
  if (udpSent === 0) {
    return { state: 'udp_blocked', message: 'Radarframes sind gültig, aber der ESP hat noch kein UDP-Paket gesendet.' };
  }
  return { state: 'streaming', message: 'UART, Radarparser und UDP-Sender liefern Daten.' };
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
    disconnected: 'NICHT VERBUNDEN',
    stale: 'VERALTET',
    no_target: 'KEIN ZIEL',
    multi_target: 'MEHRERE ZIELE',
    invalid: 'UNGÜLTIG',
    valid: 'BEREIT',
  })[state] || 'WIRD GEPRÜFT';
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
            <div class="mmwave-eyebrow">KALIBRIERUNGSREFERENZ</div>
            <h3 id="mmwaveAssistantTitle">mmWave-geführte Positionsaufnahme</h3>
            <p>Der Radar führt nur durch Kalibrierung und Blindtest. Die Live-Position bleibt eine reine WLAN-CSI-Vorhersage.</p>
          </div>
          <div class="mmwave-state is-loading" id="mmwaveState" role="status" aria-live="polite">
            VERBINDUNG WIRD GEPRÜFT
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
              <span>ABDECKUNGSSEGMENTE</span>
              <span id="mmwaveCoverage">0 Rasterzellen</span>
            </div>
            <div class="mmwave-zone-grid" id="mmwaveZones"></div>
          </div>
        </div>
        <div class="mmwave-inline-error" id="mmwaveError" hidden></div>
        <details class="mmwave-transform-panel">
          <summary>Radar im Raum ausrichten</summary>
          <form id="mmwaveTransformForm" class="mmwave-transform-form">
            <label>Ursprung X in mm<input name="origin_x_mm" type="number" required disabled></label>
            <label>Ursprung Z in mm<input name="origin_z_mm" type="number" required disabled></label>
            <label>Drehung in mdeg<input name="yaw_mdeg" type="number" min="-360000" max="360000" required disabled></label>
            <label class="mmwave-checkbox"><input name="raw_x_inverted" type="checkbox" disabled> Sensor-X spiegeln</label>
            <button type="submit" class="mmwave-secondary-button" disabled>Ausrichtung speichern</button>
          </form>
          <p class="mmwave-helper">READ-ONLY bis der Sensor physisch angekommen und geprüft ist. Diese Ansicht ändert weder Transform noch Sitzung.</p>
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

    coverage.textContent = `${Number(this.status?.coverage_cells || 0)} Rasterzellen`;
    zones.innerHTML = model.zones.length > 0
      ? model.zones.map((zone, index) => `
          <div class="mmwave-zone ${zone.training_blocks >= 6 ? 'is-trained' : ''} ${zone.id === this.status?.recommended_zone_id ? 'is-next' : ''}">
            <strong>Segment ${String(index + 1).padStart(2, '0')}</strong>
            <span>${(zone.center_mm[0] / 1000).toFixed(2)} / ${(zone.center_mm[1] / 1000).toFixed(2)} m</span>
            <small>CSI-Fenster ${zone.training_blocks}/6 · Kontrolle ${zone.blind_visits}/2</small>
          </div>
        `).join('')
      : '<div class="mmwave-zone-empty">Beim Rundgang wird die gemessene Fläche in Auswertungssegmente unterteilt. Die Radarposition selbst bleibt kontinuierlich.</div>';

    guidance.innerHTML = this._guidance(model);
    error.hidden = !this.error;
    error.textContent = this.error;
    this._fillTransformForm();
  }

  _guidance(model) {
    const status = this.status;
    if (!status) {
      return `
        <h4>Server nicht erreichbar</h4>
        <p>Starte den Sensing-Server und prüfe danach die Verbindung erneut.</p>
        <button data-mmwave-action="refresh" class="mmwave-primary-button">Erneut prüfen</button>
      `;
    }
    if (!model.connected) {
      return `
        <h4>Auf das erste gültige Radar-Paket warten</h4>
        <p>${escapeHTML(status.reason)}</p>
        ${this._transportFacts(status)}
      `;
    }
    if (status.state === 'multi_target') {
      return '<h4>Nur eine Person darf im Raum sein</h4><p>Mehrere Radarziele werden absichtlich nicht als Trainingslabel verwendet.</p>';
    }
    if (model.session) {
      const recommendedZone = model.zones.find((zone) => zone.id === status.recommended_zone_id);
      const recommendedInstruction = recommendedZone
        ? `Gehe zum noch dünn erfassten Bereich bei ${(recommendedZone.center_mm[0] / 1000).toFixed(2)} / ${(recommendedZone.center_mm[1] / 1000).toFixed(2)} m und bleibe fünf Sekunden ruhig.`
        : null;
      const instruction = ({
        empty_calibration: 'Raum jetzt 65 Sekunden vollständig leer lassen.',
        coverage: 'Gehe durch alle erreichbaren Bereiche des Raums.',
        training: recommendedInstruction || 'Bewege dich durch die angezeigten Bereiche und bleibe an dünn erfassten Stellen kurz ruhig.',
        blind: recommendedInstruction || 'Besuche alle Bereiche erneut; die WLAN-Vorhersage ist eingefroren.',
        complete: 'Aufnahme vollständig. Beende die Sitzung, um alle Dateien zu schließen.',
      })[model.session.phase] || model.session.next_instruction;
      const radarPosition = Array.isArray(status.target_position_mm)
        ? `${(status.target_position_mm[0] / 1000).toFixed(2)} / ${(status.target_position_mm[1] / 1000).toFixed(2)} m`
        : '--';
      return `
        <div class="mmwave-live-line"><span></span>${escapeHTML(model.session.kind)} · ${escapeHTML(model.session.phase)}</div>
        <h4>${escapeHTML(instruction)}</h4>
        <p>Modus ${escapeHTML(status.mode)} · Radarposition ${radarPosition} · ausgerichtete Samples ${model.session.aligned_samples}</p>
        ${this._transportFacts(status)}
        <button data-mmwave-action="stop" class="mmwave-secondary-button" ${this.busy ? 'disabled' : ''}>Aufnahme beenden</button>
      `;
    }
    if (model.blindComplete) {
      const verdict = status.blind_verdict || 'WIRD AUSGEWERTET';
      const activation = status.position_live_approved
        ? 'Der Positionsindex ist für die Live-Anzeige freigegeben.'
        : 'Der Positionsindex bleibt für die Live-Anzeige gesperrt.';
      return `<h4>Blindtest ${escapeHTML(verdict)}</h4><p>Vorhersagen wurden vor der Radarwahrheit versiegelt. ${activation}</p><p>Report SHA-256: ${escapeHTML(status.blind_report_sha256 || '--')}</p>`;
    }
    if (model.trainingComplete) {
      return `
        <h4>Referenzabdeckung vollständig</h4>
        <p>Das Modell ist eingefroren, aber noch nicht live freigegeben. Der Blindtest sammelt neue, getrennte Besuche über die erfasste Fläche.</p>
        <button data-mmwave-action="start-blind" class="mmwave-primary-button" ${status?.preflight?.ready && !this.busy ? '' : 'disabled'}>Blindtest starten</button>
      `;
    }
    return `
      <h4>Freier Rundgang mit kurzen Stopps</h4>
      <p>Gehe durch alle erreichbaren Bereiche. Jedes CSI-Zeitfenster wird mit der zeitgleichen x/z-Radarposition verknüpft; ein manuelles P01–P09-Raster ist nicht nötig.</p>
      <dl class="mmwave-facts"><div><dt>Knoten</dt><dd>${escapeHTML(status.node_id || '--')}</dd></div><div><dt>Modus</dt><dd>${escapeHTML(status.mode || '--')}</dd></div><div><dt>Radarposition X/Z</dt><dd>${Array.isArray(status.target_position_mm) ? `${(status.target_position_mm[0] / 1000).toFixed(2)} / ${(status.target_position_mm[1] / 1000).toFixed(2)} m` : '--'}</dd></div><div><dt>Paketalter</dt><dd>${status.packet_age_ms ?? '--'} ms</dd></div></dl>
      ${this._transportFacts(status)}
      <button data-mmwave-action="start-calibration" class="mmwave-primary-button" ${status?.preflight?.ready && !this.busy ? '' : 'disabled'}>Kalibrierungsrundgang starten</button>
      ${this._startRequirement(status)}
      <p class="mmwave-helper">Start bleibt gesperrt, bis alle 25-s-Preflight-Gates bestanden sind. Neue Datensätze bleiben SOFTWARE-ONLY / UNVALIDATED bis zum separaten Blindtest.</p>
    `;
  }

  _transportFacts(status) {
    const diagnostic = mmwaveTransportDiagnostic(status);
    const counter = (value) => Number.isFinite(Number(value)) ? Number(value).toLocaleString('de-DE') : '--';
    return `
      <p class="mmwave-helper" data-transport-state="${diagnostic.state}">${escapeHTML(diagnostic.message)}</p>
      <dl class="mmwave-facts">
        <div><dt>Server-UDP</dt><dd>Port ${escapeHTML(status.udp_port ?? '--')}</dd></div>
        <div><dt>UART-Bytes</dt><dd>${counter(status.uart_bytes_received)}</dd></div>
        <div><dt>Gültige Radarframes</dt><dd>${counter(status.radar_frames_valid)}</dd></div>
        <div><dt>UDP vom ESP</dt><dd>${counter(status.udp_packets_sent)}</dd></div>
        <div><dt>UDP am Server</dt><dd>${counter(status.packets_received)}</dd></div>
        <div><dt>UDP-Sendefehler</dt><dd>${counter(status.udp_send_failures)}</dd></div>
        <div><dt>Pakete verworfen</dt><dd>${counter(status.packets_rejected)}</dd></div>
      </dl>
    `;
  }

  _startRequirement(status) {
    if (!status.configured) {
      return '<p class="mmwave-helper">Node-URL und MMWAVE_NODE_TOKEN müssen beim Serverstart gesetzt sein.</p>';
    }
    if (!status.setup_sealed) {
      return '<p class="mmwave-helper">Richte den Radar zuerst aus, versiegele danach ein Setup-v2 mit Knoten-ID und Transform und starte den Server mit diesem Setup neu.</p>';
    }
    const gates = Array.isArray(status?.preflight?.gates) ? status.preflight.gates : [];
    if (!status?.preflight?.ready) {
      const blockers = gates.filter((gate) => !gate.pass);
      return `<div class="mmwave-helper"><strong>Preflight wartet:</strong><ul>${blockers.map((gate) => `<li>${escapeHTML(gate.id)} – ${escapeHTML(gate.detail)}</li>`).join('')}</ul></div>`;
    }
    return '<p class="mmwave-helper">25-s-Preflight vollständig bestanden. Setup, Radartransport und RX1–RX4 sind bereit.</p>';
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
