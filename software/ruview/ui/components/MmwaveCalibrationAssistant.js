const MM_WAVE_STATUS_ENDPOINT = '/api/v1/mmwave/status';
const MM_WAVE_SESSION_START_ENDPOINT = '/api/v1/mmwave/session/start';
const MM_WAVE_SESSION_STOP_ENDPOINT = '/api/v1/mmwave/session/stop';
const MM_WAVE_KNOWN_POINT_ENDPOINT = '/api/v1/mmwave/known-point/check';
const MM_WAVE_FIXED_POINTS_START_ENDPOINT = '/api/v1/mmwave/fixed-points/start';
const MM_WAVE_FIXED_POINTS_CHECK_ENDPOINT = '/api/v1/mmwave/fixed-points/check';
const MM_WAVE_FIXED_POINTS_CANCEL_ENDPOINT = '/api/v1/mmwave/fixed-points/cancel';
const STEP_LABELS = ['Link', 'Ausrichtung', 'RX/TX', 'CSI-Fläche', 'Segmente', 'Blindtest', 'Ergebnis'];
const EMPTY_REFERENCE_MIN_SECONDS = 60;
const EMPTY_REFERENCE_DEFAULT_SECONDS = 65;
const CALIBRATION_MIN_LEAD_SECONDS = 5;
const CALIBRATION_DEFAULT_LEAD_SECONDS = 5;
const FIXED_POINT_MEASUREMENT_MS = 10_000;
const CALIBRATION_TIMER_MS = 250;
const REPEAT_RESET_SESSION_KEY = 'ruview.mmwave.fixed-point-repeat-reset';
let repeatResetActive = false;

function repeatResetStored() {
  try {
    return globalThis.sessionStorage?.getItem(REPEAT_RESET_SESSION_KEY) === 'true';
  } catch {
    return false;
  }
}

