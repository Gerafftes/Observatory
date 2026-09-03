import { apiService } from '../services/api.service.js';
import { experimentService } from '../services/experiment.service.js';
import {
  DEFAULT_MMWAVE_POSITION_M,
  DEFAULT_SENSOR_MOUNT_RADIUS_M,
  mmwaveMountingPosition,
  MMWAVE_SENSOR,
  RoomGeometryEditor,
  validateGeometryDraft,
} from './RoomGeometryEditor.js';

const EXPECTED_POINTS = Array.from({ length: 9 }, (_, index) => `P${String(index + 1).padStart(2, '0')}`);
const EMPTY_CALIBRATION_MIN_SECONDS = 60;
const EMPTY_CALIBRATION_DEFAULT_SECONDS = 60;
const EMPTY_CALIBRATION_MIN_LEAD_SECONDS = 5;
const EMPTY_CALIBRATION_DEFAULT_LEAD_SECONDS = 20;
const EMPTY_CALIBRATION_TIMER_MS = 250;
const WORKFLOW_PHASES = [
  'create_experiment',
  'seal_setup',
  'empty_calibration',
  'train_p01_p09',
  'randomize_blind_positions',
  'capture',
  'predict',
  'reveal_truth',
  'evaluate',
  'report',
];

export function defaultSetupProfileDocument() {
  const room = [4.02, 2.59, 3.44];
  const x = [1.01, 2.01, 3.01];
  const z = [0.86, 1.72, 2.58];
  return {
    schema_version: 1,
    profile_kind: 'ruview.setup-profile',
    room_dimensions_m: room,
    sensor_mount_radius_m: DEFAULT_SENSOR_MOUNT_RADIUS_M,
    transmitter: { id: 'TX', position_m: [1.51, 1.19, 0.39] },
    receivers: [
      { id: 'RX1', role: 'receiver', position_m: [0.00, 0.50, 0.28] },
      { id: 'RX2', role: 'receiver', position_m: [4.02, 0.87, 0.97] },
      { id: 'RX3', role: 'receiver', position_m: [0.00, 0.74, 2.11] },
      { id: 'RX4', role: 'receiver', position_m: [4.02, 0.87, 2.46] },
    ],
    mmwave: {
      sensor: MMWAVE_SENSOR,
      mounting_position_m: [...DEFAULT_MMWAVE_POSITION_M],
      mounting_revision: 'draft',
      allow_exterior: true,
    },
    points: EXPECTED_POINTS.map((id, index) => ({
      id,
      coordinates_m: [x[index % 3], 0, z[Math.floor(index / 3)]],
    })),
    radio: { channel: 6 },
    environment: {
      layout_revision: 'draft',
      furniture_revision: 'draft',
      door_state_revision: 'closed',
    },
    mmwave_status: 'NOT_CONNECTED',
  };
}

export function generateThreeByThreePoints(dimensions) {
  const xs = [0.25, 0.5, 0.75].map((factor) => Math.round(numberValue(dimensions?.[0]) * factor * 100) / 100);
  const zs = [0.25, 0.5, 0.75].map((factor) => Math.round(numberValue(dimensions?.[2]) * factor * 100) / 100);
  return EXPECTED_POINTS.map((id, index) => ({
    id,
    coordinates_m: [xs[index % 3], 0, zs[Math.floor(index / 3)]],
  }));
}

function escapeHTML(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

function attribute(value) {
  return escapeHTML(value);
}

function infoTip(label, text, modifier = '') {
  const className = ['occ-info', modifier].filter(Boolean).join(' ');
  return `<span class="${className}" tabindex="0" role="note" aria-label="${attribute(`${label}: ${text}`)}"><span aria-hidden="true">i</span><span class="occ-info-tooltip" role="tooltip">${escapeHTML(text)}</span></span>`;
}

function panelHeading(kicker, title, label, text, modifier = '') {
  return `<div class="occ-panel-heading"><div><div class="occ-kicker">${escapeHTML(kicker)}</div><div class="occ-title-line"><h4>${escapeHTML(title)}</h4>${infoTip(label, text, modifier)}</div></div></div>`;
}

function numberValue(value, fallback = 0) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function formatTime(value) {
  if (!value) return '--';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString('de-DE');
}

function formatClockTime(value) {
  if (!Number.isFinite(value)) return '--:--:--';
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? '--:--:--'
    : date.toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false });
}

function phaseLabel(phase) {
  return {
    create_experiment: 'Experiment anlegen',
    seal_setup: 'Setup versiegeln',
    empty_calibration: 'Leerkalibrierung',
    train_p01_p09: 'mmWave-geführte Kalibrierung',
    randomize_blind_positions: 'Blind-Reihenfolge',
    capture: 'Blindaufnahmen',
    predict: 'Prediction-Artefakt',
    reveal_truth: 'Truth aufdecken',
    evaluate: 'Evaluation',
    report: 'Report',
  }[phase] || phase;
}

function eventPayload(run, phase, key) {
  return (run?.workflow?.events || []).filter((event) => event.phase === phase)
    .map((event) => event.payload || {})
    .reverse()
    .find((payload) => payload[key] != null);
}

function phaseEvents(run, phase) {
  return (run?.workflow?.events || []).filter((event) => event.phase === phase);
}

function nextPhase(run) {
  const current = WORKFLOW_PHASES.indexOf(run?.workflow?.current_phase);
  return current >= 0 ? WORKFLOW_PHASES[Math.min(current + 1, WORKFLOW_PHASES.length - 1)] : null;
}

function shuffledBlindOrder(seed) {
  const order = [...EXPECTED_POINTS, ...EXPECTED_POINTS];
  let state = seed >>> 0;
  for (let index = order.length - 1; index > 0; index -= 1) {
    state = (Math.imul(1664525, state) + 1013904223) >>> 0;
    const swap = state % (index + 1);
    [order[index], order[swap]] = [order[swap], order[index]];
  }
  return order;
}

function blindOrderCommitment(order) {
  let hash = 2166136261;
  for (const character of order.join('|')) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return `fnv1a32-${(hash >>> 0).toString(16).padStart(8, '0')}`;
}

function profileField(document, path, fallback = '') {
  const [section, key] = path.split('.');
  return document?.[section]?.[key] ?? fallback;
}

export class ObservatoryControlCenter {
  constructor(container) {
    this.container = container;
    this.status = null;
    this.calibrationAvailability = null;
    this.profiles = [];
    this.runs = [];
    this.recordings = [];
    this.models = [];
    this.benchmarkCatalog = null;
    this.selectedProfile = null;
    this.selectedRun = null;
    this.profileDraft = defaultSetupProfileDocument();
    this.profileLabel = 'Fixed room / WiFi-only';
    this.workflowLabel = 'WiFi-only position experiment';
    this.trainingPoint = 'P01';
    this.blindPoint = 'P01';
    this.blindOrder = [];
    this.blindIndex = 0;
    this.artifactPaths = { prediction: '', truth: '', evaluation: '' };
    this.connectionState = 'loading';
    this.profilesExpanded = false;
    this.runsExpanded = false;
    this.pointGridExpanded = false;
    this.manualCaptureExpanded = false;
    this.runSelectionCleared = false;
    this._nextRetryAt = 0;
    this.busy = false;
    this.error = '';
    this.message = '';
    this._mounted = false;
    this._pollTimer = null;
    this._emptyCalibrationTimer = null;
    this.emptyCalibrationPlan = null;
    this.geometryEditor = null;
  }

  mount() {
    if (this._mounted) return;
    this._mounted = true;
    this.container.addEventListener('click', (event) => this._onClick(event));
    this.container.addEventListener('submit', (event) => this._onSubmit(event));
    this.container.addEventListener('input', (event) => this._onInput(event));
    this.container.addEventListener('change', (event) => this._onInput(event));
    this._render();
    void this.refresh();
    this._pollTimer = setInterval(() => void this.refresh({ quiet: true }), 5000);
  }

  dispose() {
    this._mounted = false;
    if (this._pollTimer) clearInterval(this._pollTimer);
    this._pollTimer = null;
    this._clearEmptyCalibrationTimer();
    this.geometryEditor?.dispose();
    this.geometryEditor = null;
  }

  async refresh({ quiet = false, allowWhileBusy = false } = {}) {
    if (!this._mounted || (this.busy && !allowWhileBusy)) return;
    if (quiet && this.connectionState === 'offline' && Date.now() < this._nextRetryAt) return;
    if (!quiet) {
      this.message = 'Aktualisiere …';
      this.error = '';
      if (!this.status) this.connectionState = 'loading';
    }
    let serverReached = false;
    try {
      const status = await experimentService.getControlCenterStatus();
      serverReached = true;
      const [profiles, runs, recordings, models, benchmarkCatalog] = await Promise.all([
        experimentService.listProfiles(),
        experimentService.listRuns(50),
        apiService.get('/api/v1/recording/list').then((payload) => payload?.recordings || []),
        apiService.get('/api/v1/models').then((payload) => payload?.models || []).catch(() => []),
        experimentService.getBenchmarkCatalog().catch(() => null),
      ]);
      this.status = status;
      this.profiles = profiles;
      this.runs = runs;
      this.recordings = recordings;
      this.models = models;
      this.benchmarkCatalog = benchmarkCatalog;
      if (this.selectedProfile) {
        this.selectedProfile = profiles.find((profile) => profile.id === this.selectedProfile.id) || null;
      }
      if (!this.selectedProfile && profiles[0]) this._selectProfile(profiles[0]);
      if (this.selectedRun) {
        this.selectedRun = runs.find((run) => run.id === this.selectedRun.id) || this.selectedRun;
      }
      if (!this.selectedRun && !this.runSelectionCleared && runs[0]) this.selectedRun = runs[0];
      const calibrationContext = this._calibrationContextRequest();
      this.calibrationAvailability = calibrationContext
        ? await experimentService.getCalibrationAvailability({
          profileId: calibrationContext.profile_id,
          profileRevisionId: calibrationContext.profile_revision_id,
        }).catch(() => ({ available: false }))
        : null;
      if (!quiet) this.message = '';
      this.error = '';
      this.connectionState = 'ready';
      this._nextRetryAt = 0;
    } catch (error) {
      this.connectionState = serverReached ? 'degraded' : 'offline';
      this._nextRetryAt = Date.now() + 30000;
      this.message = '';
      this.error = serverReached
        ? 'Daten teilweise verfügbar. Aktualisieren oder Server prüfen.'
        : 'Server nicht erreichbar. Aktionen gesperrt.';
    } finally {
      if (!quiet || !this._editorHasFocus()) this._render();
    }
  }

  _editorHasFocus() {
    if (typeof document === 'undefined') return false;
    const active = document.activeElement;
    return Boolean(active && this.container?.contains(active) && active.matches('input, select, textarea'));
  }

  _selectProfile(profile) {
    this.selectedProfile = profile;
    this.profileLabel = profile.label;
    this.profileDraft = typeof structuredClone === 'function'
      ? structuredClone(profile.document)
      : JSON.parse(JSON.stringify(profile.document));
  }

  _calibrationContextRequest() {
    const workflow = this.selectedRun?.workflow;
    const profileId = workflow?.profile_id || this.selectedProfile?.id;
    if (!profileId) return null;
    const profileRevisionId = workflow
      ? workflow.profile_revision_id
      : this.selectedProfile?.revision_id;
    return {
      profile_id: profileId,
      ...(profileRevisionId ? { profile_revision_id: profileRevisionId } : {}),
    };
  }

  calibrationContextRequest() {
    return this._calibrationContextRequest();
  }

  /**
   * Return the exact geometry currently rendered by the CAD editor. The
   * read-only Radar/RX debug view consumes this snapshot so its fixed mmWave
   * marker cannot drift to a server fallback when setup metadata is absent.
   */
  getCurrentGeometrySnapshot() {
    const profile = this.profileDraft || {};
    return {
      roomDimensions: Array.isArray(profile.room_dimensions_m)
        ? profile.room_dimensions_m.slice(0, 3)
        : null,
      mountingPositionM: mmwaveMountingPosition(profile),
      txPosition: Array.isArray(profile.transmitter?.position_m)
        ? profile.transmitter.position_m.slice(0, 3)
        : null,
      receiverPositionsM: Array.isArray(profile.receivers)
        ? profile.receivers.map((receiver) => ({
          id: receiver.id,
          position: Array.isArray(receiver.position_m)
            ? receiver.position_m.slice(0, 3)
            : null,
        }))
        : [],
    };
  }

  _setupV2DraftActionMarkup() {
    const profile = this.selectedProfile;
    if (!profile?.id || this.status?.capabilities?.setup_v2_draft_export !== true) return '';
    const revision = profile.revision_id
      ? `?revision_id=${encodeURIComponent(profile.revision_id)}`
      : '';
    const endpoint = `/api/v1/experiments/setup-profiles/${encodeURIComponent(profile.id)}/setup-v2-draft${revision}`;
    const filename = `${profile.id}-v${profile.version || 1}-setup-v2.draft.json`;
    return `<a class="occ-button occ-button-quiet" href="${attribute(endpoint)}" download="${attribute(filename)}">Setup-v2-Entwurf laden</a>`;
  }

