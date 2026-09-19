// Dashboard Tab Component

import { healthService } from '../services/health.service.js';
import { poseService } from '../services/pose.service.js';
import { sensingService } from '../services/sensing.service.js';

const LIVE_DATA_SOURCES = new Set(['live', 'server-simulated']);

export class DashboardTab {
  constructor(containerElement) {
    this.container = containerElement;
    this.healthSubscription = null;
    this.statsInterval = null;
  }

  async init() {
    await this.loadInitialData();
    this.startMonitoring();
  }

  async loadInitialData() {
    try {
      const info = await healthService.getApiInfo();
      this.updateApiInfo(info);
    } catch (error) {
      console.log('Dashboard: API info not available');
    }
  }

  startMonitoring() {
    this.healthSubscription = healthService.subscribeToHealth((health) => {
      this.updateHealthStatus(health);
    });

    this._sensingUnsub = sensingService.onStateChange(() => {
      this.updateDataSourceIndicator();
    });
    this._sensingDataUnsub = sensingService.onData(() => {
      this.updateDataSourceIndicator();
    });
    this.updateDataSourceIndicator();

    this.statsInterval = setInterval(() => {
      this.updateLiveStats();
    }, 5000);

    healthService.startHealthMonitoring(30000);
  }

  updateDataSourceIndicator() {
    const element = this.container.querySelector('#dashboard-datasource');
    if (!element) return;

    const statusText = element.querySelector('.status-text');
    const statusMessage = element.querySelector('.status-message');
    const config = {
      live: {
        text: 'ESP32',
        status: 'healthy',
        message: 'Real hardware connected',
      },
      'server-simulated': {
        text: 'SIMULATED',
        status: 'warning',
        message: 'Server is providing synthetic data',
      },
      'server-offline': {
        text: 'OFFLINE',
        status: 'unhealthy',
        message: 'Server reachable, no fresh hardware frame',
      },
      reconnecting: {
        text: 'RECONNECTING',
        status: 'degraded',
        message: 'Attempting to connect…',
      },
    };
    const state = config[sensingService.dataSource] || config.reconnecting;

    element.className = `component-status status-${state.status}`;
    if (statusText) statusText.textContent = state.text;
    if (statusMessage) statusMessage.textContent = state.message;
  }

  updateApiInfo(info) {
    const versionElement = this.container.querySelector('.api-version');
    if (versionElement && info?.version) {
      versionElement.textContent = `v${info.version}`;
    }
  }

  updateHealthStatus(health) {
    if (!health) return;

    const overallStatus = this.container.querySelector('.overall-health');
    if (overallStatus && health.status) {
      overallStatus.className = `overall-health status-${health.status}`;
      overallStatus.textContent = health.status.toUpperCase();
    }

    Object.entries(health.components || {}).forEach(([component, status]) => {
      this.updateComponentStatus(component, status);
    });
  }

  updateComponentStatus(component, status) {
    const componentMap = {
      pose: 'inference',
      stream: 'streaming',
      hardware: 'hardware',
    };
    const element = this.container.querySelector(
      `[data-component="${componentMap[component] || component}"]`,
    );
    if (!element) return;

    const state = status?.status || 'unknown';
    element.className = `component-status status-${state}`;
    const statusText = element.querySelector('.status-text');
    const statusMessage = element.querySelector('.status-message');
    if (statusText) statusText.textContent = state.toUpperCase();
    if (statusMessage) statusMessage.textContent = status?.message || '';
  }

  async updateLiveStats() {
    if (!LIVE_DATA_SOURCES.has(sensingService.dataSource)) {
      this.clearLiveStats();
      return;
    }

    try {
      const [currentPose, stats] = await Promise.all([
        poseService.getCurrentPose(),
        poseService.getStats(1),
      ]);
      this.updatePoseStats(currentPose);
      this.updateStats(stats);
    } catch (error) {
      this.clearLiveStats();
      console.error('Failed to update live stats:', error);
    }
  }

  updatePoseStats(poseData) {
    if (!poseData) {
      this.clearLiveStats();
      return;
    }

    const persons = Array.isArray(poseData.persons) ? poseData.persons : null;
    const personCount = this.container.querySelector('.person-count');
    if (personCount) {
      const count = persons ? persons.length : poseData.total_persons;
      personCount.textContent = Number.isFinite(count) ? String(count) : '--';
    }

    const avgConfidence = this.container.querySelector('.avg-confidence');
    if (avgConfidence) {
      const confidences = (persons || [])
        .map((person) => person.confidence)
        .filter((confidence) => typeof confidence === 'number' && Number.isFinite(confidence));
      avgConfidence.textContent = confidences.length
        ? `${(confidences.reduce((sum, confidence) => sum + confidence, 0) / confidences.length * 100).toFixed(1)}%`
        : '--';
    }
  }

  updateStats(stats) {
    const detectionCount = this.container.querySelector('.detection-count');
    if (!detectionCount) return;

    detectionCount.textContent = Number.isFinite(stats?.total_detections)
      ? this.formatNumber(stats.total_detections)
      : '--';
  }

  clearLiveStats() {
    ['.person-count', '.avg-confidence', '.detection-count'].forEach((selector) => {
      const element = this.container.querySelector(selector);
      if (element) element.textContent = '--';
    });
  }

  formatNumber(number) {
    if (number >= 1000000) return `${(number / 1000000).toFixed(1)}M`;
    if (number >= 1000) return `${(number / 1000).toFixed(1)}K`;
    return String(number);
  }

  showError(message) {
    const errorContainer = this.container.querySelector('.error-container');
    if (!errorContainer) return;

    errorContainer.textContent = message;
    errorContainer.style.display = 'block';
    setTimeout(() => {
      errorContainer.style.display = 'none';
    }, 5000);
  }

  dispose() {
    this.healthSubscription?.();
    this._sensingUnsub?.();
    this._sensingDataUnsub?.();
    if (this.statsInterval) clearInterval(this.statsInterval);
    healthService.stopHealthMonitoring();
  }
}
