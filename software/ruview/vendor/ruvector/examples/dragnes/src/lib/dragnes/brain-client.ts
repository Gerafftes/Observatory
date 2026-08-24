/**
 * DrAgnes Brain Integration Client
 *
 * Connects to the pi.ruv.io collective intelligence brain for:
 *   - Sharing de-identified lesion classifications
 *   - Searching similar cases
 *   - Enriching diagnoses with PubMed literature
 *   - Syncing LoRA model updates
 *
 * All data is stripped of PHI and has differential privacy noise applied
 * before leaving the device.
 */

import type { LesionClass, BodyLocation, WitnessChain } from "./types";
import { createWitnessChain } from "./witness";
import { OfflineQueue } from "./offline-queue";

const BRAIN_BASE_URL = "https://pi.ruv.io";
const DRAGNES_TAG = "dragnes";
const DEFAULT_EPSILON = 1.0;
const FETCH_TIMEOUT_MS = 10_000;

/** Metadata accompanying a brain contribution */
export interface DiagnosisMetadata {
	/** Predicted lesion class */
	lesionClass: LesionClass;
	/** Body location of the lesion */
	bodyLocation: BodyLocation;
	/** Model version that produced the classification */
	modelVersion: string;
	/** Confidence score [0, 1] */
	confidence: number;
	/** Per-class probabilities */
	probabilities: number[];
	/** Whether a clinician confirmed the diagnosis */
	confirmed: boolean;
	/** Brain epoch at time of classification */
	brainEpoch?: number;
}

/** A similar case returned from brain search */
export interface SimilarCase {
	/** Brain memory ID */
	id: string;
	/** Similarity score [0, 1] */
	similarity: number;
	/** Lesion class of the similar case */
	lesionClass: string;
	/** Body location */
	bodyLocation: string;
	/** Confidence of the original classification */
	confidence: number;
	/** Whether it was clinician-confirmed */
	confirmed: boolean;
}

/** Literature reference from brain + PubMed context */
export interface LiteratureReference {
	/** Title of the reference */
	title: string;
	/** Source (e.g. "PubMed", "brain-collective") */
	source: string;
	/** Summary or abstract excerpt */
	summary: string;
	/** URL if available */
	url?: string;
}

/** DrAgnes-specific brain statistics */
export interface DrAgnesStats {
	/** Total number of cases in the collective */
	totalCases: number;
	/** Cases per lesion class */
	casesByClass: Record<string, number>;
	/** Brain health status */
	brainStatus: string;
	/** Current brain epoch */
	epoch: number;
}

/** Result of sharing a diagnosis */
export interface ShareResult {
	/** Whether the share succeeded (or was queued offline) */
	success: boolean;
	/** Brain memory ID if online, null if queued */
	memoryId: string | null;
	/** Witness chain for the classification */
	witnessChain: WitnessChain[];
	/** Whether the contribution was queued for later sync */
	queued: boolean;
}

// ---- Differential Privacy ----

/**
 * Sample from a Laplace distribution with location 0 and scale b.
 */
function laplaceSample(scale: number): number {
	const u = Math.random() - 0.5;
	return -scale * Math.sign(u) * Math.log(1 - 2 * Math.abs(u));
}

/**
 * Apply Laplace differential privacy noise to an embedding vector.
 *
 * @param embedding - Original embedding
 * @param epsilon - Privacy budget (lower = more noise)
 * @param sensitivity - L1 sensitivity of the embedding (default 1.0)
 * @returns New array with DP noise added
 */
function addDPNoise(embedding: number[], epsilon: number, sensitivity = 1.0): number[] {
	const scale = sensitivity / epsilon;
	return embedding.map((v) => v + laplaceSample(scale));
}

/**
 * Strip any potential PHI from metadata before sending to brain.
 * Only allows known safe fields through.
 */
function stripPHI(metadata: DiagnosisMetadata): Record<string, unknown> {
	return {
		lesionClass: metadata.lesionClass,
		bodyLocation: metadata.bodyLocation,
		modelVersion: metadata.modelVersion,
		confidence: metadata.confidence,
		confirmed: metadata.confirmed,
	};
}

// ---- Fetch helper ----