  _activePositionSetup() {
    const setup = this.status?.position_setup;
    return setup?.active === true && setup.setup_id && setup.setup_sha256 ? setup : null;
  }

  _workflowRuntimeSeal(run = this.selectedRun) {
    const setup = this._activePositionSetup();
    const workflow = run?.workflow;
    if (!setup || !workflow) return null;
    return [...(workflow.events || [])].reverse().find((event) => (
      event.phase === 'seal_setup'
      && event.status === 'PASS'
      && event.payload?.seal_kind === 'active_position_setup_v2'
      && event.payload?.profile_sha256 === workflow.profile_sha256
      && event.payload?.setup_id === setup.setup_id
      && event.payload?.setup_sha256 === setup.setup_sha256
    )) || null;
  }

  _render() {
    if (!this._mounted) return;
    const previousGeometrySelection = this.geometryEditor?.selectedIds;
    this.geometryEditor?.dispose();
    this.geometryEditor = null;
    const connection = {
      ready: { className: 'is-ready', label: 'READY' },
      degraded: { className: 'is-degraded', label: 'DEGRADED' },
      offline: { className: 'is-offline', label: 'OFFLINE' },
      loading: { className: 'is-loading', label: 'LOAD …' },
    }[this.connectionState] || { className: 'is-loading', label: 'LOAD …' };
    const experimentActionsDisabled = this.busy || this.connectionState !== 'ready';
    this.container.innerHTML = `
      <section class="occ" aria-labelledby="occTitle">
        <header class="occ-header">
          <div class="occ-header-copy">
            <div class="occ-eyebrow">OBSERVATORY / WIFI + REFERENZ ${infoTip('Getrennte Referenz', 'mmWave liefert Ground Truth für Kalibrierung und Blindtest. Es ist kein WiFi-Feature.')}</div>
            <h3 id="occTitle">Experiment-Cockpit</h3>
            <p>Setup, Runs und Auswertung an einem Ort.</p>
          </div>
          <div class="occ-header-actions">
            <span class="occ-status ${connection.className}">${connection.label}</span>
            <button type="button" class="occ-button occ-button-quiet" data-occ-action="refresh">Aktualisieren</button>
          </div>
        </header>

        <div class="occ-overview">${this._overviewMarkup()}</div>
        ${this.error ? `<div class="occ-message is-error" role="alert">${escapeHTML(this.error)}</div>` : ''}
        ${this.message ? `<div class="occ-message" role="status">${escapeHTML(this.message)}</div>` : ''}

        <section class="occ-panel occ-wide-panel occ-room-panel">
          ${panelHeading('SETUP', 'Raum-Setup', 'Setup-Profil', 'Speichert Raum, TX, RX und mmWave-Montagepunkt als versioniertes Profil. P01–P09 bleibt Legacy.')}
          <p class="occ-panel-intro">Maße in m · Achsen: <strong>x / y / z</strong>.</p>
          <div id="occRoomCadEditor" class="occ-cad-editor"></div>
          <form id="occProfileForm">
            <label class="occ-field occ-profile-name"><span>Profilname</span><input name="profile_label" maxlength="120" value="${attribute(this.profileLabel)}" required></label>
            <div class="occ-room-form-grid">
              <div class="occ-room-form-column">
              <div class="occ-form-section">
                <div class="occ-subheading">Raum [L / H / B] (m) ${infoTip('Raum', 'Reihenfolge: Länge, Höhe, Breite.')}</div>
                <div class="occ-triple">${this._tripleInputs('room_dimensions_m', this.profileDraft.room_dimensions_m)}</div>
              </div>
              <div class="occ-form-section">
                <div class="occ-subheading">TX [x / y / z] (m) ${infoTip('TX', 'Position der WiFi-Sendequelle.')}</div>
                <div class="occ-triple">${this._tripleInputs('transmitter.position_m', this.profileDraft.transmitter?.position_m)}</div>
              </div>
              <div class="occ-form-section">
                <div class="occ-subheading">RX [x / y / z] (m) ${infoTip('RX', 'Positionen der vier Empfänger.')}</div>
                <div class="occ-node-editor">${(this.profileDraft.receivers || []).map((receiver) => `
                  <div class="occ-node-row"><strong>${escapeHTML(receiver.id)}</strong><div class="occ-triple">${this._tripleInputs(`receiver.${receiver.id}`, receiver.position_m)}</div></div>
                `).join('')}</div>
              </div>
              <div class="occ-form-section">
                <div class="occ-subheading">mmWave [x / y / z] (m) ${infoTip('mmWave', 'Fester Montagepunkt des HLK-LD2450. Radar-Truth bleibt getrennt vom WiFi-Modell.')}</div>
                <div class="occ-triple">${this._tripleInputs('mmwave.mounting_position_m', mmwaveMountingPosition(this.profileDraft))}</div>
                <p class="occ-helper">Montagepunkt, nicht die aktuelle Zielposition.</p>
              </div>
              </div>
              <div class="occ-room-form-column">
                <div class="occ-calibration-route">
                  <div class="occ-route-kicker">REFERENZ</div>
                  <div class="occ-route-status ${this.status?.mmwave?.packets_received > 0 ? 'is-ready' : 'is-waiting'}"><span></span>${this.status?.mmwave?.packets_received > 0 ? 'mmWave verbunden' : 'Wartet auf mmWave'}</div>
                  <h5>Radar-Referenz</h5>
                  <p>Radar liefert x/z zu jedem CSI-Fenster. Kein P01–P09-Raster nötig.</p>
                  <button type="button" class="occ-button occ-button-primary" data-occ-action="open-mmwave-calibration">mmWave öffnen</button>
                  <p class="occ-helper">Ground Truth bleibt getrennt.</p>
                </div>
                <details class="occ-fold occ-coordinate-fallback" ${this.pointGridExpanded ? 'open' : ''}>
                  <summary data-occ-action="toggle-point-grid"><span>P01–P09-Raster</span><small>Legacy</small></summary>
                  <p class="occ-helper">Nur für alte Runs und Kontrolltests.</p>
                  <div class="occ-point-editor">${(this.profileDraft.points || []).map((point) => `
                    <div class="occ-point-row"><strong>${escapeHTML(point.id)}</strong><div class="occ-triple">${this._tripleInputs(`point.${point.id}`, point.coordinates_m)}</div></div>
                  `).join('')}</div>
                  <button type="button" class="occ-button occ-button-quiet" data-occ-action="generate-points">3×3-Raster erzeugen</button>
                </details>
              </div>
            </div>
            <div class="occ-inline-actions">
              <button type="button" class="occ-button occ-button-primary occ-mmwave-placement-button" data-occ-action="calculate-mmwave-placement">mmWave-Position berechnen</button>
              <button type="submit" class="occ-button occ-button-primary" ${experimentActionsDisabled ? 'disabled' : ''}>${this.selectedProfile ? 'Neue Profilversion speichern' : 'Setup-Profil speichern'}</button>
              ${this._setupV2DraftActionMarkup()}
            </div>
            <p class="occ-helper" data-occ-mmwave-placement-status>Berechnet die mmWave-Montage aus Raum-, TX- und RX-Geometrie und übernimmt sie direkt in die Felder und den CAD-Plan. Danach Profilversion speichern.</p>
          </form>
          <details class="occ-fold" ${this.profilesExpanded ? 'open' : ''}>
            <summary data-occ-action="toggle-profiles"><span>Profile</span><small>${this.profiles.length ? `${this.profiles.length} Versionen` : 'leer'}</small></summary>
            <div class="occ-profile-list">
              ${this.profiles.length ? this.profiles.map((profile) => `
                <button type="button" class="occ-profile-row ${profile.id === this.selectedProfile?.id ? 'is-selected' : ''}" data-occ-profile-id="${attribute(profile.id)}">
                  <span><strong>${escapeHTML(profile.label)}</strong><small>v${profile.version} · ${escapeHTML(profile.profile_sha256.slice(0, 12))}…</small></span>
                  <small>${escapeHTML(formatTime(profile.updated_at))}</small>
                </button>
              `).join('') : '<div class="occ-empty">Noch kein Profil.</div>'}
            </div>
          </details>
        </section>

        <section class="occ-panel occ-wide-panel occ-workflow-panel">
          ${panelHeading('WORKFLOW', 'WiFi-Workflow', 'Workflow', 'Run, Hash, CSI und Referenz bleiben gebunden.')}
          ${this._workflowMarkup()}
          ${this._runHistoryMarkup()}
        </section>

        <section class="occ-panel occ-wide-panel">
          <div class="occ-section-header">${panelHeading('NODES', 'TX / RX', 'Node-Status', 'Nur Status: Erreichbarkeit, Rate, Lücken und Sync.')}<span class="occ-note">Nur Status · keine Firmware-Aktion</span></div>
          ${this._nodesMarkup()}
        </section>

        <div class="occ-grid occ-lower-grid">
          <section class="occ-panel">${panelHeading('CAPTURES', 'Aufnahmen', 'Aufnahmen', 'Gefundene CSI-Aufnahmen. Rohdaten und Truth bleiben getrennt.')} ${this._recordingsMarkup()}</section>
          <section class="occ-panel">${panelHeading('BENCHMARK', 'Vergleich', 'Benchmark', 'Werte erst nach neuen gelabelten Captures und Blindtest.')} ${this._benchmarkMarkup()}</section>
        </div>
      </section>
    `;
    const editorHost = typeof this.container.querySelector === 'function'
      ? this.container.querySelector('#occRoomCadEditor')
      : null;
    if (editorHost) {
      this.geometryEditor = new RoomGeometryEditor(editorHost, {
        document: this.profileDraft,
        selectedIds: previousGeometrySelection,
        onChange: (document) => {
          this.profileDraft = document;
          this._syncProfileFormFromDraft();
        },
        onSave: (document) => this._saveGeometryDocument(document),
        saveDisabled: experimentActionsDisabled,
      });
      this.geometryEditor.mount();
    }
  }

  _overviewMarkup() {
    const nodes = Array.isArray(this.status?.nodes) ? this.status.nodes : [];
    const active = nodes.filter((node) => node.status === 'active').length;
    const txAttested = nodes.filter((node) => node.source_binding_attested === true).length;
    const current = this.selectedRun?.workflow;
    return [
      ['Aktive RX', `${active}/${nodes.length || 4}`, active === 4 ? 'is-good' : 'is-warn', 'Aktive RX', 'Online-Zahl im letzten Status.'],
      ['TX', txAttested ? `${txAttested} RX` : 'unbekannt', txAttested ? 'is-good' : 'is-warn', 'TX-Bindung', 'Bestätigte TX-Quelle.'],
      ['Präsenz', this.status?.classification_calibration?.phase || '--', this.status?.classification_calibration?.phase === 'ready' ? 'is-good' : 'is-warn', 'Präsenz', 'WiFi-Präsenzstatus, unabhängig von mmWave.'],
      ['Run', current ? phaseLabel(current.current_phase) : 'kein Run', current?.current_status === 'PASS' ? 'is-good' : '', 'Workflow', 'Aktiver Run-Schritt.'],
      ['Radar', this.status?.mmwave?.packets_received > 0 ? 'verbunden' : 'nicht verbunden', 'is-locked', 'Radar', 'Ground Truth für Kalibrierung und Blindtest; kein WiFi-Feature.'],
      ['Runs', String(this.runs.length), '', 'Runs', 'Gespeicherte Experimentläufe.'],
    ].map(([label, value, cls, infoLabel, infoText]) => `<div class="occ-metric ${cls}"><span class="occ-metric-label">${escapeHTML(label)}${infoTip(infoLabel, infoText)}</span><strong>${escapeHTML(value)}</strong></div>`).join('');
  }

  _tripleInputs(prefix, values = []) {
    return [0, 1, 2].map((index) => `<input aria-label="${attribute(prefix)} ${index + 1}" data-occ-field="${attribute(prefix)}.${index}" type="number" step="0.01" value="${attribute(values?.[index] ?? 0)}" required>`).join('');
  }

  _syncProfileFormFromDraft() {
    const form = this.container?.querySelector('#occProfileForm');
    if (!form) return;
    const set = (prefix, values) => {
      [0, 1, 2].forEach((index) => {
        const input = form.querySelector(`[data-occ-field="${prefix}.${index}"]`);
        if (input) input.value = values?.[index] ?? 0;
      });
    };
    set('room_dimensions_m', this.profileDraft.room_dimensions_m);
    set('transmitter.position_m', this.profileDraft.transmitter?.position_m);
    (this.profileDraft.receivers || []).forEach((receiver) => set(`receiver.${receiver.id}`, receiver.position_m));
    set('mmwave.mounting_position_m', mmwaveMountingPosition(this.profileDraft));
    (this.profileDraft.points || []).forEach((point) => set(`point.${point.id}`, point.coordinates_m));
  }

  _saveGeometryDocument(document) {
    this.profileDraft = document;
    this._syncProfileFormFromDraft();
    const form = this.container?.querySelector('#occProfileForm');
    if (!form) return;
    if (typeof form.checkValidity === 'function' && !form.checkValidity()) {
      form.reportValidity?.();
      return;
    }
    void this._saveProfile(form);
  }

