// Hardware Tab Component

import { API_CONFIG } from '../config/api.config.js';
import { apiService } from '../services/api.service.js';
import { deviceColor, receiverIdentity } from '../device-identity.js';
import { LocalControlPanel } from './LocalControlPanel.js';

const NODE_REFRESH_INTERVAL_MS = 5000;

function finiteNumber(value) {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

export class HardwareTab {
  constructor(containerElement) {
    this.container = containerElement;
    this.controlPanel = null;
    this.nodeRefreshInterval = null;
    this.nodeRequestInFlight = false;
  }

  init() {
    const helperContainer = this.container?.querySelector('#localControlHelper');
    if (helperContainer) {
      this.controlPanel = new LocalControlPanel(helperContainer);
      this.controlPanel.mount();
    }

    this.refreshNodes();
    this.nodeRefreshInterval = setInterval(
      () => this.refreshNodes(),
      NODE_REFRESH_INTERVAL_MS,
    );
  }

  async refreshNodes() {
    if (!this.container || this.nodeRequestInFlight) return;
    this.nodeRequestInFlight = true;
    try {
      const payload = await apiService.get(API_CONFIG.ENDPOINTS.NODES);
      this.renderNodes(payload);
    } catch (error) {
      this.renderNodes({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      this.nodeRequestInFlight = false;
    }
  }

  renderNodes(payload = {}) {
    const status = this.container?.querySelector('#hardware-node-status');
    const list = this.container?.querySelector('#hardware-node-list');
    if (!status || !list) return;

    list.replaceChildren();
    if (payload.error) {
      status.textContent = 'Node-Daten nicht verfügbar';
      const message = document.createElement('p');
      message.className = 'hardware-node-empty';
      message.textContent = payload.error;
      list.appendChild(message);
      return;
    }

    const nodes = Array.isArray(payload.nodes) ? payload.nodes : [];
    status.textContent = nodes.length
      ? `${nodes.length} Node${nodes.length === 1 ? '' : 's'} vom Sensing-Server gemeldet`
      : 'Keine Nodes vom Sensing-Server gemeldet';

    if (nodes.length === 0) {
      const message = document.createElement('p');
      message.className = 'hardware-node-empty';
      message.textContent = 'Es liegen noch keine empfangenen Node-Daten vor.';
      list.appendChild(message);
      return;
    }

    nodes.forEach((node) => list.appendChild(this.createNodeRow(node)));
  }

  createNodeRow(node) {
    const row = document.createElement('article');
    row.className = 'hardware-node-row';

    const title = document.createElement('div');
    title.className = 'hardware-node-title';

    const identity = receiverIdentity(node.node_id);
    const label = document.createElement('span');
    label.className = 'device-identity-label';
    label.style.setProperty('--device-color', deviceColor(identity?.id));
    const swatch = document.createElement('i');
    swatch.setAttribute('aria-hidden', 'true');
    label.append(swatch, identity?.id || node.display_name || 'RX?');
    title.appendChild(label);

    const state = document.createElement('span');
    state.className = `hardware-node-state ${node.status === 'active' ? 'is-active' : 'is-stale'}`;
    state.textContent = String(node.status || 'unknown').toUpperCase();
    title.appendChild(state);

    const details = document.createElement('div');
    details.className = 'hardware-node-details';
    details.textContent = [
      `RSSI ${this.formatMetric(node.rssi_dbm, ' dBm', 1)}`,
      `Frames ${this.formatMetric(node.frame_rate_hz, ' Hz', 1)}`,
      `Verlust ${this.formatMetric(node.packet_loss_percent, '%', 1)}`,
      `Zuletzt ${this.formatMetric(node.last_seen_ms, ' ms', 0)}`,
    ].join(' · ');

    row.append(title, details);
    return row;
  }

  formatMetric(value, suffix, decimals) {
    const number = finiteNumber(value);
    return number === null ? '--' : `${number.toFixed(decimals)}${suffix}`;
  }

  dispose() {
    if (this.nodeRefreshInterval) {
      clearInterval(this.nodeRefreshInterval);
      this.nodeRefreshInterval = null;
    }
    this.controlPanel?.dispose();
    this.controlPanel = null;
  }
}
