/**
 * SensingTab — Live WiFi Sensing Visualization
 *
 * Connects to the sensing WebSocket service and renders:
 *   1. A 3D Gaussian-splat signal field (via gaussian-splats.js)
 *   2. An overlay HUD with real-time metrics (RSSI, variance, bands, classification)
 */

import { sensingService } from '../services/sensing.service.js';
import {
  GaussianSplatRenderer,
  positionEstimateViewModel,
} from './gaussian-splats.js';
import { ObservatoryControlCenter } from './ObservatoryControlCenter.js';
import { MmwaveCalibrationAssistant } from './MmwaveCalibrationAssistant.js';

export class SensingTab {
  /** @param {HTMLElement} container - the #sensing section element */
  constructor(container) {
    this.container = container;
    this.splatRenderer = null;
    this._unsubData = null;
    this._unsubState = null;
    this._resizeObserver = null;
    this._threeLoaded = false;
    this._initPromise = null;
    this._initialized = false;
    this._disposed = false;
    this._lifecycleGeneration = 0;
    this._serviceStarted = false;
    this.controlCenter = null;
    this.mmwaveAssistant = null;
  }

  init() {
    if (this._initialized) {
      return Promise.resolve();
    }
    if (this._initPromise) {
      return this._initPromise;
    }

    this._disposed = false;
    const generation = ++this._lifecycleGeneration;
    const initPromise = this._initialize(generation).finally(() => {
      if (this._initPromise === initPromise) {
        this._initPromise = null;
      }
    });
    this._initPromise = initPromise;
    return initPromise;
  }

  async _initialize(generation) {
    this._buildDOM();
    await this._loadThree();
    if (this._disposed || generation !== this._lifecycleGeneration) {
      return;
    }
    this._initSplatRenderer();
    this._connectService();
    this._setupResize();
    this._initialized = true;
  }

  // ---- DOM construction --------------------------------------------------