  _guideState() {
    if (!this.selectedRun) {
      if (!this.selectedProfile) {
        return {
          progress: 'VORBEREITUNG',
          step: 'SCHRITT 01',
          state: 'SETUP FEHLT',
          tone: 'is-waiting',
          title: 'Setup-Profil speichern',
          body: 'Raum sowie TX/RX festlegen und speichern.',
          checklist: ['Maße prüfen', 'TX/RX setzen', 'Profil speichern'],
          action: 'focus-profile',
          actionLabel: 'Zum Setup-Profil',
          navigationOnly: true,
          helper: 'Keine automatische Hardwaremessung.',
        };
      }
      return {
        progress: 'VORBEREITUNG',
        step: 'SCHRITT 02',
        state: 'BEREIT FÜR RUN',
        tone: 'is-ready',
        title: 'Experiment-Run anlegen',
        body: 'Der Run bindet Profil, Dataset und Referenz.',
        checklist: ['Profil wählen', 'Name setzen', 'Run anlegen'],
        action: 'focus-workflow',
        actionLabel: 'Zum Run-Formular',
        navigationOnly: true,
        helper: 'Danach führt der Guide weiter.',
      };
    }

    const workflow = this.selectedRun.workflow || {};
    const phase = workflow.current_phase;
    const status = workflow.current_status;
    const softwareOnly = (workflow.events || []).some((event) => event.payload?.software_only === true || event.payload?.demo === 'guide walkthrough only');
    const phaseIndex = WORKFLOW_PHASES.indexOf(phase);
    const progress = `PHASE ${String(phaseIndex + 1).padStart(2, '0')} / ${WORKFLOW_PHASES.length}`;
    const done = status === 'PASS' || status === 'REUSED';
    const states = {
      RUNNING: 'PHASE OFFEN',
      READY: 'BEREIT',
      PASS: 'ABGESCHLOSSEN',
      REUSED: 'WIEDERVERWENDET',
    };
    const guide = {
      progress,
      step: phaseLabel(phase),
      state: states[status] || status || 'WARTET',
      tone: done ? 'is-complete' : status === 'RUNNING' ? 'is-running' : 'is-ready',
      checklist: [],
      helper: '',
      navigationOnly: false,
    };

    if (phase !== 'create_experiment' && !softwareOnly && !this._workflowRuntimeSeal()) {
      return {
        ...guide,
        state: 'GESPERRT',
        tone: 'is-waiting',
        title: 'Run besitzt kein gültiges Runtime-Seal',
        body: 'Dieser ältere oder abweichende Run darf nicht fortgesetzt werden. Lege mit dem aktiven Setup-v2 einen neuen Run an.',
        checklist: ['Alt-Run nicht weiterverwenden', 'Run-Auswahl lösen', 'Neuen Run versiegeln'],
        action: 'clear-run',
        actionLabel: 'Neuen Run anlegen',
        helper: 'Bestehende Daten bleiben unverändert erhalten.',
        navigationOnly: true,
      };
    }

    if (phase === 'create_experiment') {
      const setup = this._activePositionSetup();
      if (!setup) {
        guide.state = 'GESPERRT';
        guide.tone = 'is-waiting';
        guide.title = 'Runtime-Setup fehlt';
        guide.body = 'Dieser Run kann erst versiegelt werden, wenn der Server mit dem gültigen Setup-v2 gestartet wurde.';
        guide.checklist = ['Setup-v2 erzeugen', 'Server mit --position-setup starten', 'Profilabgleich bestehen'];
        guide.action = null;
        guide.helper = 'Ein Profil-Hash allein ist kein Runtime-Seal.';
      } else {
        guide.title = 'Run mit Runtime-Setup versiegeln';
        guide.body = 'Der Server prüft Profilgeometrie und Deployment-Metadaten gegen das aktive Setup-v2.';
        guide.checklist = ['Profil-Hash kontrollieren', `Runtime ${setup.setup_id}`, 'Setup versiegeln'];
        guide.action = 'seal';
        guide.actionLabel = 'Setup versiegeln';
        guide.helper = 'Setup-ID und Setup-Hash werden serverseitig im Run gespeichert.';
      }
    } else if (phase === 'seal_setup') {
      const plan = this.emptyCalibrationPlan;
      const remaining = this._emptyCalibrationRemainingSeconds();
      if (!plan) {
        if (this.calibrationAvailability?.available === true) {
          guide.title = 'Gespeicherte Leerkalibrierung verwenden';
          guide.body = 'Für dieses unveränderte Setup ist bereits eine passende Baseline vorhanden. Du kannst ohne neue Leeraumkalibrierung fortfahren.';
          guide.checklist = ['Profil- und Setup-Kontext bestätigt', 'Gespeicherte Baseline verwenden', 'mmWave-Schritt öffnen'];
          guide.action = 'reuse-empty';
          guide.actionLabel = 'Gespeicherte Baseline verwenden';
          guide.helper = 'Eine neue Messung bleibt als Alternative im Workflow verfügbar.';
        } else {
          guide.title = 'Leerkalibrierung vorbereiten';
          guide.body = 'Lege Messdauer und Vorlauf fest. Die Messung startet erst nach dem Countdown.';
          guide.checklist = ['Messdauer mindestens 60 s', 'Vorlauf festlegen', 'Raum vor Start verlassen'];
          guide.action = 'prepare-empty';
          guide.actionLabel = 'Leerkalibrierung vorbereiten';
          guide.helper = 'Noch keine Datenaufnahme. Die Baseline bleibt unabhängig von der späteren mmWave-Positionsreferenz.';
        }
      } else if (plan.phase === 'form') {
        guide.title = 'Leerkalibrierung konfigurieren';
        guide.body = 'Wähle die Messdauer und wie viele Sekunden bis zum Start verbleiben sollen.';
        guide.checklist = ['Messdauer mindestens 60 s', 'Countdown starten', 'Raum verlassen'];
        guide.action = 'focus-empty-preparation';
        guide.actionLabel = 'Dauer und Vorlauf einstellen';
        guide.navigationOnly = true;
        guide.helper = 'Die Messung beginnt erst nach dem Countdown.';
      } else if (plan.phase === 'countdown') {
        guide.title = 'Raum verlassen';
        guide.body = `Die Leerkalibrierung startet in ${remaining} s. Sicher zurück ab ${formatClockTime(this._emptyCalibrationSafeReturnAt())} Uhr.`;
        guide.checklist = ['Raum verlassen', 'Tür schließen', 'Nicht zurückkehren'];
        guide.action = 'cancel-empty-preparation';
        guide.actionLabel = 'Countdown abbrechen';
        guide.helper = 'Es werden noch keine CSI-Daten gesammelt.';
      } else if (plan.phase === 'starting') {
        guide.title = 'Leerkalibrierung startet';
        guide.body = 'Der Server wird jetzt für die leere Baseline aktiviert.';
        guide.checklist = ['Raum leer halten', 'Messung abwarten', 'Nicht zurückkehren'];
        guide.action = null;
        guide.helper = 'Start wird bestätigt …';
      } else if (plan.phase === 'collecting') {
        guide.title = 'Leerkalibrierung läuft';
        guide.body = `Noch ${remaining} s. Sicher zurück ab ${formatClockTime(this._emptyCalibrationSafeReturnAt())} Uhr. Der Raum muss vollständig leer bleiben.`;
        guide.checklist = ['Raum leer halten', 'Nicht bewegen', 'Automatischen Abschluss abwarten'];
        guide.action = null;
        guide.helper = `Automatischer Abschluss nach ${plan.durationSeconds} s.`;
      } else if (plan.phase === 'stopping') {
        guide.title = 'Leerkalibrierung wird abgeschlossen';
        guide.body = 'Die Baseline wird jetzt geprüft und gespeichert.';
        guide.checklist = ['Raum leer halten', 'Baseline prüfen', 'mmWave-Schritt abwarten'];
        guide.action = null;
        guide.helper = 'Bitte noch nicht in den Raum zurückkehren.';
      } else {
        guide.title = 'Leerkalibrierung abschließen';
        guide.body = 'Der automatische Abschluss konnte noch nicht bestätigt werden.';
        guide.checklist = ['Raum leer halten', 'Abschluss erneut senden', 'Ergebnis prüfen'];
        guide.action = 'stop-empty';
        guide.actionLabel = 'Leerkalibrierung abschließen';
        guide.helper = 'Die Mindestdauer wurde bereits abgewartet.';
      }
    } else if (phase === 'empty_calibration') {
      const plan = this.emptyCalibrationPlan;
      const remaining = this._emptyCalibrationRemainingSeconds();
      if (status === 'PASS' || status === 'REUSED') {
        guide.title = 'mmWave-Kalibrierung vorbereiten';
        guide.body = status === 'REUSED'
          ? 'Die gespeicherte leere WiFi-Baseline ist wiederverwendet. Öffne jetzt den mmWave-geführten Kalibrierungsweg.'
          : 'Die leere WiFi-Baseline ist abgeschlossen. Öffne jetzt den mmWave-geführten Kalibrierungsweg.';
        guide.checklist = ['Baseline abgeschlossen', 'mmWave-Assistent öffnen', 'Radarreferenz prüfen'];
        guide.action = 'open-training';
        guide.actionLabel = 'mmWave-Kalibrierung öffnen';
        guide.helper = 'Die Baseline ist unabhängig von der späteren mmWave-Positionsreferenz.';
      } else if (plan?.phase === 'collecting') {
        guide.title = 'Leerkalibrierung läuft';
        guide.body = `Noch ${remaining} s. Sicher zurück ab ${formatClockTime(this._emptyCalibrationSafeReturnAt())} Uhr. Der Raum muss vollständig leer bleiben.`;
        guide.checklist = ['Raum leer halten', 'Nicht bewegen', 'Automatischen Abschluss abwarten'];
        guide.action = null;
        guide.helper = `Automatischer Abschluss nach ${plan.durationSeconds} s.`;
      } else if (plan?.phase === 'stopping') {
        guide.title = 'Leerkalibrierung wird abgeschlossen';
        guide.body = 'Die Baseline wird jetzt geprüft und gespeichert.';
        guide.checklist = ['Raum leer halten', 'Baseline prüfen', 'Nächsten Schritt abwarten'];
        guide.action = null;
        guide.helper = 'Bitte noch nicht in den Raum zurückkehren.';
      } else if (plan?.phase === 'ready_to_finish') {
        guide.title = 'Leerkalibrierung abschließen';
        guide.body = 'Die Mindestdauer ist erreicht. Der Server wartet noch auf die Abschlussbestätigung.';
        guide.checklist = ['Raum leer halten', 'Abschluss senden', 'Ergebnis prüfen'];
        guide.action = 'stop-empty';
        guide.actionLabel = 'Leerkalibrierung abschließen';
        guide.helper = 'Die Messdauer wurde bereits abgewartet.';
      } else {
        guide.title = status === 'RUNNING' ? 'Leerkalibrierung läuft lassen' : 'Leerkalibrierung erneut vorbereiten';
        guide.body = status === 'RUNNING'
          ? 'Warte, bis genügend leere CSI-Fingerprints gesammelt wurden. Bewege dich währenddessen nicht im Raum.'
          : 'Bereite eine neue leere WiFi-Baseline mit Vorlauf und fester Messdauer vor.';
        guide.checklist = status === 'RUNNING'
          ? ['Raum leer halten', 'CSI-Pakete prüfen', 'Aufnahme abschließen']
          : ['Vorbereitung öffnen', 'Messdauer festlegen', 'Raum verlassen'];
        guide.action = status === 'RUNNING' ? 'stop-empty' : 'prepare-empty';
        guide.actionLabel = status === 'RUNNING' ? 'Leerkalibrierung abschließen' : 'Leerkalibrierung vorbereiten';
        guide.helper = 'Bei zu wenigen RX-Fingerprints bleibt der Schritt absichtlich gesperrt.';
      }
    } else if (phase === 'train_p01_p09') {
      guide.title = done ? 'Kalibrierung abgeschlossen' : 'CSI mit mmWave-Referenz kalibrieren';
      guide.body = done
        ? 'Die Positionsreferenz ist abgeschlossen. Bereite als Nächstes den getrennten Blindtest vor.'
        : 'Starte den mmWave-Assistenten. Er verknüpft CSI-Zeitfenster mit Radarpositionen; P01–P09 müssen nicht manuell aufgenommen werden.';
      guide.checklist = done
        ? ['Kalibrierung prüfen', 'Blindtest-Reihenfolge öffnen', 'Blindtest starten']
        : ['Radar-Status prüfen', 'Raum abdecken', 'CSI/Radar-Zeitbezug prüfen'];
      guide.action = done ? 'open-randomize' : 'open-mmwave-calibration';
      guide.actionLabel = done ? 'Blindtest vorbereiten' : 'mmWave-Assistent öffnen';
      guide.navigationOnly = !done;
      guide.helper = 'mmWave bleibt unabhängige Ground Truth und wird nicht in den WiFi-Prädiktor eingespeist.';
    } else if (phase === 'randomize_blind_positions') {
      guide.title = done ? 'Blindaufnahmen öffnen' : 'Blindtest-Reihenfolge erzeugen';
      guide.body = done
        ? 'Die Reihenfolge ist versiegelt. Öffne jetzt die Blindaufnahmen.'
        : 'Erzeuge eine reproduzierbare Reihenfolge. Die Wahrheit bleibt bis nach der Prediction getrennt.';
      guide.checklist = done ? ['Seed gespeichert', 'Blindaufnahmen öffnen'] : ['Seed festlegen', 'Reihenfolge erzeugen', 'Wahrheit verborgen halten'];
      guide.action = done ? 'open-capture' : 'randomize';
      guide.actionLabel = done ? 'Blindaufnahmen öffnen' : 'Blind-Reihenfolge erzeugen';
      guide.helper = 'Die Blind-Reihenfolge ist nur für den Versuchsablauf sichtbar.';
    } else if (phase === 'capture') {
      const blindCaptures = phaseEvents(this.selectedRun, 'capture').filter((event) => event.payload?.capture_kind === 'blind' && event.payload?.capture_completed === true);
      const complete = blindCaptures.length >= 18;
      guide.title = done ? 'Prediction vorbereiten' : complete ? 'Blindaufnahmen abschließen' : 'Blindaufnahmen durchführen';
      guide.body = done
        ? 'Alle Blindaufnahmen sind abgeschlossen. Öffne den Prediction-Schritt.'
        : complete
          ? 'Die vorgesehenen Blindaufnahmen liegen vor. Schließe die Phase ab, bevor du Prediction registrierst.'
          : `Nimm die nächste Blindaufnahme auf. Fortschritt: ${blindCaptures.length}/18; die Ground Truth bleibt verborgen.`;
      guide.checklist = done ? ['Captures abgeschlossen', 'Prediction-Artefakt vorbereiten'] : ['Anweisung befolgen', 'CSI aufnehmen', 'Erst danach Truth aufdecken'];
      guide.action = done ? 'open-predict' : complete ? 'finish-blind' : 'record-blind';
      guide.actionLabel = done ? 'Prediction öffnen' : complete ? 'Blindaufnahmen abschließen' : 'Nächste Blindaufnahme starten';
      guide.helper = 'Die angezeigte Position darf nicht in Capture-Metadaten oder Prediction-Eingang gelangen.';
    } else if (phase === 'predict') {
      const hasArtifact = Boolean(this.artifactPaths.prediction);
      guide.title = status === 'READY' ? 'Truth-Phase öffnen' : 'Prediction-Artefakt registrieren';
      guide.body = status === 'READY'
        ? softwareOnly
          ? 'Der Demo-Run markiert Prediction softwareseitig als abgeschlossen. Im echten Run muss hier erst eine echte Prediction-Datei registriert werden.'
          : 'Die Prediction ist registriert. Öffne jetzt die Truth-Phase, ohne die Radarwahrheit rückwirkend einzuspeisen.'
        : 'Trage den relativen Pfad der Prediction-Datei ein und registriere sie. Der Server bindet den SHA-256-Hash an den Run.';
      guide.checklist = status === 'READY' ? ['Prediction-Hash gespeichert', 'Truth-Phase öffnen'] : ['Prediction-Datei erzeugen', 'Pfad eintragen', 'Artefakt registrieren'];
      guide.action = status === 'READY' ? 'advance-reveal' : hasArtifact ? 'register-prediction' : 'focus-artifact-prediction';
      guide.actionLabel = status === 'READY' ? 'Truth-Phase öffnen' : hasArtifact ? 'Prediction registrieren' : 'Zum Prediction-Pfad';
      guide.navigationOnly = !hasArtifact && status !== 'READY';
      guide.helper = 'Prediction und Truth bleiben bis zur Auswertung strikt getrennt.';
    } else if (phase === 'reveal_truth') {
      const hasArtifact = Boolean(this.artifactPaths.truth);
      guide.title = status === 'READY' ? 'Evaluation öffnen' : 'Truth-Artefakt registrieren';
      guide.body = status === 'READY'
        ? softwareOnly
          ? 'Der Demo-Run markiert die Truth-Phase softwareseitig als abgeschlossen. Im echten Run wird hier die getrennt gespeicherte Radarwahrheit registriert.'
          : 'Die Radar-/Positionswahrheit ist registriert. Öffne jetzt die Evaluation.'
        : 'Trage erst jetzt die getrennte Truth-Datei ein. Vorher darf sie nicht für die Prediction verwendet werden.';
      guide.checklist = status === 'READY' ? ['Truth-Hash gespeichert', 'Evaluation öffnen'] : ['Truth-Datei prüfen', 'Pfad eintragen', 'Artefakt registrieren'];
      guide.action = status === 'READY' ? 'advance-evaluate' : hasArtifact ? 'register-truth' : 'focus-artifact-truth';
      guide.actionLabel = status === 'READY' ? 'Evaluation öffnen' : hasArtifact ? 'Truth registrieren' : 'Zum Truth-Pfad';
      guide.navigationOnly = !hasArtifact && status !== 'READY';
      guide.helper = 'Die Truth-Datei wird erst in dieser Phase sichtbar gemacht.';
    } else if (phase === 'evaluate') {
      const hasArtifact = Boolean(this.artifactPaths.evaluation);
      guide.title = status === 'READY' ? 'Ergebnisbericht öffnen' : 'Evaluation registrieren';
      guide.body = status === 'READY'
        ? softwareOnly
          ? 'Die Demo-Evaluation ist nur softwareseitig markiert. Der echte Report darf erst nach Prediction, Truth und belastbaren Kennzahlen erzeugt werden.'
          : 'Die Evaluation ist abgeschlossen. Öffne den Report für Accuracy, Coverage, Fehlerdistanz und Gate-Ergebnisse.'
        : 'Trage die erzeugte Evaluation-Datei ein und registriere sie am Run.';
      guide.checklist = status === 'READY' ? ['Kennzahlen geprüft', 'Report öffnen'] : ['Evaluation ausführen', 'Pfad eintragen', 'Artefakt registrieren'];
      guide.action = status === 'READY' ? 'advance-report' : hasArtifact ? 'register-evaluation' : 'focus-artifact-evaluation';
      guide.actionLabel = status === 'READY' ? 'Report öffnen' : hasArtifact ? 'Evaluation registrieren' : 'Zum Evaluation-Pfad';
      guide.navigationOnly = !hasArtifact && status !== 'READY';
      guide.helper = 'Die Offline-Gates bleiben die Quelle für die Aussagekraft der Kennzahlen.';
    } else if (phase === 'report') {
      guide.title = softwareOnly && status === 'PASS' ? 'Software-only Report-Demo' : status === 'PASS' ? 'Ergebnisbericht schreiben' : 'Report wartet auf Vorbedingungen';
      guide.body = status === 'PASS'
        ? softwareOnly
          ? 'Der Ablauf ist als Software-Demo vollständig durchlaufen. Ein erzeugter Report bleibt UNVALIDATED und enthält keine echte Hardware- oder Modellqualität.'
          : 'Alle Artefakte sind vorhanden. Schreibe jetzt den versionierten Ergebnisbericht.'
        : 'Der Report wird erst freigeschaltet, wenn Prediction, Truth und Evaluation registriert sind.';
      guide.checklist = ['Prediction vorhanden', 'Truth vorhanden', 'Evaluation vorhanden'];
      guide.action = status === 'PASS' ? 'write-report' : null;
      guide.actionLabel = 'Report schreiben';
      guide.helper = 'Der Report bleibt an Setup-Hash und Run-ID gebunden.';
    }
    if (softwareOnly) {
      guide.tone = 'is-demo';
      guide.state = `${guide.state} · DEMO`;
      guide.helper = `SOFTWARE-ONLY / UNVALIDATED · ${guide.helper}`;
    }
    return guide;
  }

