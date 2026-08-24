/**
 * Witness Chain Implementation for DrAgnes
 *
 * Creates a 3-entry audit chain for each classification using SubtleCrypto SHA-256.
 * Each entry links to the previous via hash chaining, providing tamper-evident
 * provenance for every diagnosis.
 */

import type { WitnessChain } from "./types";

/** Compute SHA-256 hex digest using SubtleCrypto */
async function sha256(data: string): Promise<string> {
	const encoded = new TextEncoder().encode(data);
	const buffer = await crypto.subtle.digest("SHA-256", encoded.buffer);
	return Array.from(new Uint8Array(buffer))
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
}

/** Input parameters for witness chain creation */
export interface WitnessInput {
	/** Image embedding vector (already de-identified) */
	embedding: number[];
	/** Model version string */
	modelVersion: string;
	/** Per-class probability scores */
	probabilities: number[];
	/** Brain epoch at time of classification */
	brainEpoch: number;
	/** Final classification result label */
	finalResult: string;
	/** Confidence score of the final result */
	confidence: number;
}

/**
 * Creates a 3-entry witness chain for a classification event.
 *
 * Chain structure:
 *   1. Input hash: hash(embedding + model version)
 *   2. Classification hash: hash(probabilities + brain epoch + previous hash)
 *   3. Output hash: hash(final result + timestamp + previous hash)
 *
 * @param input - The classification data to chain
 * @returns Array of 3 WitnessChain entries, linked by previousHash
 */
export async function createWitnessChain(input: WitnessInput): Promise<WitnessChain[]> {
	const now = new Date().toISOString();
	const chain: WitnessChain[] = [];

	// Entry 1: Input hash
	const inputPayload = JSON.stringify({
		embedding: input.embedding.slice(0, 8), // partial for privacy
		modelVersion: input.modelVersion,
	});
	const inputDataHash = await sha256(inputPayload);
	const inputHash = await sha256(`input:${inputDataHash}:genesis`);

	chain.push({
		hash: inputHash,
		previousHash: "genesis",
		action: "input",
		timestamp: now,
		dataHash: inputDataHash,
	});

	// Entry 2: Classification hash
	const classPayload = JSON.stringify({
		probabilities: input.probabilities,
		brainEpoch: input.brainEpoch,
	});
	const classDataHash = await sha256(classPayload);
	const classHash = await sha256(`classification:${classDataHash}:${inputHash}`);

	chain.push({
		hash: classHash,
		previousHash: inputHash,
		action: "classification",
		timestamp: now,
		dataHash: classDataHash,
	});

	// Entry 3: Output hash
	const outputPayload = JSON.stringify({
		finalResult: input.finalResult,
		confidence: input.confidence,
		timestamp: now,
	});
	const outputDataHash = await sha256(outputPayload);
	const outputHash = await sha256(`output:${outputDataHash}:${classHash}`);

	chain.push({
		hash: outputHash,
		previousHash: classHash,
		action: "output",
		timestamp: now,
		dataHash: outputDataHash,
	});

	return chain;
}

/**
 * Verifies the integrity of a witness chain.
 *
 * Checks that:
 *   - Chain has exactly 3 entries
 *   - First entry's previousHash is "genesis"
 *   - Each entry's previousHash matches the prior entry's hash
 *   - Actions follow the expected sequence: input -> classification -> output
 *
 * @param chain - The witness chain to verify
 * @returns true if chain is valid, false otherwise
 */
export function verifyWitnessChain(chain: WitnessChain[]): boolean {
	if (chain.length !== 3) {
		return false;
	}

	const expectedActions = ["input", "classification", "output"];

	for (let i = 0; i < chain.length; i++) {
		const entry = chain[i];

		// Check action sequence
		if (entry.action !== expectedActions[i]) {
			return false;
		}

		// Check hash linking
		if (i === 0) {
			if (entry.previousHash !== "genesis") {
				return false;
			}
		} else {
			if (entry.previousHash !== chain[i - 1].hash) {
				return false;
			}
		}

		// Verify hashes are non-empty hex strings
		if (!/^[a-f0-9]{64}$/.test(entry.hash)) {
			return false;
		}
		if (!/^[a-f0-9]{64}$/.test(entry.dataHash)) {
			return false;
		}
	}

	return true;
}
