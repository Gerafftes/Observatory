// Observatory Control Center experiment API.
// The service intentionally exposes only metadata and software replay calls;
// hardware capture, node configuration, and mmWave control stay elsewhere.

import { apiService } from './api.service.js';

const EXPERIMENTS_ENDPOINT = '/api/v1/experiments';

export class ExperimentService {
  async getStatus() {
    return apiService.get(`${EXPERIMENTS_ENDPOINT}/status`);
  }

  async listRuns(limit = 50) {
    const payload = await apiService.get(`${EXPERIMENTS_ENDPOINT}/runs`, { limit });
    return Array.isArray(payload?.runs) ? payload.runs : [];
  }

  async createRun({ label, fixtureId }) {
    return apiService.post(`${EXPERIMENTS_ENDPOINT}/runs`, {
      label,
      fixture_id: fixtureId,
    });
  }

  async getRun(id) {
    return apiService.get(`${EXPERIMENTS_ENDPOINT}/runs/${encodeURIComponent(id)}`);
  }

  async replayRun(id) {
    return apiService.post(
      `${EXPERIMENTS_ENDPOINT}/runs/${encodeURIComponent(id)}/replay`,
      {},
    );
  }

  async listProfiles() {
    const payload = await apiService.get(`${EXPERIMENTS_ENDPOINT}/setup-profiles`);
    return Array.isArray(payload?.profiles) ? payload.profiles : [];
  }

  async createProfile({ label, document }) {
    return apiService.post(`${EXPERIMENTS_ENDPOINT}/setup-profiles`, { label, document });
  }

  async updateProfile(id, { label, document }) {
    return apiService.put(
      `${EXPERIMENTS_ENDPOINT}/setup-profiles/${encodeURIComponent(id)}`,
      { label, document },
    );
  }

  async createWorkflow({ label, profileId, datasetVersion = 'unassigned', firmwareVersion = 'unassigned', blindSeed }) {
    return apiService.post(`${EXPERIMENTS_ENDPOINT}/workflows`, {
      label,
      profile_id: profileId,
      dataset_version: datasetVersion,
      firmware_version: firmwareVersion,
      ...(Number.isSafeInteger(blindSeed) ? { blind_seed: blindSeed } : {}),
    });
  }

  async advancePhase(id, { phase, status, payload = {} }) {
    return apiService.post(
      `${EXPERIMENTS_ENDPOINT}/runs/${encodeURIComponent(id)}/phase`,
      { phase, status, payload },
    );
  }

  async registerArtifact(id, { kind, relativePath }) {
    return apiService.post(
      `${EXPERIMENTS_ENDPOINT}/runs/${encodeURIComponent(id)}/artifacts`,
      { kind, relative_path: relativePath },
    );
  }

  async writeReport(id) {
    return apiService.post(`${EXPERIMENTS_ENDPOINT}/runs/${encodeURIComponent(id)}/report`, {});
  }

  async getReport(id) {
    return apiService.get(`${EXPERIMENTS_ENDPOINT}/runs/${encodeURIComponent(id)}/report`);
  }

  async exportRun(id, format = 'json') {
    const path = `${EXPERIMENTS_ENDPOINT}/runs/${encodeURIComponent(id)}/export`;
    if (format === 'csv') {
      return apiService.getText(path, { format: 'csv' });
    }
    return apiService.get(path);
  }

  async getControlCenterStatus() {
    return apiService.get('/api/v1/control-center/status');
  }

  async getBenchmarkCatalog() {
    return apiService.get('/api/v1/benchmarks/catalog');
  }
}

export const experimentService = new ExperimentService();