  _guideMarkup() {
    const guide = this._guideState();
    const actionDisabled = !guide.action || this.busy || (this.connectionState !== 'ready' && !guide.navigationOnly);
    return `
      <section class="occ-guide ${guide.tone}" aria-labelledby="occGuideTitle" aria-live="polite">
        <div class="occ-guide-rail">
          <span class="occ-guide-kicker">GUIDE</span>
          <strong>${escapeHTML(guide.progress)}</strong>
          <small>${escapeHTML(guide.step)}</small>
        </div>
        <div class="occ-guide-content">
          <div class="occ-guide-header"><span class="occ-guide-label">NÄCHSTE AKTION</span><span class="occ-guide-state">${escapeHTML(guide.state)}</span></div>
          <h5 id="occGuideTitle">${escapeHTML(guide.title)}</h5>
          <p>${escapeHTML(guide.body)}</p>
          <ul class="occ-guide-checklist">${guide.checklist.map((item) => `<li>${escapeHTML(item)}</li>`).join('')}</ul>
          <div class="occ-guide-action-row">
            ${guide.action ? `<button type="button" class="occ-button occ-button-primary" data-occ-action="${attribute(guide.action)}" ${actionDisabled ? 'disabled' : ''}>${escapeHTML(guide.actionLabel)}</button>` : '<span class="occ-lock-note">Noch keine Aktion verfügbar.</span>'}
            <span class="occ-guide-helper">${escapeHTML(guide.helper)}</span>
          </div>
        </div>
      </section>
    `;
  }

  _workflowMarkup() {
    if (!this.selectedRun) {
      return `
        ${this._guideMarkup()}
        <p class="occ-copy">Ein Run bindet Profil, Dataset und Blindtest-Seed.</p>
        <form id="occWorkflowForm">
          <label class="occ-field"><span>Versuchsname</span><input name="workflow_label" maxlength="120" value="${attribute(this.workflowLabel)}" required></label>
          <label class="occ-field"><span>Profil ${infoTip('Profil', 'Der Run übernimmt den Hash dieser Version.')}</span><select name="profile_id" ${this.profiles.length ? '' : 'disabled'}>${this.profiles.map((profile) => `<option value="${attribute(profile.id)}" ${profile.id === this.selectedProfile?.id ? 'selected' : ''}>${escapeHTML(profile.label)} · ${escapeHTML(profile.profile_sha256.slice(0, 12))}…</option>`).join('')}</select></label>
          <div class="occ-inline-actions"><button type="submit" class="occ-button occ-button-primary" ${this.profiles.length && !this.busy && this.connectionState === 'ready' ? '' : 'disabled'}>Run anlegen</button></div>
        </form>
        <p class="occ-helper">${this.profiles.length ? 'Nächster Schritt: Setup versiegeln.' : 'Zuerst Profil speichern.'}</p>
      `;
    }
    const workflow = this.selectedRun.workflow;
    const trainingCaptures = phaseEvents(this.selectedRun, 'train_p01_p09').filter((event) => event.payload?.capture_kind === 'training' && event.payload?.capture_completed === true);
    const blindCaptures = phaseEvents(this.selectedRun, 'capture').filter((event) => event.payload?.capture_kind === 'blind' && event.payload?.capture_completed === true);
    const currentPoint = this.trainingPoint;
    const completedTrainingPoints = new Set(trainingCaptures.map((event) => event.payload?.point_id).filter(Boolean));
    return `
      ${this._guideMarkup()}
      <div class="occ-run-head"><strong>${escapeHTML(this.selectedRun.label)}</strong><span class="occ-run-badge">${escapeHTML(this.selectedRun.execution_status)}</span></div>
      <div class="occ-run-meta"><span><strong>Run-ID</strong> ${escapeHTML(this.selectedRun.id)}</span><span><strong>Profil-Hash</strong> ${escapeHTML(workflow?.profile_sha256?.slice(0, 12) || '--')}…</span><span><strong>Blind-Seed</strong> ${escapeHTML(workflow?.blind_seed)}</span></div>
      <div class="occ-flow-heading"><span>Workflow-Phasen</span>${infoTip('Workflow-Phasen', 'Die Reihenfolge schützt die Trennung von Training, Prediction und Truth. Ein Schritt wird erst nach seiner Vorbedingung freigeschaltet.')}</div>
      <ol class="occ-phase-list">${WORKFLOW_PHASES.map((phase, index) => {
        const currentIndex = WORKFLOW_PHASES.indexOf(workflow?.current_phase);
        const done = index < currentIndex || (index === currentIndex && ['PASS', 'REUSED'].includes(workflow?.current_status));
        const current = index === currentIndex;
        return `<li class="${done ? 'is-done' : ''} ${current ? 'is-current' : ''}"><span>${String(index + 1).padStart(2, '0')}</span><strong>${escapeHTML(phaseLabel(phase))}</strong><small>${current ? escapeHTML(workflow.current_status) : done ? 'PASS' : 'gesperrt'}</small></li>`;
      }).join('')}</ol>
      <fieldset class="occ-action-fieldset" ${this.connectionState === 'ready' ? '' : 'disabled'}><div class="occ-workflow-actions">${this._workflowActions(workflow, completedTrainingPoints.size, blindCaptures.length, currentPoint, completedTrainingPoints)}</div></fieldset>
      <div class="occ-capture-counts"><span>Manueller Fallback <strong>${completedTrainingPoints.size}/9</strong></span><span>Legacy-Blindaufnahmen <strong>${blindCaptures.length}/18</strong></span></div>
      <button type="button" class="occ-button occ-button-quiet" data-occ-action="clear-run">Run-Auswahl lösen</button>
    `;
  }