/**
 * Fetch with timeout. Throws on network error or timeout.
 */
async function fetchWithTimeout(
	url: string,
	options: RequestInit = {},
	timeoutMs = FETCH_TIMEOUT_MS
): Promise<Response> {
	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), timeoutMs);

	try {
		const response = await fetch(url, {
			...options,
			signal: controller.signal,
		});
		return response;
	} finally {
		clearTimeout(timer);
	}
}

// ---- Brain Client ----

/** Singleton offline queue instance */
let offlineQueue: OfflineQueue | null = null;

function getOfflineQueue(): OfflineQueue {
	if (!offlineQueue) {
		offlineQueue = new OfflineQueue(BRAIN_BASE_URL);
	}
	return offlineQueue;
}

/**
 * Share a de-identified diagnosis with the pi.ruv.io brain.
 *
 * Pipeline:
 *   1. Strip all PHI from metadata
 *   2. Apply Laplace differential privacy noise (epsilon=1.0)
 *   3. Create witness chain hash
 *   4. POST to brain with dragnes tags
 *   5. If offline, queue for later sync
 *
 * @param embedding - Raw embedding vector (will have DP noise added)
 * @param metadata - Classification metadata (will have PHI stripped)
 * @returns ShareResult with witness chain and memory ID
 */
export async function shareDiagnosis(
	embedding: number[],
	metadata: DiagnosisMetadata
): Promise<ShareResult> {
	// Step 1: Strip PHI
	const safeMetadata = stripPHI(metadata);

	// Step 2: Apply differential privacy noise
	const dpEmbedding = addDPNoise(embedding, DEFAULT_EPSILON);

	// Step 3: Create witness chain
	const witnessChain = await createWitnessChain({
		embedding: dpEmbedding,
		modelVersion: metadata.modelVersion,
		probabilities: metadata.probabilities,
		brainEpoch: metadata.brainEpoch ?? 0,
		finalResult: metadata.lesionClass,
		confidence: metadata.confidence,
	});

	const witnessHash = witnessChain[witnessChain.length - 1].hash;

	// Step 4: Build brain memory payload
	const category = metadata.confirmed ? "solution" : "pattern";
	const tags = [
		DRAGNES_TAG,
		`class:${metadata.lesionClass}`,
		`location:${metadata.bodyLocation}`,
		category,
	];

	const payload = {
		title: `DrAgnes ${metadata.lesionClass} classification`,
		content: JSON.stringify({
			...safeMetadata,
			witnessHash,
			epsilon: DEFAULT_EPSILON,
		}),
		tags,
		category,
		embedding: dpEmbedding,
	};

	// Step 5: Attempt to send, queue if offline
	try {
		const response = await fetchWithTimeout(`${BRAIN_BASE_URL}/v1/memories`, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(payload),
		});

		if (response.ok) {
			const result = (await response.json()) as { id?: string };
			return {
				success: true,
				memoryId: result.id ?? null,
				witnessChain,
				queued: false,
			};
		}

		// Non-OK response: queue for retry
		await getOfflineQueue().enqueue("/v1/memories", payload);
		return { success: true, memoryId: null, witnessChain, queued: true };
	} catch {
		// Network error: queue for later
		await getOfflineQueue().enqueue("/v1/memories", payload);
		return { success: true, memoryId: null, witnessChain, queued: true };
	}
}

/**
 * Search the brain for similar lesion embeddings.
 *
 * @param embedding - Query embedding (DP noise is added before search)
 * @param k - Number of results to return (default 5)
 * @returns Array of similar cases from the collective
 */