  _buildDOM() {
    this.container.innerHTML = `
      <h2>WiFi Sensing</h2>

      <!-- Data-source status banner — updated by _onStateChange -->
      <div id="sensingSourceBanner" class="sensing-source-banner sensing-source-reconnecting"
           role="status" aria-live="polite">
        VERBINDE …
      </div>

      <div id="observatoryControlCenter"></div>

      <div id="mmwaveCalibrationAssistant"></div>

      <div class="sensing-layout">
        <!-- 3D viewport -->
        <div class="sensing-viewport" id="sensingViewport">
          <div class="sensing-loading">Loading 3D engine...</div>
        </div>

        <!-- Side panel -->
        <div class="sensing-panel">
          <!-- Connection -->
          <div class="sensing-card">
            <div class="sensing-card-title">Connection</div>
            <div class="sensing-connection">
              <span class="sensing-dot" id="sensingDot"></span>
              <span id="sensingState">Verbinde …</span>
              <span class="sensing-source" id="sensingSource"></span>
            </div>
          </div>

          <!-- RSSI -->
          <div class="sensing-card">
            <div class="sensing-card-title">RSSI</div>
            <div class="sensing-big-value" id="sensingRssi">-- dBm</div>
            <canvas id="sensingSparkline" width="200" height="40"></canvas>
          </div>

          <!-- Signal Features -->
          <div class="sensing-card">
            <div class="sensing-card-title">Signal</div>
            <div class="sensing-meters">
              <div class="sensing-meter">
                <label>Variance</label>
                <div class="sensing-bar"><div class="sensing-bar-fill" id="barVariance"></div></div>
                <span class="sensing-meter-val" id="valVariance">0</span>
              </div>
              <div class="sensing-meter">
                <label>Bewegung</label>
                <div class="sensing-bar"><div class="sensing-bar-fill motion" id="barMotion"></div></div>
                <span class="sensing-meter-val" id="valMotion">0</span>
              </div>
              <div class="sensing-meter">
                <label>Atmung</label>
                <div class="sensing-bar"><div class="sensing-bar-fill breath" id="barBreath"></div></div>
                <span class="sensing-meter-val" id="valBreath">0</span>
              </div>
              <div class="sensing-meter">
                <label>Spektrum</label>
                <div class="sensing-bar"><div class="sensing-bar-fill spectral" id="barSpectral"></div></div>
                <span class="sensing-meter-val" id="valSpectral">0</span>
              </div>
            </div>
          </div>

          <!-- Classification -->
          <div class="sensing-card">
            <div class="sensing-card-title">Classification</div>
            <div class="sensing-classification" id="sensingClassification">
              <div class="sensing-class-label" id="classLabel">ABSENT</div>
              <div class="sensing-confidence">
                <label>Confidence</label>
                <div class="sensing-bar"><div class="sensing-bar-fill confidence" id="barConfidence"></div></div>
                <span class="sensing-meter-val" id="valConfidence">0%</span>
              </div>
            </div>
          </div>

          <!-- Validated discrete position estimate -->
          <div class="sensing-card">
            <div class="sensing-card-title">Position</div>
            <div class="sensing-localization">
              <div class="sensing-localization-status unknown" id="localizationStatus">
                UNKNOWN
              </div>
              <div class="sensing-detail-row">
                <span>Punkt</span><span id="positionPointId">--</span>
              </div>
              <div class="sensing-detail-row">
                <span>Koordinaten</span><span id="positionCoordinates">--</span>
              </div>
              <div class="sensing-detail-row">
                <span>Hinweis</span><span id="positionReason">Keine geprüfte Position</span>
              </div>
            </div>
          </div>

          <!-- Setup info -->
          <div class="sensing-card">
            <div class="sensing-card-title">Daten</div>
            <p class="sensing-about-text">
              CSI von <strong><span id="sensingNodeCount">0</span> ESP32</strong>: Präsenz, Atmung und Bewegung.
              Die Körperwolke erscheint nur bei geprüfter Position. Das Farbfeld zeigt Linkaktivität,
              keine Personenposition.
            </p>
          </div>

          <!-- Node Status -->
          <div class="sensing-card" id="sensingNodeCards">
            <div class="sensing-card-title">NODES</div>
            <div id="nodeStatusContainer"></div>
          </div>

          <!-- Extra info -->
          <div class="sensing-card">
            <div class="sensing-card-title">Details</div>
            <div class="sensing-details">
              <div class="sensing-detail-row">
                <span>Frequenz</span><span id="valDomFreq">0 Hz</span>
              </div>
              <div class="sensing-detail-row">
                <span>Sprünge</span><span id="valChangePoints">0</span>
              </div>
              <div class="sensing-detail-row">
                <span>Rate</span><span id="valSampleRate">--</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    `;
    this.controlCenter = new ObservatoryControlCenter(
      this.container.querySelector('#observatoryControlCenter'),
    );
    this.controlCenter.mount();

    this.mmwaveAssistant = new MmwaveCalibrationAssistant(
      this.container.querySelector('#mmwaveCalibrationAssistant'),
    );
    this.mmwaveAssistant.mount();
  }

  // ---- Three.js loading --------------------------------------------------