  _runHistoryMarkup() {
    if (!this.runs.length) return '';
    return `
      <details class="occ-fold occ-run-history" ${this.runsExpanded ? 'open' : ''}>
        <summary data-occ-action="toggle-runs"><span>Versuchsverlauf</span><small>${this.runs.length} Runs</small></summary>
        <div class="occ-profile-list">
          ${this.runs.slice(0, 12).map((run) => `
            <button type="button" class="occ-profile-row occ-run-row ${run.id === this.selectedRun?.id ? 'is-selected' : ''}" data-occ-run-id="${attribute(run.id)}">
              <span><strong>${escapeHTML(run.label)}</strong><small>${escapeHTML(phaseLabel(run.workflow?.current_phase || run.phase))} · ${escapeHTML(run.workflow?.current_status || run.execution_status)}</small></span>
              <small>${escapeHTML(formatTime(run.created_at))}</small>
            </button>
          `).join('')}
        </div>
      </details>
    `;
  }

  _workflowActions(workflow, trainingCount, blindCount, currentPoint, completedTrainingPoints = new Set()) {
    if (!workflow) return '';
    const softwareOnly = (workflow.events || []).some((event) => (
      event.payload?.software_only === true || event.payload?.demo === 'guide walkthrough only'
    ));
    if (workflow.current_phase !== 'create_experiment'
      && !softwareOnly
      && this.selectedRun?.workflow === workflow
      && !this._workflowRuntimeSeal()) {
      return '<button type="button" class="occ-button occ-button-primary" disabled>Run ohne gültiges Runtime-Seal</button><p class="occ-helper">Dieser Alt-Run bleibt unverändert, kann aber nicht fortgesetzt werden. Run-Auswahl lösen und einen neuen Run anlegen.</p>';
    }
    if (workflow.current_phase === 'create_experiment') {
      const setup = this._activePositionSetup();
      if (!setup) {
        return '<button type="button" class="occ-button occ-button-primary" disabled>Runtime-Setup fehlt</button><p class="occ-helper">Server mit dem gültigen Setup-v2 über <code>--position-setup</code> starten. Ein Profil-Hash allein kann den Run nicht versiegeln.</p>';
      }
      return `<button type="button" class="occ-button occ-button-primary" data-occ-action="seal">Mit ${escapeHTML(setup.setup_id)} versiegeln</button><p class="occ-helper">${infoTip('Runtime-Seal', 'Der Server prüft das Run-Profil gegen das aktive Setup-v2 und speichert Setup-ID plus Setup-Hash im Phasenereignis.')}</p>`;
    }
    if (workflow.current_phase === 'seal_setup') {
      return this._emptyCalibrationWorkflowMarkup();
    }
    if (workflow.current_phase === 'empty_calibration') {
      if (['collecting', 'stopping', 'ready_to_finish'].includes(this.emptyCalibrationPlan?.phase)) {
        return this._emptyCalibrationWorkflowMarkup();
      }
      if (workflow.current_status === 'RUNNING') return '<button type="button" class="occ-button occ-button-primary" data-occ-action="stop-empty">Leerkalibrierung abschließen</button>';
      if (['PASS', 'REUSED'].includes(workflow.current_status)) return '<button type="button" class="occ-button occ-button-primary" data-occ-action="open-training">Training öffnen</button>';
      return this._emptyCalibrationWorkflowMarkup();
    }
    if (workflow.current_phase === 'train_p01_p09') {
      if (workflow.current_status === 'PASS') return '<button type="button" class="occ-button occ-button-primary" data-occ-action="open-randomize">Blindtest-Vorbereitung öffnen</button>';
      return `
        <div class="occ-calibration-route occ-workflow-route">
          <div class="occ-route-kicker">STANDARDWEG</div>
          <div class="occ-route-status ${this.status?.mmwave?.packets_received > 0 ? 'is-ready' : 'is-waiting'}"><span></span>${this.status?.mmwave?.packets_received > 0 ? 'Radar-Referenz verfügbar' : 'Radar noch nicht verbunden'}</div>
          <h5>CSI mit mmWave-Koordinaten kalibrieren</h5>
          <p>CSI-Fenster werden mit Radarpositionen gekoppelt. P01–P09 entfällt.</p>
          <button type="button" class="occ-button occ-button-primary" data-occ-action="open-mmwave-calibration">mmWave öffnen</button>
        </div>
        <details class="occ-fold occ-manual-fallback" ${this.manualCaptureExpanded ? 'open' : ''}>
          <summary data-occ-action="toggle-manual-capture"><span>Punktaufnahme</span><small>Legacy-Fallback</small></summary>
          <label class="occ-field"><span>Referenzpunkt ${infoTip('Manueller Referenzpunkt', 'Dieser Ablauf bleibt nur für bestehende P01–P09-Datensätze und Tests ohne Radar erhalten.')}</span><select data-occ-select="training-point">${EXPECTED_POINTS.map((point) => `<option value="${point}" ${point === currentPoint ? 'selected' : ''} ${completedTrainingPoints.has(point) ? 'disabled' : ''}>${point}${completedTrainingPoints.has(point) ? ' · fertig' : ''}</option>`).join('')}</select></label>
          <div class="occ-inline-actions">${this.status?.recording?.phase === 'recording' ? '<button type="button" class="occ-button occ-button-primary" data-occ-action="stop-recording">Aufnahme stoppen</button>' : `<button type="button" class="occ-button occ-button-quiet" data-occ-action="record-training">${currentPoint} als Fallback aufnehmen</button>`}${trainingCount === 9 && this.status?.recording?.phase !== 'recording' ? '<button type="button" class="occ-button occ-button-quiet" data-occ-action="finish-training">Fallback-Training abschließen</button>' : ''}</div>
          <p class="occ-helper">35 s CSI-Aufnahme pro Punkt. Nicht der reguläre Ablauf mit angeschlossenem mmWave-Sensor.</p>
        </details>
      `;
    }
    if (workflow.current_phase === 'randomize_blind_positions') {
      if (workflow.current_status === 'PASS') return '<button type="button" class="occ-button occ-button-primary" data-occ-action="open-capture">Blindaufnahmen öffnen</button>';
      return `<button type="button" class="occ-button occ-button-primary" data-occ-action="randomize">Blind-Reihenfolge erzeugen</button><p class="occ-helper">Der Seed wird im Run gespeichert; die Wahrheit bleibt bis zur Truth-Phase getrennt. ${infoTip('Blind-Reihenfolge', 'Die 18 Positionen werden reproduzierbar gemischt. Während der Aufnahme wird die Ground Truth nicht an die Prediction weitergegeben.')}</p>`;
    }
    if (workflow.current_phase === 'capture') {
      if (workflow.current_status === 'PASS') return '<button type="button" class="occ-button occ-button-primary" data-occ-action="open-predict">Prediction öffnen</button>';
      const order = this._blindOrder();
      const nextBlindPoint = order[this.blindIndex] || '--';
      return `
        <div class="occ-instruction"><span>Nächste Blind-Anweisung ${infoTip('Blindaufnahme', 'Die Position wird nur dem Versuchsablauf angezeigt. Sie darf nicht in den Prediction-Eingang oder die Capture-Metadaten gelangen.')}</span><strong>${escapeHTML(nextBlindPoint)}</strong><small>${this.blindIndex + 1}/18 · wird nicht in der Capture-Metadatei gespeichert</small></div>
        <div class="occ-inline-actions">${this.status?.recording?.phase === 'recording' ? '<button type="button" class="occ-button occ-button-primary" data-occ-action="stop-recording">Aufnahme stoppen</button>' : '<button type="button" class="occ-button occ-button-primary" data-occ-action="record-blind">Blindaufnahme starten</button>'}${blindCount >= 18 && this.status?.recording?.phase !== 'recording' ? '<button type="button" class="occ-button occ-button-quiet" data-occ-action="finish-blind">Blindaufnahmen abschließen</button>' : ''}</div>
        <p class="occ-helper">Die ausgewählte Position wird nicht an die Prediction weitergegeben. 18 Blind-Aufnahmen sind für die bestehende Gate-Logik vorgesehen.</p>
      `;
    }
    if (workflow.current_phase === 'predict') {
      return workflow.current_status === 'READY'
        ? '<div class="occ-lock-note">Prediction-Artefakt registriert. Truth bleibt bis zur nächsten Phase getrennt.</div><button type="button" class="occ-button occ-button-primary" data-occ-action="advance-reveal">Truth-Phase öffnen</button>'
        : `${this._artifactForm('prediction', 'Prediction-Artefakt', 'position-predictions.json')}<p class="occ-helper">Der Server liest die Datei nur aus dem lokalen Datenverzeichnis und speichert ihren SHA-256-Hash.</p>`;
    }
    if (workflow.current_phase === 'reveal_truth') {
      return workflow.current_status === 'READY'
        ? '<div class="occ-lock-note">Truth-Artefakt registriert. Die bestehenden Offline-Gates bleiben maßgeblich.</div><button type="button" class="occ-button occ-button-primary" data-occ-action="advance-evaluate">Evaluation öffnen</button>'
        : `${this._artifactForm('truth', 'Truth-Artefakt', 'position-truth.json')}<p class="occ-helper">Truth wird erst hier registriert; sie gehört nicht in Trainings- oder Prediction-Dateien.</p>`;
    }
    if (workflow.current_phase === 'evaluate') {
      return workflow.current_status === 'READY'
        ? '<div class="occ-lock-note">Evaluation-Artefakt registriert; Report kann erzeugt werden.</div><button type="button" class="occ-button occ-button-primary" data-occ-action="advance-report">Report-Phase öffnen</button>'
        : `${this._artifactForm('evaluation', 'Evaluation-Artefakt', 'position-evaluation.json')}<p class="occ-helper">Die bestehenden Offline-Gates bleiben die Quelle für Accuracy, Coverage und Fehlerdistanz.</p>`;
    }
    if (workflow.current_phase === 'report') {
      return workflow.current_status === 'PASS'
        ? '<button type="button" class="occ-button occ-button-primary" data-occ-action="write-report">Report schreiben</button>'
        : '<span class="occ-lock-note">Report wartet auf die vorherigen Artefakte.</span>';
    }
    return '';
  }

  _calibrationReuseMarkup() {
    const availability = this.calibrationAvailability;
    const calibration = availability?.calibration;
    if (availability?.available !== true || !calibration) return '';
    return `<div class="occ-calibration-reuse"><div class="occ-route-kicker">KOMPATIBLE BASELINE</div><h5>Gespeicherte Leerkalibrierung gefunden</h5><p>Profilkontext und versiegeltes Setup stimmen überein. Die Baseline von ${escapeHTML(formatTime(calibration.captured_at))} kann für diesen Run wiederverwendet werden.</p><div class="occ-inline-actions"><button type="button" class="occ-button occ-button-primary" data-occ-action="reuse-empty">Gespeicherte Baseline verwenden</button><button type="button" class="occ-button occ-button-quiet" data-occ-action="prepare-empty">Neue Leerkalibrierung</button></div><p class="occ-helper">Kalibrierung ${escapeHTML(calibration.calibration_id)} · ${escapeHTML(calibration.node_count)} RX-Referenzen. Eine Positions-, Raum-, Firmware-, Grid- oder TX-Filteränderung würde diese Option entfernen.</p></div>`;
  }