export async function searchSimilar(embedding: number[], k = 5): Promise<SimilarCase[]> {
	const dpEmbedding = addDPNoise(embedding, DEFAULT_EPSILON);

	try {
		const params = new URLSearchParams({
			q: JSON.stringify(dpEmbedding.slice(0, 16)),
			limit: String(k),
			tag: DRAGNES_TAG,
		});

		const response = await fetchWithTimeout(`${BRAIN_BASE_URL}/v1/search?${params}`);

		if (!response.ok) {
			return [];
		}

		const data = (await response.json()) as {
			results?: Array<{
				id: string;
				similarity?: number;
				content?: string;
				tags?: string[];
			}>;
		};

		if (!data.results) {
			return [];
		}

		return data.results.map((r) => {
			let parsed: Record<string, unknown> = {};
			try {
				parsed = JSON.parse(r.content ?? "{}") as Record<string, unknown>;
			} catch {
				// content might not be JSON
			}

			return {
				id: r.id,
				similarity: r.similarity ?? 0,
				lesionClass: (parsed.lesionClass as string) ?? "unknown",
				bodyLocation: (parsed.bodyLocation as string) ?? "unknown",
				confidence: (parsed.confidence as number) ?? 0,
				confirmed: (parsed.confirmed as boolean) ?? false,
			};
		});
	} catch {
		return [];
	}
}

/**
 * Search brain and trigger PubMed context for literature references.
 *
 * @param lesionClass - The lesion class to search literature for
 * @returns Array of literature references
 */
export async function searchLiterature(lesionClass: LesionClass): Promise<LiteratureReference[]> {
	try {
		const params = new URLSearchParams({
			q: `${lesionClass} dermoscopy diagnosis treatment`,
			tag: DRAGNES_TAG,
		});

		const response = await fetchWithTimeout(`${BRAIN_BASE_URL}/v1/search?${params}`);

		if (!response.ok) {
			return [];
		}

		const data = (await response.json()) as {
			results?: Array<{
				title?: string;
				content?: string;
				tags?: string[];
				url?: string;
			}>;
		};

		if (!data.results) {
			return [];
		}

		return data.results.map((r) => ({
			title: r.title ?? "Untitled",
			source: r.tags?.includes("pubmed") ? "PubMed" : "brain-collective",
			summary: (r.content ?? "").slice(0, 500),
			url: r.url,
		}));
	} catch {
		return [];
	}
}

/**
 * Check for LoRA model updates from the collective brain.
 *
 * @returns Object with update availability and version info, or null if offline
 */
export async function syncModel(): Promise<{
	available: boolean;
	version: string | null;
	epoch: number;
} | null> {
	try {
		const response = await fetchWithTimeout(`${BRAIN_BASE_URL}/v1/status`);

		if (!response.ok) {
			return null;
		}

		const status = (await response.json()) as {
			epoch?: number;
			version?: string;
			loraAvailable?: boolean;
		};

		return {
			available: status.loraAvailable ?? false,
			version: status.version ?? null,
			epoch: status.epoch ?? 0,
		};
	} catch {
		return null;
	}
}

/**
 * Get DrAgnes-specific brain statistics.
 *
 * @returns Statistics about the collective, or null if offline
 */
export async function getStats(): Promise<DrAgnesStats | null> {
	try {
		const [statusRes, searchRes] = await Promise.all([
			fetchWithTimeout(`${BRAIN_BASE_URL}/v1/status`),
			fetchWithTimeout(
				`${BRAIN_BASE_URL}/v1/search?${new URLSearchParams({ q: "*", tag: DRAGNES_TAG, limit: "0" })}`
			),
		]);

		if (!statusRes.ok) {
			return null;
		}

		const status = (await statusRes.json()) as {
			status?: string;
			epoch?: number;
			totalMemories?: number;
		};

		let totalCases = status.totalMemories ?? 0;
		const casesByClass: Record<string, number> = {};

		if (searchRes.ok) {
			const searchData = (await searchRes.json()) as {
				total?: number;
				results?: Array<{ content?: string }>;
			};
			totalCases = searchData.total ?? totalCases;

			if (searchData.results) {
				for (const r of searchData.results) {
					try {
						const parsed = JSON.parse(r.content ?? "{}") as { lesionClass?: string };
						if (parsed.lesionClass) {
							casesByClass[parsed.lesionClass] =
								(casesByClass[parsed.lesionClass] ?? 0) + 1;
						}
					} catch {
						// skip unparseable entries
					}
				}
			}
		}

		return {
			totalCases,
			casesByClass,
			brainStatus: status.status ?? "unknown",
			epoch: status.epoch ?? 0,
		};
	} catch {
		return null;
	}
}

/**
 * Get the offline queue instance for manual queue management.
 */
export function getQueue(): OfflineQueue {
	return getOfflineQueue();
}