  async _loadThree() {
    if (window.THREE) {
      this._threeLoaded = true;
      return;
    }

    return new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = 'https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js';
      script.onload = () => {
        this._threeLoaded = true;
        resolve();
      };
      script.onerror = () => reject(new Error('Failed to load Three.js'));
      document.head.appendChild(script);
    });
  }

  // ---- Splat renderer ----------------------------------------------------

  _initSplatRenderer() {
    const viewport = this.container.querySelector('#sensingViewport');
    if (!viewport) return;

    // Remove loading message
    viewport.innerHTML = '';

    try {
      this.splatRenderer = new GaussianSplatRenderer(viewport, {
        width: viewport.clientWidth,
        height: viewport.clientHeight || 500,
      });
      const fieldNotice = document.createElement('div');
      fieldNotice.className = 'sensing-field-notice';
      fieldNotice.textContent = 'Link-Heatmap · keine Position';
      viewport.appendChild(fieldNotice);
    } catch (e) {
      console.error('[SensingTab] Failed to init splat renderer:', e);
      viewport.innerHTML = '<div class="sensing-loading">3D rendering unavailable</div>';
    }
  }

  // ---- Service connection ------------------------------------------------

  _connectService() {
    sensingService.start();
    this._serviceStarted = true;

    this._unsubData = sensingService.onData((data) => this._onSensingData(data));
    this._unsubState = sensingService.onStateChange((state) => this._onStateChange(state));
  }

  _onSensingData(data) {
    // Update 3D view
    if (this.splatRenderer) {
      this.splatRenderer.update(data);
    }

    // Update HUD
    this._updateHUD(data);

    // Update per-node panels
    this._updateNodePanels(data);
  }

  _onStateChange(state) {
    const dot    = this.container.querySelector('#sensingDot');
    const text   = this.container.querySelector('#sensingState');
    const banner = this.container.querySelector('#sensingSourceBanner');

    if (dot && text) {
      const stateLabels = {
        disconnected: 'Getrennt',
        connecting:   'Verbinde …',
        connected:    'Verbunden',
        reconnecting: 'Verbinde …',
        simulated:    'Simulation',
      };
      dot.className = 'sensing-dot ' + state;
      text.textContent = stateLabels[state] || state;
    }

    if (banner) {
      // Map the service's dataSource to banner text and CSS modifier class.
      const dataSource = sensingService.dataSource;
      const bannerConfig = {
        'live':              { text: 'LIVE · ESP32',             cls: 'sensing-source-live' },
        'server-simulated':  { text: 'SIMULATION · SERVER',       cls: 'sensing-source-server-sim' },
        'reconnecting':      { text: 'VERBINDE …',                cls: 'sensing-source-reconnecting' },
        'simulated':         { text: 'OFFLINE · SIMULATION',      cls: 'sensing-source-simulated' },
      };
      const cfg = bannerConfig[dataSource] || bannerConfig.reconnecting;
      banner.textContent = cfg.text;
      banner.className = 'sensing-source-banner ' + cfg.cls;
    }

    if (['disconnected', 'connecting', 'reconnecting'].includes(state)) {
      this._invalidateLiveReadout();
    }
  }

  // ---- HUD update --------------------------------------------------------

  _invalidateLiveReadout() {
    this.splatRenderer?.invalidatePositionEstimate('stale');

    this._setText('sensingRssi', '-- dBm');
    this._setText('sensingSource', '');
    this._setText('sensingNodeCount', '0');
    this._setBar('barVariance', 0, 1, 'valVariance', '--');
    this._setBar('barMotion', 0, 1, 'valMotion', '--');
    this._setBar('barBreath', 0, 1, 'valBreath', '--');
    this._setBar('barSpectral', 0, 1, 'valSpectral', '--');
    this._setBar('barConfidence', 0, 1, 'valConfidence', '0%');

    const label = this.container.querySelector('#classLabel');
    if (label) {
      label.textContent = 'UNKNOWN';
      label.className = 'sensing-class-label unknown';
    }

    this._renderPositionEstimate({
      source: 'esp32',
      position_estimate: {
        state: 'stale',
        reason: 'Verbindung weg. Position gelöscht.',
      },
    });
    this._updateNodePanels({});
  }

  _updateHUD(data) {
    const f = data.features || {};
    const c = data.classification || {};

    // Node count
    const nodeCount = (data.nodes || []).length;
    const countEl = this.container.querySelector('#sensingNodeCount');
    if (countEl) countEl.textContent = String(nodeCount);

    // RSSI
    this._setText('sensingRssi', `${(f.mean_rssi || -80).toFixed(1)} dBm`);
    this._setText('sensingSource', data.source || '');

    // Bars (scale to 0-100%)
    this._setBar('barVariance', f.variance, 10, 'valVariance', f.variance);
    this._setBar('barMotion', f.motion_band_power, 0.5, 'valMotion', f.motion_band_power);
    this._setBar('barBreath', f.breathing_band_power, 0.3, 'valBreath', f.breathing_band_power);
    this._setBar('barSpectral', f.spectral_power, 2.0, 'valSpectral', f.spectral_power);

    // Classification
    const label = this.container.querySelector('#classLabel');
    if (label) {
      const level = (c.motion_level || 'absent').toUpperCase();
      label.textContent = level;
      label.className = 'sensing-class-label ' + (c.motion_level || 'absent');
    }

    const confPct = ((c.confidence || 0) * 100).toFixed(0);
    this._setBar('barConfidence', c.confidence, 1.0, 'valConfidence', confPct + '%');

    this._renderPositionEstimate(data);

    // Details
    this._setText('valDomFreq', (f.dominant_freq_hz || 0).toFixed(3) + ' Hz');
    this._setText('valChangePoints', String(f.change_points || 0));
    const srcLabel = (data.source === 'simulated' || data.source === 'simulate') ? 'sim' : data.source || 'live';
    this._setText('valSampleRate', srcLabel);

    // Sparkline
    this._drawSparkline();
  }

  _renderPositionEstimate(data) {
    const rawEstimate = positionEstimateViewModel(data);
    const estimate =
      rawEstimate.coordinates && data?.classification?.presence !== true
        ? {
            state: 'unknown',
            label: 'NO PRESENCE',
            pointId: null,
            coordinates: null,
            reason: 'Keine Präsenz. Position gelöscht.',
          }
        : rawEstimate;
    const statusEl = this.container.querySelector('#localizationStatus');
    if (statusEl) {
      statusEl.textContent = estimate.label;
      statusEl.className = `sensing-localization-status ${estimate.state}`;
    }
    this._setText('positionPointId', estimate.pointId || '--');
    this._setText(
      'positionCoordinates',
      estimate.coordinates
        ? `x ${estimate.coordinates[0].toFixed(2)} m · y ${estimate.coordinates[1].toFixed(2)} m · z ${estimate.coordinates[2].toFixed(2)} m`
        : '--'
    );
    this._setText('positionReason', estimate.reason || '--');
  }

  _setText(id, text) {
    const el = this.container.querySelector('#' + id);
    if (el) el.textContent = text;
  }

  _setBar(barId, value, maxVal, valId, displayVal) {
    const bar = this.container.querySelector('#' + barId);
    if (bar) {
      const pct = Math.min(100, Math.max(0, ((value || 0) / maxVal) * 100));
      bar.style.width = pct + '%';
    }
    if (valId && displayVal != null) {
      const el = this.container.querySelector('#' + valId);
      if (el) el.textContent = typeof displayVal === 'number' ? displayVal.toFixed(3) : displayVal;
    }
  }

  _drawSparkline() {
    const canvas = this.container.querySelector('#sensingSparkline');
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const history = sensingService.getRssiHistory();
    if (history.length < 2) return;

    const w = canvas.width;
    const h = canvas.height;
    ctx.clearRect(0, 0, w, h);

    const min = Math.min(...history) - 2;
    const max = Math.max(...history) + 2;
    const range = max - min || 1;

    ctx.beginPath();
    ctx.strokeStyle = '#32b8c6';
    ctx.lineWidth = 1.5;

    for (let i = 0; i < history.length; i++) {
      const x = (i / (history.length - 1)) * w;
      const y = h - ((history[i] - min) / range) * h;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
  }

  // ---- Per-node panels ---------------------------------------------------

  _updateNodePanels(data) {
    const container = this.container.querySelector('#nodeStatusContainer');
    if (!container) return;
    const nodeFeatures = data.node_features || [];
    if (nodeFeatures.length === 0) {
      container.textContent = '';
      const msg = document.createElement('div');
      msg.style.cssText = 'color:#888;font-size:12px;padding:8px;';
      msg.textContent = 'Keine Nodes';
      container.appendChild(msg);
      return;
    }
    const NODE_COLORS = ['#00ccff', '#ff6600', '#00ff88', '#ff00cc', '#ffcc00', '#8800ff', '#00ffcc', '#ff0044'];
    container.textContent = '';
    for (const nf of nodeFeatures) {
      const color = NODE_COLORS[nf.node_id % NODE_COLORS.length];
      const statusColor = nf.stale ? '#888' : '#0f0';

      const row = document.createElement('div');
      row.style.cssText = `display:flex;align-items:center;gap:8px;padding:6px 8px;margin-bottom:4px;background:rgba(255,255,255,0.03);border-radius:6px;border-left:3px solid ${color};`;

      const idCol = document.createElement('div');
      idCol.style.minWidth = '50px';
      const nameEl = document.createElement('div');
      nameEl.style.cssText = `font-size:11px;font-weight:600;color:${color};`;
      nameEl.textContent = 'Node ' + nf.node_id;
      const statusEl = document.createElement('div');
      statusEl.style.cssText = `font-size:9px;color:${statusColor};`;
      statusEl.textContent = nf.stale ? 'STALE' : 'ACTIVE';
      idCol.appendChild(nameEl);
      idCol.appendChild(statusEl);

      const metricsCol = document.createElement('div');
      metricsCol.style.cssText = 'flex:1;font-size:10px;color:#aaa;';
      const d6Ratio = Number(nf.d6_fingerprint?.anomaly_ratio);
      const d6Text = Number.isFinite(d6Ratio) ? ` · D6 ${d6Ratio.toFixed(2)}×` : ' · D6 --';
      metricsCol.textContent =
        (nf.rssi_dbm || -80).toFixed(0) +
        ' dBm · var ' +
        (nf.features?.variance || 0).toFixed(1) +
        d6Text;

      const classCol = document.createElement('div');
      classCol.style.cssText = 'font-size:10px;font-weight:600;color:#ccc;';
      const motion = (nf.classification?.motion_level || 'absent').toUpperCase();
      const conf = ((nf.classification?.confidence || 0) * 100).toFixed(0);
      classCol.textContent = motion + ' ' + conf + '%';

      row.appendChild(idCol);
      row.appendChild(metricsCol);
      row.appendChild(classCol);
      container.appendChild(row);
    }
  }

  // ---- Resize ------------------------------------------------------------

  _setupResize() {
    const viewport = this.container.querySelector('#sensingViewport');
    if (!viewport || !window.ResizeObserver) return;

    this._resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (this.splatRenderer) {
          this.splatRenderer.resize(entry.contentRect.width, entry.contentRect.height);
        }
      }
    });
    this._resizeObserver.observe(viewport);
  }

  // ---- Cleanup -----------------------------------------------------------

  dispose() {
    this._disposed = true;
    this._initialized = false;
    this._lifecycleGeneration += 1;
    this._initPromise = null;

    if (this._unsubData) {
      this._unsubData();
      this._unsubData = null;
    }
    if (this._unsubState) {
      this._unsubState();
      this._unsubState = null;
    }
    if (this._resizeObserver) {
      this._resizeObserver.disconnect();
      this._resizeObserver = null;
    }
    if (this.splatRenderer) {
      this.splatRenderer.dispose();
      this.splatRenderer = null;
    }
    if (this.mmwaveAssistant) {
      this.mmwaveAssistant.dispose();
      this.mmwaveAssistant = null;
    }
    if (this.controlCenter) {
      this.controlCenter.dispose();
      this.controlCenter = null;
    }
    if (this._serviceStarted) {
      sensingService.stop();
      this._serviceStarted = false;
    }
  }
}