  _emptyCalibrationWorkflowMarkup() {
    const plan = this.emptyCalibrationPlan;
    const remaining = this._emptyCalibrationRemainingSeconds();
    const reuseMarkup = this._calibrationReuseMarkup();
    if (!plan) {
      return `${reuseMarkup}<div id="occEmptyCalibrationRoute" class="occ-calibration-route"><div class="occ-route-kicker">VORBEREITUNG</div><h5>Leerkalibrierung vorbereiten</h5><p>Lege zuerst die Messdauer und den Vorlauf fest. Die Aufnahme startet erst nach dem Countdown.</p><button type="button" class="occ-button occ-button-primary" data-occ-action="prepare-empty">Leerkalibrierung vorbereiten</button><p class="occ-helper">Mindestens 60 Sekunden · Vorlauf schützt den Raum vor Bewegung beim Start.</p></div>`;
    }
    if (plan.phase === 'form') {
      return `${reuseMarkup}<div id="occEmptyCalibrationRoute" class="occ-calibration-route"><div class="occ-route-kicker">VORBEREITUNG</div><h5>Dauer und Vorlauf festlegen</h5><p>Die Messung beginnt erst, wenn der Countdown abgelaufen ist.</p><form id="occEmptyCalibrationPrepareForm" class="occ-empty-calibration-form"><label class="occ-field"><span>Messdauer (Sekunden) · mindestens 60</span><input name="duration_seconds" type="number" min="${EMPTY_CALIBRATION_MIN_SECONDS}" step="1" value="${plan.durationSeconds}" required></label><label class="occ-field"><span>Vorlauf bis Start (Sekunden)</span><input name="lead_seconds" type="number" min="${EMPTY_CALIBRATION_MIN_LEAD_SECONDS}" step="1" value="${plan.leadSeconds}" required></label><div class="occ-inline-actions"><button type="submit" class="occ-button occ-button-primary">Countdown starten</button><button type="button" class="occ-button occ-button-quiet" data-occ-action="cancel-empty-preparation">Abbrechen</button></div></form><p class="occ-helper">Nach dem Vorlauf startet die WiFi-Leerkalibrierung automatisch und endet nach der gewählten Dauer.</p></div>`;
    }
    if (plan.phase === 'countdown') {
      return `<div id="occEmptyCalibrationRoute" class="occ-calibration-route"><div class="occ-route-kicker">RAUM VERLASSEN</div><div class="occ-instruction"><span>LEERKALIBRIERUNG STARTET IN</span><strong>${remaining} s</strong><small>Bitte jetzt den Raum verlassen. Es werden noch keine CSI-Daten gesammelt.</small>${this._emptyCalibrationSafeReturnMarkup()}</div><button type="button" class="occ-button occ-button-quiet" data-occ-action="cancel-empty-preparation">Countdown abbrechen</button></div>`;
    }
    if (plan.phase === 'starting') {
      return `<div id="occEmptyCalibrationRoute" class="occ-calibration-route"><div class="occ-route-kicker">START</div><div class="occ-instruction"><span>LEERKALIBRIERUNG WIRD GESTARTET</span><strong>…</strong><small>Bitte außerhalb bleiben.</small>${this._emptyCalibrationSafeReturnMarkup()}</div></div>`;
    }
    if (plan.phase === 'collecting') {
      return `<div id="occEmptyCalibrationRoute" class="occ-calibration-route"><div class="occ-route-kicker">MESSUNG LÄUFT</div><div class="occ-instruction"><span>LEERKALIBRIERUNG ENDET IN</span><strong>${remaining} s</strong><small>Raum leer halten. Der Abschluss erfolgt automatisch.</small>${this._emptyCalibrationSafeReturnMarkup()}</div></div>`;
    }
    if (plan.phase === 'stopping') {
      return `<div id="occEmptyCalibrationRoute" class="occ-calibration-route"><div class="occ-route-kicker">AUSWERTUNG</div><div class="occ-instruction"><span>BASELINE WIRD GESPEICHERT</span><strong>…</strong><small>Bitte noch nicht in den Raum zurückkehren.</small></div></div>`;
    }
    return `<div id="occEmptyCalibrationRoute" class="occ-calibration-route"><div class="occ-route-kicker">ABSCHLUSS</div><div class="occ-instruction"><span>MESSDAUER ERREICHT</span><strong>0 s</strong><small>Der automatische Abschluss muss noch bestätigt werden.</small></div><button type="button" class="occ-button occ-button-primary" data-occ-action="stop-empty">Leerkalibrierung abschließen</button></div>`;
  }

  _artifactForm(kind, label, placeholder) {
    const explanation = {
      prediction: 'Prediction enthält nur die Modellvorhersagen aus dem Blindtest.',
      truth: 'Truth enthält die später aufgedeckten echten Positionen und bleibt bis zu dieser Phase getrennt.',
      evaluation: 'Evaluation enthält die berechneten Kennzahlen und Gate-Ergebnisse aus Prediction plus Truth.',
    }[kind] || 'Artefaktdatei des aktuellen Experiment-Runs.';
    return `<div class="occ-artifact-form"><label class="occ-field"><span>${escapeHTML(label)} · relativer Pfad unter data/ ${infoTip(label, explanation)}</span><input data-occ-artifact-input="${attribute(kind)}" type="text" maxlength="240" value="${attribute(this.artifactPaths[kind] || '')}" placeholder="${attribute(placeholder)}" autocomplete="off"></label><button type="button" class="occ-button occ-button-quiet" data-occ-action="register-${attribute(kind)}">Prüfen & registrieren</button></div>`;
  }

  _nodesMarkup() {
    const nodes = Array.isArray(this.status?.nodes) ? this.status.nodes : [];
    if (!nodes.length) return '<div class="occ-empty">Keine CSI-Nodes.</div>';
    return `<div class="occ-table-wrap"><table class="occ-table"><thead><tr><th>Node</th><th>Status</th><th>RSSI ${infoTip('RSSI', 'Empfangsstärke des letzten Pakets.')}</th><th>CSI-Rate ${infoTip('CSI-Rate', 'Frames pro Sekunde.')}</th><th>Verlust ${infoTip('Verlust', 'Aus Sequenzlücken geschätzt.')}</th><th>Seq.</th><th>Zeit / Sync ${infoTip('Zeit / Sync', 'Zeitbezug zum Mesh.')}</th></tr></thead><tbody>${nodes.map((node) => `
      <tr><td><strong>${escapeHTML(node.display_name || `RX${node.node_id}`)}</strong><small>${escapeHTML(node.role || 'unbekannt')}</small></td><td><span class="occ-node-state ${node.status === 'active' ? 'is-active' : 'is-stale'}">${escapeHTML(node.status)}</span><small>${escapeHTML(node.last_seen_ms ?? '--')} ms ago</small></td><td>${node.rssi_dbm == null ? '--' : `${Number(node.rssi_dbm).toFixed(1)} dBm`}</td><td>${node.frame_rate_hz == null ? `wird ermittelt (${escapeHTML(node.frame_rate_samples ?? 0)})` : `${Number(node.frame_rate_hz).toFixed(1)} Hz`}</td><td>${node.packet_loss_percent == null ? '--' : `${Number(node.packet_loss_percent).toFixed(1)}%`}<small>${escapeHTML(node.inferred_lost_frames ?? 0)} geschätzt</small></td><td>${escapeHTML(node.latest_sequence ?? '--')}</td><td>${node.sync ? `${escapeHTML(node.sync.is_valid ? 'gültig' : 'veraltet')} · ${escapeHTML(node.sync.offset_us)} µs` : 'kein Mesh-Sync'}</td></tr>
    `).join('')}</tbody></table></div>`;
  }

  _recordingsMarkup() {
    if (!this.recordings.length) return '<div class="occ-empty">Keine CSI-Aufnahmen.</div>';
    return `<div class="occ-recording-list">${this.recordings.slice(0, 12).map((recording) => `<div class="occ-recording-row"><span><strong>${escapeHTML(recording.label || recording.id)}</strong><small>${escapeHTML(recording.status || '--')} · ${escapeHTML(recording.frame_count ?? recording.frames ?? 0)} frames</small></span><small>${escapeHTML(recording.id)}</small></div>`).join('')}</div>`;
  }

  _benchmarkMarkup() {
    const activeModel = this.status?.active_model_id || 'none';
    const catalog = this.benchmarkCatalog || {};
    const comparators = Array.isArray(catalog.comparators) ? catalog.comparators : [];
    const ablation = Array.isArray(catalog.rx_ablation) ? catalog.rx_ablation : [];
    return `<div class="occ-benchmark-grid"><div><span>Modell</span><strong>${escapeHTML(activeModel)}</strong></div><div><span>RVF</span><strong>${this.models.length}</strong></div><div><span>Baseline</span><strong>${escapeHTML(catalog.baseline?.id || 'prototype_d6')}</strong></div><div><span>Vergleich</span><strong>${comparators.map((model) => escapeHTML(model.id)).join(' · ') || 'lädt'}</strong></div><div><span>Split</span><strong>${escapeHTML(catalog.split?.id || 'sealed_wifi_train_blind_test_v1')}</strong></div><div><span>RX-Ablation</span><strong>${ablation.length || 5}</strong></div></div><p class="occ-helper">${catalog.status === 'READY_FOR_WIFI_DATA' ? 'Bereit. Ohne neue gelabelte Captures keine Modellwerte.' : 'Benchmark lädt.'} mmWave bleibt Referenz.</p>`;
  }

  _emptyCalibrationRemainingSeconds() {
    const plan = this.emptyCalibrationPlan;
    let deadline = null;
    if (plan?.phase === 'countdown') {
      deadline = plan.startsAtMs;
    } else if (plan?.phase === 'collecting') {
      deadline = plan.endsAtMs;
    }
    if (!Number.isFinite(deadline)) return 0;
    return Math.max(0, Math.ceil((deadline - Date.now()) / 1000));
  }

  _emptyCalibrationSafeReturnAt() {
    const plan = this.emptyCalibrationPlan;
    if (!plan || !Number.isFinite(plan.durationSeconds)) return null;
    if (['countdown', 'starting'].includes(plan.phase) && Number.isFinite(plan.startsAtMs)) {
      return plan.startsAtMs + plan.durationSeconds * 1000;
    }
    if (plan.phase === 'collecting' && Number.isFinite(plan.endsAtMs)) return plan.endsAtMs;
    return null;
  }

  _emptyCalibrationSafeReturnMarkup() {
    const safeReturnAt = this._emptyCalibrationSafeReturnAt();
    if (!Number.isFinite(safeReturnAt)) return '';
    return `<small class="occ-safe-return">Sicher zurück ab ${formatClockTime(safeReturnAt)} Uhr</small>`;
  }

  _clearEmptyCalibrationTimer() {
    if (this._emptyCalibrationTimer !== null) clearInterval(this._emptyCalibrationTimer);
    this._emptyCalibrationTimer = null;
  }

  _startEmptyCalibrationTimer() {
    this._clearEmptyCalibrationTimer();
    this._emptyCalibrationTimer = setInterval(() => this._tickEmptyCalibration(), EMPTY_CALIBRATION_TIMER_MS);
  }

  _prepareEmptyCalibration() {
    if (this.emptyCalibrationPlan?.phase === 'collecting' || this.emptyCalibrationPlan?.phase === 'stopping') return;
    this._clearEmptyCalibrationTimer();
    this.emptyCalibrationPlan = {
      phase: 'form',
      durationSeconds: EMPTY_CALIBRATION_DEFAULT_SECONDS,
      leadSeconds: EMPTY_CALIBRATION_DEFAULT_LEAD_SECONDS,
    };
    this.error = '';
    this.message = '';
    this._render();
  }

  _cancelEmptyCalibrationPreparation() {
    if (!['form', 'countdown'].includes(this.emptyCalibrationPlan?.phase)) return;
    this._clearEmptyCalibrationTimer();
    this.emptyCalibrationPlan = null;
    this.error = '';
    this.message = 'Leerkalibrierung nicht gestartet.';
    this._render();
  }

  _tickEmptyCalibration() {
    const plan = this.emptyCalibrationPlan;
    if (!plan) {
      this._clearEmptyCalibrationTimer();
      return;
    }
    if (this.busy) return;
    const now = Date.now();
    if (plan.phase === 'countdown') {
      if (now >= plan.startsAtMs) {
        this._clearEmptyCalibrationTimer();
        void this._startEmptyCalibration(plan);
        return;
      }
      const remaining = this._emptyCalibrationRemainingSeconds();
      if (plan.displaySeconds !== remaining) {
        this.emptyCalibrationPlan = { ...plan, displaySeconds: remaining };
        this._render();
      }
      return;
    }
    if (plan.phase === 'collecting') {
      if (now >= plan.endsAtMs) {
        this._clearEmptyCalibrationTimer();
        void this._stopEmptyCalibration({ automatic: true });
        return;
      }
      const remaining = this._emptyCalibrationRemainingSeconds();
      if (plan.displaySeconds !== remaining) {
        this.emptyCalibrationPlan = { ...plan, displaySeconds: remaining };
        this._render();
      }
    }
  }

  _scheduleEmptyCalibration(form) {
    const read = (name) => Number(form.querySelector(`[name="${name}"]`)?.value);
    const durationSeconds = read('duration_seconds');
    const leadSeconds = read('lead_seconds');
    if (!Number.isInteger(durationSeconds) || durationSeconds < EMPTY_CALIBRATION_MIN_SECONDS) {
      this.error = `Die Messdauer muss eine ganze Zahl von mindestens ${EMPTY_CALIBRATION_MIN_SECONDS} Sekunden sein.`;
      this._render();
      return;
    }
    if (!Number.isInteger(leadSeconds) || leadSeconds < EMPTY_CALIBRATION_MIN_LEAD_SECONDS) {
      this.error = `Der Vorlauf muss eine ganze Zahl von mindestens ${EMPTY_CALIBRATION_MIN_LEAD_SECONDS} Sekunden sein.`;
      this._render();
      return;
    }

    this.error = '';
    this.message = `Start in ${leadSeconds} s. Bitte jetzt den Raum verlassen.`;
    this.emptyCalibrationPlan = {
      phase: 'countdown',
      durationSeconds,
      leadSeconds,
      startsAtMs: Date.now() + leadSeconds * 1000,
      displaySeconds: leadSeconds,
    };
    this._startEmptyCalibrationTimer();
    this._render();
  }

  async _onSubmit(event) {
    if (event.target.id === 'occProfileForm') {
      event.preventDefault();
      await this._saveProfile(event.target);
    } else if (event.target.id === 'occWorkflowForm') {
      event.preventDefault();
      await this._createWorkflow(event.target);
    } else if (event.target.id === 'occEmptyCalibrationPrepareForm') {
      event.preventDefault();
      this._scheduleEmptyCalibration(event.target);
    }
  }