function storeRepeatReset(value) {
  try {
    if (value) globalThis.sessionStorage?.setItem(REPEAT_RESET_SESSION_KEY, 'true');
    else globalThis.sessionStorage?.removeItem(REPEAT_RESET_SESSION_KEY);
  } catch {
    // Private browsing or a disabled storage area must not block calibration.
  }
}

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
  const outsideRoomTarget = status?.last_rejection?.category === 'room_bounds'
    || status?.targets?.some((target) => target?.inside_room === false);
  const connected = Boolean(status)
    && !['disconnected', 'stale'].includes(status.state)
    && (status.state !== 'invalid' || outsideRoomTarget);
  const fixedPointComplete = status?.fixed_point_calibration?.state === 'complete';

  let activeStep = 0;
  if (connected) activeStep = 1;
  if (status?.transform) activeStep = 2;
  if (fixedPointComplete) activeStep = 3;
  if (Number(status?.coverage_cells) > 0) activeStep = 3;
  if (zones.length === zoneCount) activeStep = 4;
  if (trainingComplete) activeStep = 5;
  if (blindComplete) activeStep = 6;

  const phase = session?.phase || null;
  if (phase === 'coverage') activeStep = 2;
  if (phase === 'training') activeStep = 4;
  if (phase === 'blind') activeStep = 5;
  if (phase === 'complete' && session?.kind === 'blind') activeStep = 6;
  if (session?.kind === 'mmwave_only') activeStep = phase === 'complete' ? 2 : 1;

  return {
    activeStep,
    blindComplete,
    connected,
    fixedPointComplete,
    phase,
    session,
    sessionInterrupted: session?.lifecycle === 'interrupted',
    sessionErrored: session?.lifecycle === 'error',
    trainingComplete,
    zoneCount,
    zones,
    fixedPointPreflightReady: status?.fixed_point_preflight?.ready === true,
    radarPreflightReady: status?.radar_preflight?.ready === true,
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
    return { state: 'unavailable', message: status.node_control?.reachable === true
      ? status.node_status_error : 'ESP-Status fehlt.' };
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
  cad_profile_active: 'CAD-Ausrichtung',
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

/**
 * Resolve one sensor from the multi-node status envelope without silently
 * falling back to the primary sensor. A legacy, single-sensor response is
 * still accepted for MMWAVE1, but it must never be displayed as MMWAVE2.
 */
export function selectMmwaveStatus(payload, nodeId = 'MMWAVE1') {
  const envelope = payload && typeof payload === 'object' ? payload : null;
  if (!envelope) return null;
  if (Array.isArray(envelope.sensors)) {
    return envelope.sensors.find((sensor) => (
      sensor && (sensor.configured_node_id || sensor.node_id) === nodeId
    )) || null;
  }
  const envelopeNodeId = envelope.configured_node_id || envelope.node_id;
  if (nodeId !== 'MMWAVE1' && envelopeNodeId !== nodeId) return null;
  return envelope;
}

function statusNodeIds(payload) {
  if (Array.isArray(payload?.sensors)) {
    return payload.sensors
      .map((sensor) => sensor?.configured_node_id || sensor?.node_id)
      .filter((nodeId) => typeof nodeId === 'string' && nodeId.length > 0);
  }
  const nodeId = payload?.configured_node_id || payload?.node_id;
  return nodeId ? [nodeId] : ['MMWAVE1'];
}

export class MmwaveCalibrationAssistant {
  constructor(container, calibrationContextProvider = () => null, nodeId = 'MMWAVE1') {
    this.container = container;
    this.calibrationContextProvider = calibrationContextProvider;
    this.nodeId = nodeId;
    this.timer = null;
    this.busy = false;
    this.refreshInFlight = false;
    this.status = null;
    this.error = '';
    this.actionError = '';
    this.statusError = '';
    this.calibrationPlan = null;
    this.fixedPointResultDismissed = repeatResetActive || repeatResetStored();
    this.calibrationTimer = null;
    this.pointCheck = null;
    this.availableNodeIds = [nodeId === 'MMWAVE2' ? 'MMWAVE2' : 'MMWAVE1'];
  }

  _endpoint(path) {
    return this.nodeId === 'MMWAVE1'
      ? path
      : `${path}?node_id=${encodeURIComponent(this.nodeId)}`;
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
            <div role="group" aria-label="mmWave-Sensor auswählen">
              <button type="button" class="mmwave-secondary-button" data-mmwave-node="MMWAVE1">MMWAVE1</button>
              <button type="button" class="mmwave-secondary-button" data-mmwave-node="MMWAVE2">MMWAVE2</button>
            </div>
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
          <form id="mmwavePointCheckForm" class="mmwave-point-check-form">
            <div class="mmwave-eyebrow">BEKANNTER PUNKT</div>
            <p>Stelle dich auf einen bekannten CAD-Punkt. Die letzten frischen Radarziele werden gegen X/Z geprüft; dabei wird weder RX noch CSI benötigt.</p>
            <div class="mmwave-point-check-fields">
              <label>X (m)<input name="expected_x_m" type="number" min="-100" max="100" step="0.01" required></label>
              <label>Z (m)<input name="expected_z_m" type="number" min="-100" max="100" step="0.01" required></label>
              <label>Toleranz (mm)<input name="tolerance_mm" type="number" min="50" max="2000" step="10" value="350" required></label>
            </div>
            <button type="submit" class="mmwave-secondary-button">Bekannten Punkt prüfen</button>
            <div id="mmwavePointCheckResult" class="mmwave-point-check-result" aria-live="polite"></div>
          </form>
          <div id="mmwaveYawCalibrationResult"></div>
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
      this.availableNodeIds = statusNodeIds(payload);
      const sensorStatus = selectMmwaveStatus(payload, this.nodeId);
      if (!sensorStatus) {
        throw new Error(`${this.nodeId} ist im aktiven Setup nicht konfiguriert.`);
      }
      if (sensorStatus?.fixed_point_calibration?.state === 'active') {
        repeatResetActive = false;
        this.fixedPointResultDismissed = false;
        storeRepeatReset(false);
      }
      this.status = this.fixedPointResultDismissed
        ? { ...sensorStatus, fixed_point_calibration: null, yaw_calibration: null }
        : sensorStatus;
      const session = this.status?.session;
      if (session?.lifecycle && session.lifecycle !== 'active') {
        this.calibrationPlan = null;
      } else if (session?.phase && session.phase !== 'empty_calibration') {
        this.calibrationPlan = null;
      } else if (!this.status?.session && ['phase_two_starting', 'collecting'].includes(this.calibrationPlan?.phase)) {
        this.calibrationPlan = null;
      } else if (!session && !this.calibrationPlan
        && this.status?.fixed_point_calibration?.state === 'complete') {
        this.calibrationPlan = {
          ...this._defaultCalibrationPlan(),
          phase: 'phase_two_ready',
        };
      } else if (!session && this.status?.fixed_point_calibration?.state === 'active'
        && !['anchor_countdown', 'anchor_measuring', 'anchor_failed'].includes(this.calibrationPlan?.phase)) {
        this.calibrationPlan = {
          ...(this.calibrationPlan || this._defaultCalibrationPlan()),
          phase: 'anchor_waiting',
        };
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
    const nodeButton = event.target.closest('[data-mmwave-node]');
    if (nodeButton && !this.busy) {
      this.nodeId = nodeButton.dataset.mmwaveNode;
      this.status = null;
      this.pointCheck = null;
      this.calibrationPlan = null;
      await this.refresh();
      return;
    }
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
      await this._cancelCalibrationPreparation();
      return;
    }
    if (action === 'start-anchor-countdown' || action === 'retry-anchor') {
      this._scheduleCurrentAnchorCountdown();
      return;
    }
    if (action === 'start-phase-two') {
      await this._startCalibration(this.calibrationPlan || this._defaultCalibrationPlan());
      return;
    }
    if (action === 'repeat-yaw-calibration') {
      await this._repeatYawCalibration();
      return;
    }
    const requests = {
      'start-blind': [MM_WAVE_SESSION_START_ENDPOINT, { kind: 'blind' }],
      'start-mmwave-only': [MM_WAVE_SESSION_START_ENDPOINT, {
        kind: 'mmwave_only',
        policy: { empty_calibration_seconds: 60 },
      }],
      stop: [MM_WAVE_SESSION_STOP_ENDPOINT, {}],
    };
    if (!requests[action]) return;
    this.busy = true;
    this._clearActionError();
    this._render();
    try {
      const [url, body] = requests[action];
      const response = await fetch(this._endpoint(url), {
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
      await this._scheduleCalibration(event.target);
      return;
    }
    if (event.target.id === 'mmwavePointCheckForm') {
      event.preventDefault();
      await this._checkKnownPoint(event.target);
      return;
    }
    if (event.target.id !== 'mmwaveTransformForm') return;
    event.preventDefault();
    this._setActionError('READ-ONLY: mmWave-Aktionen sind bis zur physischen Sensorprüfung gesperrt.');
    this._render();
  }

  _prepareCalibration() {
    this._clearCalibrationTimer();
    this.calibrationPlan = this._defaultCalibrationPlan();
    this._clearActionError();
    this._render();
  }

  async _repeatYawCalibration() {
    this._clearCalibrationTimer();
    this._clearActionError();
    repeatResetActive = true;
    this.fixedPointResultDismissed = true;
    storeRepeatReset(true);
    this.status = this.status
      ? { ...this.status, fixed_point_calibration: null, yaw_calibration: null }
      : this.status;
    this.calibrationPlan = null;
    this.busy = true;
    this._render();
    try {
      const response = await fetch(this._endpoint(MM_WAVE_FIXED_POINTS_CANCEL_ENDPOINT), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: '{}',
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error(payload.error || `Zurücksetzen fehlgeschlagen: HTTP ${response.status}`);
      }
      this.status = {
        ...this.status,
        ...payload,
        fixed_point_calibration: null,
        yaw_calibration: null,
      };
    } catch (error) {
      this._setActionError(error.message || 'Die alte Winkelmessung konnte nicht zurückgesetzt werden.');
    } finally {
      this.busy = false;
      this._render();
    }
  }

  _defaultCalibrationPlan() {
    return {
      phase: 'form',
      durationSeconds: EMPTY_REFERENCE_DEFAULT_SECONDS,
      leadSeconds: CALIBRATION_DEFAULT_LEAD_SECONDS,
      toleranceMm: 350,
    };
  }

  async _cancelCalibrationPreparation() {
    this._clearCalibrationTimer();
    const hasServerPhase = this.status?.fixed_point_calibration?.state === 'active';
    if (hasServerPhase) {
      this.busy = true;
      this._render();
      try {
        const response = await fetch(this._endpoint(MM_WAVE_FIXED_POINTS_CANCEL_ENDPOINT), {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: '{}',
        });
        const payload = await response.json().catch(() => ({}));
        if (!response.ok) throw new Error(payload.error || `Abbruch fehlgeschlagen: HTTP ${response.status}`);
        this.status = payload;
      } catch (error) {
        this._setActionError(error.message || 'Phase 1 konnte nicht abgebrochen werden.');
        this.busy = false;
        this._render();
        return;
      }
      this.busy = false;
    }
    this.calibrationPlan = null;
    this._clearActionError();
    this._render();
  }

  async _scheduleCalibration(form) {
    const read = (name) => Number(form.querySelector(`[name="${name}"]`)?.value);
    const durationSeconds = read('duration_seconds');
    const leadSeconds = read('lead_seconds');
    const toleranceMm = read('tolerance_mm');
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
    if (!Number.isInteger(toleranceMm) || toleranceMm < 50 || toleranceMm > 2000) {
      this._setActionError('Die Punkttoleranz muss zwischen 50 und 2000 mm liegen.');
      this._render();
      return;
    }

    this._clearActionError();
    this.calibrationPlan = { phase: 'phase_one_starting', durationSeconds, leadSeconds, toleranceMm };
    this._render();
    await this._startFixedPointPhase();
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
    if (!['anchor_countdown', 'anchor_measuring'].includes(plan?.phase) || this.busy) return;
    if (Date.now() >= plan.startsAtMs) {
      if (plan.phase === 'anchor_countdown') {
        this.calibrationPlan = {
          ...plan,
          phase: 'anchor_measuring',
          startsAtMs: Date.now() + FIXED_POINT_MEASUREMENT_MS,
          displaySeconds: Math.ceil(FIXED_POINT_MEASUREMENT_MS / 1000),
        };
        this._render();
      } else {
        this._clearCalibrationTimer();
        void this._checkCurrentFixedPoint();
      }
      return;
    }
    const remaining = this._calibrationRemainingSeconds();
    if (plan.displaySeconds !== remaining) {
      this.calibrationPlan = { ...plan, displaySeconds: remaining };
      this._render();
    }
  }

  _calibrationRemainingSeconds() {
    if (['anchor_countdown', 'anchor_measuring'].includes(this.calibrationPlan?.phase)) {
      return Math.max(0, Math.ceil((this.calibrationPlan.startsAtMs - Date.now()) / 1000));
    }
    const remaining = Number(this.status?.session?.empty_remaining_seconds);
    return Number.isFinite(remaining) ? Math.max(0, Math.ceil(remaining)) : 0;
  }

  _calibrationSafeReturnAt() {
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
    const fixed = this.status?.fixed_point_calibration;
    const current = fixed?.anchors?.find((anchor) => anchor.id === fixed.current_anchor_id);
    const anchors = Array.isArray(fixed?.anchors) ? fixed.anchors : [];
    const anchorProgress = anchors.length > 0
      ? `<ol class="mmwave-fixed-points">${anchors.map((anchor) => `<li class="is-${escapeHTML(anchor.state)}"><strong>${escapeHTML(anchor.id)}</strong><span>${anchor.state === 'complete' ? `gemessen${Number.isFinite(Number(anchor.check?.median_error_mm)) ? ` · ${escapeHTML(anchor.check.median_error_mm)} mm` : ''}` : anchor.state === 'current' ? 'als Nächstes' : 'wartet'}</span></li>`).join('')}</ol>`
      : '';
    if (plan?.phase === 'form') {
      return `
        <div class="mmwave-preparation">
          <div class="mmwave-eyebrow">ZWEI PHASEN</div>
          <h4>Zuerst RX1–RX4 und TX, danach CSI</h4>
          <p>Vor jedem festen Punkt läuft derselbe Vorlauf. Danach bitte 10 Sekunden stillstehen. Aus allen fünf Messpunkten berechnet der Server gemeinsam den Winkel mit dem kleinsten Gesamtfehler.</p>
          <form id="mmwaveCalibrationPrepareForm" class="mmwave-preparation-form">
            <label><span>Wegezeit je Punkt (Sekunden)</span><input name="lead_seconds" type="number" min="${CALIBRATION_MIN_LEAD_SECONDS}" step="1" value="${plan.leadSeconds}" required></label>
            <label><span>Punkttoleranz (mm)</span><input name="tolerance_mm" type="number" min="50" max="2000" step="10" value="${plan.toleranceMm}" required></label>
            <label><span>CSI-Leerreferenz in Phase 2 (Sekunden) · mindestens 60</span><input name="duration_seconds" type="number" min="${EMPTY_REFERENCE_MIN_SECONDS}" max="3600" step="1" value="${plan.durationSeconds}" required></label>
            <div class="mmwave-preparation-actions">
              <button type="submit" class="mmwave-primary-button">Phase 1 starten</button>
              <button type="button" data-mmwave-action="cancel-calibration-preparation" class="mmwave-secondary-button">Abbrechen</button>
            </div>
          </form>
        </div>`;
    }
    if (plan?.phase === 'phase_one_starting') {
      return '<div class="mmwave-preparation is-countdown"><div class="mmwave-eyebrow">PHASE 1</div><h4>Feste Punkte werden vorbereitet …</h4></div>';
    }
    if (plan?.phase === 'anchor_waiting') {
      return `
        <div class="mmwave-preparation">
          <div class="mmwave-eyebrow">PHASE 1 · FESTE PUNKTE</div>
          <h4>Als Nächstes: ${escapeHTML(fixed?.current_anchor_id || '--')}</h4>
          <p>Starte den Vorlauf, gehe zum markierten Kalibrierstandpunkt <strong>${escapeHTML(fixed?.current_anchor_id || '--')}</strong> und bleibe anschließend für die 10-sekündige Messung ruhig stehen.</p>
          ${anchorProgress}
          <div class="mmwave-preparation-actions">
            <button type="button" data-mmwave-action="start-anchor-countdown" class="mmwave-primary-button">${escapeHTML(fixed?.current_anchor_id || 'Punkt')} starten</button>
            <button type="button" data-mmwave-action="cancel-calibration-preparation" class="mmwave-secondary-button">Phase 1 abbrechen</button>
          </div>
        </div>`;
    }
    if (plan?.phase === 'anchor_countdown') {
      return `
        <div class="mmwave-preparation is-countdown">
          <div class="mmwave-eyebrow">PHASE 1 · ${escapeHTML(fixed?.current_anchor_id || plan.anchorId || '')}</div>
          <h4>Noch ${this._calibrationRemainingSeconds()} s bis zur Messung</h4>
          <p>Gehe jetzt zum markierten Kalibrierstandpunkt ${escapeHTML(fixed?.current_anchor_id || plan.anchorId || 'dem Punkt')}.</p>
          ${anchorProgress}
          <button type="button" data-mmwave-action="cancel-calibration-preparation" class="mmwave-secondary-button">Phase 1 abbrechen</button>
        </div>`;
    }
    if (plan?.phase === 'anchor_measuring') {
      return `<div class="mmwave-preparation is-countdown"><div class="mmwave-eyebrow">PHASE 1 · MESSUNG</div><h4>Bei ${escapeHTML(fixed?.current_anchor_id || plan.anchorId || '')} noch ${this._calibrationRemainingSeconds()} s stillstehen</h4><p>Radar-Samples werden jetzt gesammelt.</p>${anchorProgress}</div>`;
    }
    if (plan?.phase === 'anchor_failed') {
      return `<div class="mmwave-preparation"><div class="mmwave-eyebrow">PHASE 1 · KEINE MESSUNG</div><h4>${escapeHTML(fixed?.current_anchor_id || plan?.anchorId || '')} erneut messen</h4><p>Im Messfenster wurde kein frisches einzelnes Radarziel erfasst. Bleibe allein und ruhig am markierten Kalibrierstandpunkt und wiederhole den Vorlauf.</p>${anchorProgress}<div class="mmwave-preparation-actions"><button type="button" data-mmwave-action="retry-anchor" class="mmwave-primary-button">Erneut versuchen</button><button type="button" data-mmwave-action="cancel-calibration-preparation" class="mmwave-secondary-button">Phase 1 abbrechen</button></div></div>`;
    }
    if (plan?.phase === 'phase_two_ready') {
      return `<div class="mmwave-preparation"><div class="mmwave-eyebrow">PHASE 1 ABGESCHLOSSEN</div><h4>Phase 2 · CSI-Kalibrierung</h4><p>RX1–RX4 und TX wurden einzeln erfasst. Die gemeinsame Winkelkorrektur gilt jetzt für alle weiteren Radarpositionen. Phase 2 erstellt zuerst die CSI-Leerreferenz; danach gehst du langsam durch alle Bereiche des Raums.</p>${anchorProgress}${this._yawCalibrationMarkup(fixed?.yaw_calibration || this.status?.yaw_calibration)}<div class="mmwave-preparation-actions"><button type="button" data-mmwave-action="start-phase-two" class="mmwave-primary-button" ${this.status?.preflight?.ready && !this.busy ? '' : 'disabled'}>Phase 2 starten</button><button type="button" data-mmwave-action="repeat-yaw-calibration" class="mmwave-secondary-button" ${this.busy ? 'disabled' : ''}>Winkelmessung wiederholen</button></div>${this._startRequirement(this.status)}</div>`;
    }
    return `
      <div class="mmwave-preparation is-countdown">
        <div class="mmwave-eyebrow">PHASE 2</div>
        <h4>CSI-Kalibrierung wird gestartet …</h4>
        <p>Für die erste Leerreferenz bitte den Raum verlassen.</p>
      </div>`;
  }

  async _startFixedPointPhase() {
    this.busy = true;
    this._clearActionError();
    this._render();
    try {
      const response = await fetch(this._endpoint(MM_WAVE_FIXED_POINTS_START_ENDPOINT), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: '{}',
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(payload.error || `Phase 1 konnte nicht starten: HTTP ${response.status}`);
      repeatResetActive = false;
      this.fixedPointResultDismissed = false;
      storeRepeatReset(false);
      this.status = { ...this.status, ...payload, yaw_calibration: null };
      this.calibrationPlan = { ...this.calibrationPlan, phase: 'anchor_waiting' };
    } catch (error) {
      this._setActionError(error.message || 'Phase 1 konnte nicht gestartet werden.');
      this.calibrationPlan = { ...this.calibrationPlan, phase: 'form' };
    } finally {
      this.busy = false;
      this._render();
    }
  }

  _scheduleCurrentAnchorCountdown() {
    const anchorId = this.status?.fixed_point_calibration?.current_anchor_id;
    if (!anchorId) return;
    const leadSeconds = Number(this.calibrationPlan?.leadSeconds) || CALIBRATION_DEFAULT_LEAD_SECONDS;
    this._clearActionError();
    this.calibrationPlan = {
      ...(this.calibrationPlan || this._defaultCalibrationPlan()),
      phase: 'anchor_countdown',
      anchorId,
      startsAtMs: Date.now() + leadSeconds * 1000,
      displaySeconds: leadSeconds,
    };
    this._startCalibrationTimer();
    this._render();
  }

  async _checkCurrentFixedPoint() {
    this.busy = true;
    this._clearActionError();
    this._render();
    try {
      const response = await fetch(this._endpoint(MM_WAVE_FIXED_POINTS_CHECK_ENDPOINT), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          tolerance_mm: Math.round(Number(this.calibrationPlan?.toleranceMm) || 350),
          window_ms: FIXED_POINT_MEASUREMENT_MS,
        }),
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(payload.error || `Punktmessung fehlgeschlagen: HTTP ${response.status}`);
      this.status = { ...this.status, ...payload };
      const fixed = payload.fixed_point_calibration;
      if (fixed?.state === 'complete') {
        this.calibrationPlan = { ...this.calibrationPlan, phase: 'phase_two_ready' };
      } else {
        this.calibrationPlan = {
          ...this.calibrationPlan,
          phase: 'anchor_waiting',
        };
      }
    } catch (error) {
      this._setActionError(error.message || 'Punktmessung fehlgeschlagen.');
      this.calibrationPlan = { ...this.calibrationPlan, phase: 'anchor_failed' };
    } finally {
      this.busy = false;
      this._render();
    }
  }

  _yawCalibrationMarkup(calibration) {
    if (!calibration) {
      return '<p class="mmwave-helper">Noch keine gemeinsame Winkelkorrektur vorhanden.</p>';
    }
    const anchors = Array.isArray(calibration.anchors) ? calibration.anchors : [];
    const rows = anchors.map((anchor) => `
      <tr>
        <th scope="row">${escapeHTML(anchor.id)}</th>
        <td>${escapeHTML(anchor.before_error_mm)} mm</td>
        <td>${escapeHTML(anchor.after_error_mm)} mm</td>
      </tr>`).join('');
    const state = calibration.applied ? 'KORREKTUR AKTIV' : 'WINKEL UNVERÄNDERT';
    return `
      <section class="mmwave-yaw-result" aria-label="Ergebnis der Winkelkalibrierung">
        <div class="mmwave-yaw-result-heading">
          <span>${state}</span>
          <strong>${(Number(calibration.optimized_yaw_mdeg) / 1000).toFixed(1)}°</strong>
        </div>
        <p>Gesamtfehler (RMS) ${escapeHTML(calibration.before_rms_error_mm)} → ${escapeHTML(calibration.after_rms_error_mm)} mm · ${Number(calibration.improvement_percent || 0).toFixed(1)} % besser · ${escapeHTML(calibration.point_count)} Punkte gemeinsam.</p>
        <table><thead><tr><th>Punkt</th><th>Vorher</th><th>Nachher</th></tr></thead><tbody>${rows}</tbody></table>
        <p class="mmwave-helper">Basis ${(Number(calibration.base_yaw_mdeg) / 1000).toFixed(1)}° · Korrektur ${(Number(calibration.correction_mdeg) / 1000).toFixed(1)}° · ${calibration.source_kind === 'setup' ? 'setupgebunden' : 'profilgebunden'} gespeichert.</p>
      </section>`;
  }

  async _startCalibration(plan) {
    this.busy = true;
    this._clearActionError();
    this.calibrationPlan = { ...plan, phase: 'phase_two_starting', displaySeconds: 0 };
    this._render();
    try {
      const calibrationContext = this.calibrationContextProvider();
      if (!calibrationContext?.profile_id) {
        throw new Error('Im Control Center muss ein unveränderliches Setup-Profil ausgewählt sein.');
      }
      const response = await fetch(this._endpoint(MM_WAVE_SESSION_START_ENDPOINT), {
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
      this.calibrationPlan = { ...plan, phase: 'phase_two_ready' };
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _checkKnownPoint(form) {
    const read = (name) => Number(form.querySelector(`[name="${name}"]`)?.value);
    const expectedX = read('expected_x_m');
    const expectedZ = read('expected_z_m');
    const tolerance = read('tolerance_mm');
    if (![expectedX, expectedZ, tolerance].every(Number.isFinite)
      || Math.abs(expectedX) > 100 || Math.abs(expectedZ) > 100
      || tolerance < 50 || tolerance > 2000) {
      this._setActionError('X/Z und Toleranz müssen gültige Werte im unterstützten Bereich sein.');
      this._render();
      return;
    }
    this.busy = true;
    this.pointCheck = { phase: 'checking' };
    this._clearActionError();
    this._render();
    try {
      const response = await fetch(this._endpoint(MM_WAVE_KNOWN_POINT_ENDPOINT), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          expected_position_m: [expectedX, expectedZ],
          tolerance_mm: Math.round(tolerance),
          window_ms: 1500,
        }),
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(payload.error || `Punktprüfung fehlgeschlagen: HTTP ${response.status}`);
      this.pointCheck = { phase: 'complete', result: payload };
    } catch (error) {
      this.pointCheck = null;
      this._setActionError(error.message || 'Punktprüfung fehlgeschlagen.');
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

    this.container.querySelectorAll('[data-mmwave-node]').forEach((button) => {
      const nodeId = button.dataset.mmwaveNode;
      const available = this.availableNodeIds.includes(nodeId);
      button.disabled = !available || this.busy;
      button.setAttribute('aria-pressed', String(this.nodeId === nodeId));
      button.classList.toggle('is-active', this.nodeId === nodeId);
      button.title = available ? `Sensor ${nodeId} auswählen` : `${nodeId} ist im aktiven Setup nicht konfiguriert`;
    });

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
    this._renderPointCheckResult();
    const yawResult = this.container.querySelector('#mmwaveYawCalibrationResult');
    if (yawResult) yawResult.innerHTML = this.status?.yaw_calibration
      ? `${this._yawCalibrationMarkup(this.status.yaw_calibration)}
        <div class="mmwave-preparation-actions">
          <button type="button" data-mmwave-action="repeat-yaw-calibration" class="mmwave-secondary-button" ${this.busy || this.status?.session ? 'disabled' : ''}>Winkel neu kalibrieren</button>
        </div>`
      : '';
  }

  _renderPointCheckResult() {
    const output = this.container.querySelector('#mmwavePointCheckResult');
    if (!output) return;
    if (this.pointCheck?.phase === 'checking') {
      output.textContent = 'Prüfe frische Radarziele …';
      return;
    }
    const result = this.pointCheck?.result;
    if (!result) {
      output.textContent = '';
      return;
    }
    const verdict = result.pass ? 'BESTANDEN' : 'ABWEICHUNG';
    output.textContent = `${verdict} · erwartet ${result.expected_position_mm?.join(' / ')} mm · gemessen ${result.observed_position_mm?.join(' / ')} mm · Medianfehler ${result.median_error_mm} mm · ${result.sample_count} Samples`;
    output.className = `mmwave-point-check-result ${result.pass ? 'is-pass' : 'is-fail'}`;
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
      if (model.session.kind === 'mmwave_only' && model.session.mmwave_only_validity) {
        return `
          <h4>mmWave-only-Prüfung ${model.session.mmwave_only_validity.verdict === 'valid' ? 'bestanden' : 'nicht bestanden'}</h4>
          ${this._mmwaveOnlyValidityMarkup(model.session.mmwave_only_validity)}
          ${this._transportFacts(status)}
          <button data-mmwave-action="stop" class="mmwave-secondary-button" ${this.busy ? 'disabled' : ''}>Ergebnis schließen</button>
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
    if (this.calibrationPlan && this.calibrationPlan.phase !== 'form') {
      return this._calibrationPreparationMarkup();
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
        coverage: 'Langsam durch alle erreichbaren Bereiche gehen.',
        training: recommendedInstruction || 'Bereiche abgehen, dünne Stellen kurz halten.',
        blind: recommendedInstruction || 'Alle Bereiche erneut besuchen.',
        mmwave_only: Number.isFinite(emptyRemaining)
          ? `mmWave-only-Prüfung läuft noch ${Math.max(0, Math.ceil(emptyRemaining))} s; Raum leer lassen.`
          : 'mmWave-only-Prüfung läuft; Raum leer lassen.',
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
        ${this._mmwaveOnlyValidityMarkup(model.session.mmwave_only_validity)}
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
      <p>Phase 1 bestätigt RX1–RX4 und TX einzeln. Danach verknüpft Phase 2 den langsamen Rundgang mit Radar-X/Z und CSI.</p>
      <dl class="mmwave-facts"><div><dt>Node</dt><dd>${escapeHTML(status.node_id || '--')}</dd></div><div><dt>Modus</dt><dd>${escapeHTML(status.mode || '--')}</dd></div><div><dt>Radar X/Z</dt><dd>${Array.isArray(status.target_position_mm) ? `${(status.target_position_mm[0] / 1000).toFixed(2)} / ${(status.target_position_mm[1] / 1000).toFixed(2)} m` : '--'}</dd></div><div><dt>Alter</dt><dd>${status.packet_age_ms ?? '--'} ms</dd></div></dl>
      ${this._transportFacts(status)}
      <button data-mmwave-action="prepare-calibration" class="mmwave-primary-button" ${model.fixedPointPreflightReady && (status?.setup_sealed || status?.cad_profile) && !this.busy ? '' : 'disabled'}>Zwei-Phasen-Kalibrierung</button>
      <button data-mmwave-action="start-mmwave-only" class="mmwave-secondary-button" ${model.radarPreflightReady && !this.busy ? '' : 'disabled'}>mmWave-only prüfen (60 s)</button>
      ${this._startRequirement(status)}
      ${this._radarOnlyStartRequirement(status)}
      <p class="mmwave-helper">Phase 1 braucht nur Radar und die gespeicherte CAD-Geometrie. Das versiegelte Setup und RX/CSI werden erst für Phase 2 vorausgesetzt. SOFTWARE-ONLY / UNVALIDATED bis Blindtest.</p>
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

  _mmwaveOnlyValidityMarkup(validity) {
    if (!validity || !['valid', 'invalid'].includes(validity.verdict)) return '';
    const valid = validity.verdict === 'valid';
    const reasons = Array.isArray(validity.reasons) ? validity.reasons : [];
    return `
      <section class="mmwave-validity ${valid ? 'is-valid' : 'is-invalid'}" aria-label="mmWave-only-Ergebnis">
        <strong>Radar-only: ${valid ? 'GÜLTIG' : 'UNGÜLTIG'}</strong>
        <p>${Number(validity.radar_packets || 0).toLocaleString('de-DE')} Radar-Pakete · ${Number(validity.no_target_packets || 0).toLocaleString('de-DE')} leere Frames · maximale Lücke ${Number(validity.max_radar_gap_ms || 0).toLocaleString('de-DE')} ms</p>
        ${reasons.length > 0 ? `<ul>${reasons.map((reason) => `<li>${escapeHTML(reason)}</li>`).join('')}</ul>` : '<p>Keine Gültigkeitsverletzung aufgezeichnet.</p>'}
      </section>`;
  }

  _transportFacts(status) {
    const diagnostic = mmwaveTransportDiagnostic(status);
    const counter = (value) => value != null && Number.isFinite(Number(value)) ? Number(value).toLocaleString('de-DE') : '--';
    const duration = (value) => Number.isFinite(Number(value)) ? `${counter(value)} ms` : '--';
    const rate = (value) => Number.isFinite(Number(value)) ? `${Number(value).toFixed(2)} Hz` : '--';
    const nodeControl = status.node_control || {};
    let nodeStatus = 'noch nicht geprüft';
    if (nodeControl.reachable === true) {
      nodeStatus = nodeControl.last_success_age_ms == null
        ? 'erreichbar · Diagnosezähler fehlen'
        : `erreichbar · letzter Status vor ${duration(nodeControl.last_success_age_ms)}`;
    } else if (nodeControl.reachable === false) {
      nodeStatus = `${nodeControl.last_error_kind || 'nicht erreichbar'}${nodeControl.last_error ? ` · ${nodeControl.last_error}` : ''}`;
    } else if (!nodeControl.url_configured || !nodeControl.token_configured) {
      nodeStatus = !nodeControl.url_configured
        ? 'Radar-Adresse fehlt · automatische Suche läuft'
        : 'Zugriffstoken fehlt · UDP-Empfang bleibt möglich';
    }
    const rejectReasons = Object.entries(status.reject_reasons || {})
      .filter(([, value]) => Number(value) > 0)
      .map(([category, value]) => `${escapeHTML(category)} ${counter(value)}`)
      .join(' · ') || '--';
    const recentRejectReasons = Object.entries(status.reject_reasons_window || {})
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
      ${status.connection?.hint ? `<p class="mmwave-helper" role="status">${escapeHTML(status.connection.hint)}</p>` : ''}
      ${status.cad_profile_error ? `<p class="mmwave-helper" role="alert">${escapeHTML(status.cad_profile_error)}</p>` : ''}
      ${status.cad_profile ? `<p class="mmwave-helper">CAD-Profil ${escapeHTML(status.cad_profile.profile_sha256.slice(0, 16))}… aktiv · Montage ${status.cad_profile.mounting_position_m.map((value) => escapeHTML(value)).join(' / ')} m · Vorschau mit Sensor-Ausrichtung, noch nicht versiegelt.</p>` : ''}
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
        <div><dt>Messfenster gültig</dt><dd>${counter(status.transport?.valid_samples)} / ${counter(status.transport?.window_samples)}</dd></div>
        <div><dt>Gültige Rate</dt><dd>${rate(status.transport?.valid_rate_hz)}</dd></div>
        <div><dt>Ankunft Median / P95</dt><dd>${duration(status.transport?.inter_arrival_median_ms)} / ${duration(status.transport?.inter_arrival_p95_ms)}</dd></div>
        <div><dt>Verarbeitung Median / P95</dt><dd>${duration(status.transport?.receive_to_process_median_ms)} / ${duration(status.transport?.receive_to_process_p95_ms)}</dd></div>
      </dl>
      <p class="mmwave-helper">Verworfen nach Grund: ${rejectReasons}</p>
      <p class="mmwave-helper">Verworfen im aktuellen 25-s-Fenster: ${counter(status.packets_rejected_window)} · ${recentRejectReasons}</p>
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
      return '<p class="mmwave-helper">Radar ausrichten, versiegeltes Setup aktivieren, Server neu starten.</p>';
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

  _radarOnlyStartRequirement(status) {
    const gates = Array.isArray(status?.radar_preflight?.gates)
      ? status.radar_preflight.gates
      : [];
    if (status?.radar_preflight?.ready) {
      return '<p class="mmwave-helper">Radar-only-Preflight bestanden. RX/CSI und ein versiegeltes Setup sind für diese Prüfung nicht erforderlich.</p>';
    }
    const blockers = gates.filter((gate) => !gate.pass);
    if (blockers.length === 0) {
      return '<p class="mmwave-helper">Radar-only-Preflight wartet auf Statusdaten.</p>';
    }
    return `<div class="mmwave-helper"><strong>Radar-only:</strong><ul>${blockers.map((gate) => `<li>${escapeHTML(preflightGateLabel(gate.id))} – ${escapeHTML(gate.detail)}</li>`).join('')}</ul></div>`;
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