  _onInput(event) {
    const target = event.target;
    if (!target?.closest) return;
    const profileForm = target.closest('#occProfileForm');
    if (profileForm) {
      this.profileLabel = String(profileForm.querySelector('[name="profile_label"]')?.value || '');
      this.profileDraft = this._readProfileFromForm(profileForm);
      this.geometryEditor?.setDocument(this.profileDraft);
    }
    if (target.matches('[name="workflow_label"]')) this.workflowLabel = String(target.value || '');
    if (target.matches('[data-occ-select="training-point"]')) this.trainingPoint = String(target.value || 'P01');
    const artifactKind = target.dataset.occArtifactInput;
    if (artifactKind) this.artifactPaths[artifactKind] = String(target.value || '');
  }

  async _onClick(event) {
    const runButton = event.target.closest('[data-occ-run-id]');
    if (runButton) {
      const run = this.runs.find((candidate) => candidate.id === runButton.dataset.occRunId);
      if (run) {
        this.selectedRun = run;
        this.runSelectionCleared = false;
        this.calibrationAvailability = null;
        this._render();
        const context = this._calibrationContextRequest();
        if (context) {
          void experimentService.getCalibrationAvailability({
            profileId: context.profile_id,
            profileRevisionId: context.profile_revision_id,
          }).then((availability) => {
            const currentContext = this._calibrationContextRequest();
            if (currentContext?.profile_id !== context.profile_id
              || currentContext?.profile_revision_id !== context.profile_revision_id) return;
            this.calibrationAvailability = availability;
            this._render();
          }).catch(() => {});
        }
      }
      return;
    }
    const profileButton = event.target.closest('[data-occ-profile-id]');
    if (profileButton) {
      const profile = this.profiles.find((candidate) => candidate.id === profileButton.dataset.occProfileId);
      if (profile) {
        this._selectProfile(profile);
        this.calibrationAvailability = null;
        this._render();
        const workflow = this.selectedRun?.workflow;
        const context = workflow?.profile_id
          ? this._calibrationContextRequest()
          : {
            profile_id: profile.id,
            ...(profile.revision_id ? { profile_revision_id: profile.revision_id } : {}),
          };
        void experimentService.getCalibrationAvailability({
          profileId: context.profile_id,
          profileRevisionId: context.profile_revision_id,
        }).then((availability) => {
          const currentContext = this._calibrationContextRequest();
          if (currentContext?.profile_id !== context.profile_id
            || currentContext?.profile_revision_id !== context.profile_revision_id) return;
          this.calibrationAvailability = availability;
          this._render();
        }).catch(() => {});
      }
      return;
    }
    const button = event.target.closest('[data-occ-action]');
    if (!button || this.busy) return;
    const action = button.dataset.occAction;
    if (action === 'refresh') return this.refresh();
    if (action === 'focus-profile') return this._focusAndScroll('#occProfileForm [name="profile_label"]');
    if (action === 'focus-workflow') return this._focusAndScroll('#occWorkflowForm [name="workflow_label"]');
    if (action.startsWith('focus-artifact-')) {
      return this._focusAndScroll(`[data-occ-artifact-input="${attribute(action.replace('focus-artifact-', ''))}"]`);
    }
    if (action === 'generate-points') return this._generatePointsFromForm();
    if (action === 'calculate-mmwave-placement') {
      const result = this.geometryEditor?.calculateOptimalMmwavePlacement?.();
      const status = this.container.querySelector('[data-occ-mmwave-placement-status]');
      if (result?.ok) {
        if (status) status.textContent = `Berechnet: ${result.label} · Position ${result.positionM.map((value) => Number(value).toFixed(2)).join(' / ')} m · ${result.referencePointsCovered}/${result.referencePointCount} TX/RX-Referenzen im Sichtfeld. Profilversion speichern.`;
      } else if (status) {
        status.textContent = result?.error || 'Die mmWave-Position konnte nicht berechnet werden.';
      }
      return;
    }
    if (action === 'toggle-profiles') {
      event.preventDefault();
      this.profilesExpanded = !this.profilesExpanded;
      this._render();
      return;
    }
    if (action === 'toggle-runs') {
      event.preventDefault();
      this.runsExpanded = !this.runsExpanded;
      this._render();
      return;
    }
    if (action === 'toggle-point-grid') {
      event.preventDefault();
      this.pointGridExpanded = !this.pointGridExpanded;
      this._render();
      return;
    }
    if (action === 'toggle-manual-capture') {
      event.preventDefault();
      this.manualCaptureExpanded = !this.manualCaptureExpanded;
      this._render();
      return;
    }
    if (action === 'prepare-empty') return this._prepareEmptyCalibration();
    if (action === 'focus-empty-preparation') return this._focusAndScroll('#occEmptyCalibrationPrepareForm [name="duration_seconds"]');
    if (action === 'cancel-empty-preparation') return this._cancelEmptyCalibrationPreparation();
    if (action === 'reuse-empty') return this._reuseEmptyCalibration();
    if (action === 'open-mmwave-calibration') {
      const assistant = document.querySelector('.mmwave-assistant');
      if (assistant) {
        assistant.setAttribute('tabindex', '-1');
        assistant.scrollIntoView({ behavior: 'smooth', block: 'start' });
        assistant.focus({ preventScroll: true });
        this.message = 'mmWave-Assistent geöffnet. Der Sensorpfad bleibt bis zum ersten echten Radar-Paket unvalidiert.';
      } else {
        this.error = 'Der mmWave-Assistent ist in dieser Ansicht nicht verfügbar.';
      }
      this._render();
      return;
    }
    if (action === 'clear-run') {
      if (['form', 'countdown'].includes(this.emptyCalibrationPlan?.phase)) this._cancelEmptyCalibrationPreparation();
      this.selectedRun = null;
      this.runSelectionCleared = true;
      this._render();
      return;
    }
    if (action === 'seal') return this._sealSetup();
    if (action === 'start-empty') return this._startEmptyCalibration();
    if (action === 'stop-empty') return this._stopEmptyCalibration();
    if (action === 'open-training') return this._advance('train_p01_p09', 'READY', { resumed_after_empty_baseline: true, calibration_reference: 'mmwave' });
    if (action === 'record-training') return this._recordPoint('training');
    if (action === 'stop-recording') return this._stopCurrentRecording();
    if (action === 'finish-training') return this._completePhaseAndOpenNext('train_p01_p09', 'randomize_blind_positions', { capture_count: 9, capture_kind: 'training', calibration_reference: 'manual_checkpoint_fallback' });
    if (action === 'open-randomize') return this._advance('randomize_blind_positions', 'READY', { resumed_after_completed_training: true });
    if (action === 'randomize') return this._randomizeBlindPositions();
    if (action === 'open-capture') return this._advance('capture', 'READY', { resumed_after_randomization: true });
    if (action === 'record-blind') return this._recordPoint('blind');
    if (action === 'finish-blind') return this._completePhaseAndOpenNext('capture', 'predict', { capture_count: 18, capture_kind: 'blind' }, { awaiting_prediction_artifact: true }, 'RUNNING');
    if (action === 'open-predict') return this._advance('predict', 'RUNNING', { resumed_after_completed_capture: true, awaiting_prediction_artifact: true });
    if (action === 'advance-reveal') return this._advance('reveal_truth', 'READY', { prediction_artifact_registered: true });
    if (action === 'advance-evaluate') return this._advance('evaluate', 'READY', { truth_artifact_registered: true });
    if (action === 'register-prediction') return this._registerArtifact('prediction', 'predict');
    if (action === 'register-truth') return this._registerArtifact('truth', 'reveal_truth');
    if (action === 'register-evaluation') return this._registerArtifact('evaluation', 'evaluate');
    if (action === 'advance-report') return this._advance('report', 'PASS', { report_inputs_registered: true, validation_status: 'UNVALIDATED' });
    if (action === 'write-report') return this._writeReport();
  }

  _focusAndScroll(selector) {
    const target = this.container?.querySelector(selector);
    if (!target) {
      this.error = 'Das Ziel dieses Guide-Schritts ist in der aktuellen Ansicht nicht verfügbar.';
      this._render();
      return;
    }
    target.scrollIntoView({ behavior: 'smooth', block: 'center' });
    target.focus({ preventScroll: true });
  }

  _readProfileFromForm(form = this.container.querySelector('#occProfileForm')) {
    const read = (prefix) => [0, 1, 2].map((index) => numberValue(form.querySelector(`[data-occ-field="${prefix}.${index}"]`)?.value));
    const existingMmwave = this.profileDraft?.mmwave || {};
    const mmwaveExteriorInput = form.querySelector('[data-cad-mmwave-exterior]')
      || this.container?.querySelector?.('[data-cad-mmwave-exterior]');
    const allowMmwaveExterior = mmwaveExteriorInput
      ? Boolean(mmwaveExteriorInput.checked)
      : existingMmwave.allow_exterior !== false;
    const existingRadio = this.profileDraft?.radio || { channel: 6 };
    const existingEnvironment = this.profileDraft?.environment || {
      layout_revision: 'control-center',
      furniture_revision: 'control-center',
      door_state_revision: 'closed',
    };
    return {
      schema_version: 1,
      profile_kind: 'ruview.setup-profile',
      room_dimensions_m: read('room_dimensions_m'),
      sensor_mount_radius_m: this.profileDraft?.sensor_mount_radius_m ?? DEFAULT_SENSOR_MOUNT_RADIUS_M,
      transmitter: { id: 'TX', position_m: read('transmitter.position_m') },
      receivers: ['RX1', 'RX2', 'RX3', 'RX4'].map((id) => ({ id, role: 'receiver', position_m: read(`receiver.${id}`) })),
      mmwave: {
        ...existingMmwave,
        sensor: existingMmwave.sensor || MMWAVE_SENSOR,
        mounting_position_m: read('mmwave.mounting_position_m'),
        mounting_revision: existingMmwave.mounting_revision || 'draft',
        allow_exterior: allowMmwaveExterior,
      },
      points: EXPECTED_POINTS.map((id) => ({ id, coordinates_m: read(`point.${id}`) })),
      radio: existingRadio,
      environment: existingEnvironment,
      mmwave_status: 'NOT_CONNECTED',
    };
  }

  async _saveProfile(form) {
    const label = String(new FormData(form).get('profile_label') || '').trim();
    const document = this._readProfileFromForm(form);
    const validation = validateGeometryDraft(document);
    if (!label) {
      this.message = '';
      this.error = 'Bitte einen Profilnamen eintragen.';
      this._render();
      return;
    }
    if (!validation.valid) {
      this.message = '';
      this.error = `Geometrie ist nicht speicherbar: ${validation.errors.join(' ')}`;
      this._render();
      return;
    }
    this.profileDraft = document;
    this.busy = true;
    this.error = '';
    this._render();
    try {
      const profile = this.selectedProfile
        ? await experimentService.updateProfile(this.selectedProfile.id, { label, document })
        : await experimentService.createProfile({ label, document });
      this._selectProfile(profile);
      this.message = `Profile gespeichert: ${profile.profile_sha256.slice(0, 16)}…`;
      await this.refresh({ quiet: true, allowWhileBusy: true });
    } catch (error) {
      this.message = '';
      this.error = error?.message || 'Setup-Profil konnte nicht gespeichert werden.';
    } finally {
      this.busy = false;
      this._render();
    }
  }

  _generatePointsFromForm() {
    const form = this.container.querySelector('#occProfileForm');
    if (!form) return;
    const dimensions = [0, 1, 2].map((index) => numberValue(form.querySelector(`[data-occ-field="room_dimensions_m.${index}"]`)?.value));
    generateThreeByThreePoints(dimensions).forEach((point) => {
      point.coordinates_m.forEach((value, coordinate) => {
        const input = form.querySelector(`[data-occ-field="point.${point.id}.${coordinate}"]`);
        if (input) input.value = value;
      });
    });
    this.profileLabel = String(form.querySelector('[name="profile_label"]')?.value || this.profileLabel);
    this.profileDraft = this._readProfileFromForm(form);
    this.message = 'Optionales P01–P09-Kontrollraster aktualisiert. Die reguläre Kalibrierung verwendet mmWave-Koordinaten.';
    this._render();
  }

  async _createWorkflow(form) {
    this.busy = true;
    this.error = '';
    try {
      const data = new FormData(form);
      const profileId = String(data.get('profile_id') || '');
      this.workflowLabel = String(data.get('workflow_label') || '').trim();
      const selectedProfile = this.profiles.find((profile) => profile.id === profileId) || this.selectedProfile;
      this.selectedRun = await experimentService.createWorkflow({
        label: this.workflowLabel,
        profileId,
        profileRevisionId: selectedProfile?.revision_id,
      });
      this.runSelectionCleared = false;
      this.message = 'Workflow angelegt. Als Nächstes das aktive Runtime-Setup prüfen und versiegeln.';
      await this.refresh({ quiet: true, allowWhileBusy: true });
    } catch (error) {
      this.message = '';
      this.error = error?.message || 'Workflow konnte nicht angelegt werden.';
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _advance(phase, status, payload = {}) {
    if (!this.selectedRun) return;
    this.busy = true;
    this.error = '';
    try {
      this.selectedRun = await experimentService.advancePhase(this.selectedRun.id, { phase, status, payload });
      this.message = `${phaseLabel(phase)}: ${status}`;
      await this.refresh({ quiet: true, allowWhileBusy: true });
    } catch (error) {
      this.message = '';
      this.error = error?.message || `Workflow-Phase ${phase} konnte nicht gespeichert werden.`;
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _sealSetup() {
    const setup = this._activePositionSetup();
    if (!setup) {
      this.message = '';
      this.error = 'Setup kann nicht versiegelt werden: Der Server läuft ohne aktives Setup-v2.';
      this._render();
      return;
    }
    return this._advance('seal_setup', 'PASS', {
      setup_id: setup.setup_id,
      setup_sha256: setup.setup_sha256,
    });
  }

  async _completePhaseAndOpenNext(currentPhase, nextPhase, payload = {}, nextPayload = {}, nextStatus = 'READY') {
    if (!this.selectedRun) return;
    this.busy = true;
    this.error = '';
    try {
      let run = await experimentService.advancePhase(this.selectedRun.id, {
        phase: currentPhase,
        status: 'PASS',
        payload,
      });
      run = await experimentService.advancePhase(run.id, {
        phase: nextPhase,
        status: nextStatus,
        payload: nextPayload,
      });
      this.selectedRun = run;
      this.message = `${phaseLabel(currentPhase)} abgeschlossen. ${phaseLabel(nextPhase)} ist bereit.`;
      await this.refresh({ quiet: true, allowWhileBusy: true });
    } catch (error) {
      this.message = '';
      this.error = error?.message || `Workflow konnte nicht zu ${phaseLabel(nextPhase)} wechseln.`;
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _startEmptyCalibration(plan = this.emptyCalibrationPlan) {
    if (!plan || plan.phase !== 'countdown') return;
    this._clearEmptyCalibrationTimer();
    this.emptyCalibrationPlan = { ...plan, phase: 'starting', displaySeconds: 0 };
    this.busy = true;
    this.error = '';
    this.message = '';
    this._render();
    try {
      const context = this._calibrationContextRequest();
      if (!context) throw new Error('Bitte zuerst ein Setup-Profil auswählen.');
      const response = await apiService.post('/api/v1/classification/calibration/start', context);
      if (response?.success !== true) throw new Error(response?.error || 'D5/D6-Kalibrierung konnte nicht starten.');
      const startedAtMs = Date.now();
      this.emptyCalibrationPlan = {
        ...plan,
        phase: 'collecting',
        startedAtMs,
        endsAtMs: startedAtMs + plan.durationSeconds * 1000,
        displaySeconds: plan.durationSeconds,
      };
      this.message = `Leerkalibrierung läuft ${plan.durationSeconds} s.`;
      this._startEmptyCalibrationTimer();
      await this._advance('empty_calibration', 'RUNNING', {
        calibration_kind: 'wifi_d5_d6',
        requested_duration_seconds: plan.durationSeconds,
        start_delay_seconds: plan.leadSeconds,
      });
    } catch (error) {
      this._clearEmptyCalibrationTimer();
      this.emptyCalibrationPlan = null;
      this.message = '';
      this.error = error?.message || 'WiFi-Kalibrierung konnte nicht gestartet werden.';
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _stopEmptyCalibration({ automatic = false } = {}) {
    const plan = this.emptyCalibrationPlan;
    this._clearEmptyCalibrationTimer();
    this.emptyCalibrationPlan = plan ? { ...plan, phase: 'stopping' } : null;
    this.busy = true;
    this.error = '';
    this._render();
    try {
      const response = await apiService.post('/api/v1/classification/calibration/stop', {});
      if (response?.success !== true) throw new Error(response?.error || 'D5/D6-Kalibrierung ist noch nicht bereit.');
      this.emptyCalibrationPlan = null;
      await this._completePhaseAndOpenNext(
        'empty_calibration',
        'train_p01_p09',
        {
          calibration_kind: 'wifi_d5_d6',
          calibration_id: response.calibration_id,
          calibration_source: response.calibration_source || 'captured',
          calibration_context_sha256: response.calibration_context_sha256,
          response,
          ...(plan?.durationSeconds ? { requested_duration_seconds: plan.durationSeconds } : {}),
          ...(automatic ? { automatic_completion: true } : {}),
        },
        { training_points: EXPECTED_POINTS },
      );
    } catch (error) {
      if (plan?.durationSeconds) this.emptyCalibrationPlan = { ...plan, phase: 'ready_to_finish' };
      this.message = '';
      this.error = error?.message || 'WiFi-Kalibrierung konnte nicht abgeschlossen werden.';
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _reuseEmptyCalibration() {
    if (!this.selectedRun) return;
    this.busy = true;
    this.error = '';
    this.message = '';
    this._render();
    try {
      const context = this._calibrationContextRequest();
      if (!context) throw new Error('Bitte zuerst ein Setup-Profil auswählen.');
      const response = await experimentService.reuseCalibration({
        profileId: context.profile_id,
        profileRevisionId: context.profile_revision_id,
      });
      if (response?.success !== true) throw new Error(response?.error || response?.message || 'Gespeicherte Leerkalibrierung konnte nicht verwendet werden.');
      let run = await experimentService.advancePhase(this.selectedRun.id, {
        phase: 'empty_calibration',
        status: 'REUSED',
        payload: {
          calibration_kind: 'wifi_d5_d6',
          calibration_id: response.calibration_id,
          calibration_source: 'reused',
          calibration_context_sha256: response.calibration_context_sha256,
          reused_without_measurement: true,
        },
      });
      run = await experimentService.advancePhase(run.id, {
        phase: 'train_p01_p09',
        status: 'READY',
        payload: { training_points: EXPECTED_POINTS, resumed_after_reused_baseline: true },
      });
      this.selectedRun = run;
      this.message = 'Gespeicherte Leerkalibrierung wiederverwendet. Keine neue Leeraumkalibrierung nötig.';
      await this.refresh({ quiet: true, allowWhileBusy: true });
    } catch (error) {
      this.message = '';
      this.error = error?.message || 'Gespeicherte Leerkalibrierung konnte nicht verwendet werden.';
    } finally {
      this.busy = false;
      this._render();
    }
  }

  _selectedPoint(kind) {
    const select = this.container.querySelector(`[data-occ-select="${kind}-point"]`);
    const point = select?.value || (kind === 'training' ? this.trainingPoint : this.blindPoint);
    if (kind === 'training') this.trainingPoint = point;
    else this.blindPoint = point;
    return point;
  }

  async _recordPoint(kind) {
    if (!this.selectedRun || !this.selectedProfile) return;
    const blindOrder = kind === 'blind' ? this._blindOrder() : null;
    const captureIndex = kind === 'blind' ? this.blindIndex : null;
    const pointId = kind === 'blind'
      ? blindOrder[captureIndex]
      : this._selectedPoint(kind);
    if (kind === 'blind' && !pointId) {
      this.error = 'Blind-Reihenfolge ist nicht verfügbar. Randomisierung erneut ausführen.';
      this._render();
      return;
    }
    if (kind === 'training' && phaseEvents(this.selectedRun, 'train_p01_p09').some((event) => event.payload?.capture_kind === 'training' && event.payload?.capture_completed === true && event.payload?.point_id === pointId)) {
      this.error = `${pointId} wurde bereits vollständig aufgenommen.`;
      this._render();
      return;
    }
    const point = this.profileDraft.points.find((candidate) => candidate.id === pointId);
    this.busy = true;
    try {
      const recordingId = kind === 'blind'
        ? `${this.selectedRun.id}-blind-${String(captureIndex + 1).padStart(2, '0')}-${Date.now()}`
        : `${this.selectedRun.id}-training-${pointId}-${Date.now()}`;
      const response = await apiService.post('/api/v1/recording/start', {
        id: recordingId,
        label: kind === 'blind' ? `blind_capture_${String(captureIndex + 1).padStart(2, '0')}` : `training_${pointId}`,
        max_duration_seconds: 35,
        ...(kind === 'training' ? {
          ground_truth: { occupied: true, person_count: 1, position_m: point?.coordinates_m || null, activity: 'still_position_training' },
        } : {}),
      });
      if (response?.success !== true) throw new Error(response?.error || 'CSI-Aufnahme konnte nicht gestartet werden.');
      this.selectedRun = await experimentService.advancePhase(this.selectedRun.id, {
        phase: kind === 'training' ? 'train_p01_p09' : 'capture',
        status: 'RUNNING',
        payload: {
          capture_kind: kind,
          ...(kind === 'training' ? { point_id: pointId } : { capture_index: captureIndex }),
          recording_id: response.recording_id,
          operator_truth_held_back: kind === 'blind',
        },
      });
      this.message = `${pointId} recording läuft. Stop über den bestehenden Recording-Stop oder nach 35 s automatisch.`;
      await this.refresh({ quiet: true, allowWhileBusy: true });
    } catch (error) {
      this.message = '';
      this.error = error?.message || 'CSI-Aufnahme konnte nicht gestartet werden.';
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _stopCurrentRecording() {
    this.busy = true;
    try {
      const response = await apiService.post('/api/v1/recording/stop', {});
      if (response?.success !== true || response?.incomplete === true) {
        throw new Error(response?.writer_error || response?.error || 'CSI-Aufnahme ist unvollständig.');
      }
      const phase = this.selectedRun?.workflow?.current_phase;
      const kind = phase === 'train_p01_p09' ? 'training' : 'blind';
      const payload = eventPayload(this.selectedRun, phase, 'recording_id');
      const captureKind = kind;
      this.selectedRun = await experimentService.advancePhase(this.selectedRun.id, {
        phase,
        status: 'RUNNING',
        payload: {
          capture_kind: captureKind,
          capture_completed: true,
          ...(kind === 'training'
            ? { point_id: payload?.point_id || this.trainingPoint }
            : { capture_index: payload?.capture_index ?? this.blindIndex }),
          recording_id: response.recording_id,
          frames_written: response.frames_written,
          dropped_frames: response.dropped_frames,
        },
      });
      if (kind === 'blind') this.blindIndex = Math.min(this.blindIndex + 1, 18);
      this.message = `Aufnahme ${response.recording_id} abgeschlossen und als Artefakt katalogisiert.`;
      await this.refresh({ quiet: true, allowWhileBusy: true });
    } catch (error) {
      this.message = '';
      this.error = error?.message || 'CSI-Aufnahme konnte nicht abgeschlossen werden.';
      this.status = await experimentService.getControlCenterStatus().catch(() => this.status);
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _randomizeBlindPositions() {
    if (!this.selectedRun?.workflow) return;
    const seed = this.selectedRun.workflow.blind_seed;
    const order = shuffledBlindOrder(seed);
    this.blindOrder = order;
    this.blindIndex = 0;
    this.blindPoint = order[0];
    await this._completePhaseAndOpenNext('randomize_blind_positions', 'capture', {
      order_commitment: blindOrderCommitment(order),
      order_length: order.length,
      truth_separate: true,
    }, {
      expected_capture_count: order.length,
      truth_separate: true,
    });
  }

  _blindOrder() {
    if (!this.selectedRun?.workflow) return [];
    if (this.blindOrder.length !== 18) {
      this.blindOrder = shuffledBlindOrder(this.selectedRun.workflow.blind_seed);
    }
    const completed = phaseEvents(this.selectedRun, 'capture')
      .filter((event) => event.payload?.capture_kind === 'blind' && event.payload?.capture_completed === true).length;
    this.blindIndex = Math.min(Math.max(this.blindIndex, completed), 18);
    this.blindPoint = this.blindOrder[this.blindIndex] || this.blindPoint;
    return this.blindOrder;
  }

  async _writeReport() {
    this.busy = true;
    try {
      this.selectedRun = await experimentService.writeReport(this.selectedRun.id);
      this.message = 'Workflow-Report geschrieben. Validierung bleibt UNVALIDATED.';
      await this.refresh({ quiet: true, allowWhileBusy: true });
    } catch (error) {
      this.message = '';
      this.error = error?.message || 'Report konnte nicht geschrieben werden.';
    } finally {
      this.busy = false;
      this._render();
    }
  }

  async _registerArtifact(kind, phase) {
    const input = this.container.querySelector(`[data-occ-artifact-input="${kind}"]`);
    const relativePath = String(input?.value || '').trim();
    if (!relativePath || !this.selectedRun) {
      this.message = '';
      this.error = 'Bitte einen relativen Artefaktpfad unter data/ angeben.';
      this._render();
      return;
    }
    this.busy = true;
    this.error = '';
    try {
      let run = await experimentService.registerArtifact(this.selectedRun.id, { kind, relativePath });
      const artifact = (run.artifacts || []).find((candidate) => candidate.kind === kind);
      run = await experimentService.advancePhase(run.id, {
        phase,
        status: 'READY',
        payload: {
          artifact_registered: true,
          artifact_kind: kind,
          relative_path: relativePath,
          sha256: artifact?.sha256 || null,
        },
      });
      this.selectedRun = run;
      this.artifactPaths[kind] = '';
      this.message = `${kind} geprüft und mit SHA-256 an den Run gebunden.`;
      await this.refresh({ quiet: true, allowWhileBusy: true });
    } catch (error) {
      this.message = '';
      this.error = error?.message || `${kind}-Artefakt konnte nicht registriert werden.`;
    } finally {
      this.busy = false;
      this._render();
    }
  }
}

export default ObservatoryControlCenter;
