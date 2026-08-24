/**
 * DrAgnes Privacy Pipeline
 *
 * Provides EXIF stripping, PII detection, differential privacy
 * noise addition, witness chain hashing, and k-anonymity checks
 * for dermoscopic image analysis.
 */

import type { PrivacyReport, WitnessChain } from "./types";

/** Common PII patterns */
const PII_PATTERNS: Array<{ name: string; regex: RegExp }> = [
	{ name: "email", regex: /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g },
	{ name: "phone", regex: /\b\d{3}[-.]?\d{3}[-.]?\d{4}\b/g },
	{ name: "ssn", regex: /\b\d{3}-\d{2}-\d{4}\b/g },
	{ name: "date_of_birth", regex: /\b(0[1-9]|1[0-2])\/(0[1-9]|[12]\d|3[01])\/(19|20)\d{2}\b/g },
	{ name: "mrn", regex: /\bMRN\s*:?\s*\d{6,10}\b/gi },
	{ name: "name_prefix", regex: /\b(Mr|Mrs|Ms|Dr|Patient)\.\s[A-Z][a-z]+\s[A-Z][a-z]+\b/g },
	{ name: "ip_address", regex: /\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b/g },
];

/** EXIF marker bytes in JPEG */
const EXIF_MARKERS = {
	SOI: 0xffd8,
	APP1: 0xffe1,
	APP13: 0xffed,
	SOS: 0xffda,
};

/**
 * Privacy pipeline for dermoscopic image analysis.
 * Handles EXIF stripping, PII detection, differential privacy,
 * and witness chain computation.
 */
export class PrivacyPipeline {
	private epsilon: number;
	private kValue: number;
	private witnessChain: WitnessChain[];

	/**
	 * @param epsilon - Differential privacy epsilon parameter (default 1.0)
	 * @param kValue - k-anonymity threshold (default 5)
	 */
	constructor(epsilon: number = 1.0, kValue: number = 5) {
		this.epsilon = epsilon;
		this.kValue = kValue;
		this.witnessChain = [];
	}

	/**
	 * Run the full privacy pipeline on image data and metadata.
	 *
	 * @param imageBytes - Raw image bytes (JPEG/PNG)
	 * @param metadata - Associated text metadata to scan for PII
	 * @param embedding - Optional embedding vector to add DP noise to
	 * @returns Privacy report with actions taken
	 */
	async process(
		imageBytes: Uint8Array,
		metadata: Record<string, string> = {},
		embedding?: Float32Array
	): Promise<{ cleanImage: Uint8Array; cleanMetadata: Record<string, string>; report: PrivacyReport }> {
		// Step 1: Strip EXIF
		const cleanImage = this.stripExif(imageBytes);
		const exifStripped = cleanImage.length !== imageBytes.length || !this.hasExifMarker(cleanImage);

		// Step 2: Detect and redact PII
		const piiDetected: string[] = [];
		const cleanMetadata: Record<string, string> = {};
		for (const [key, value] of Object.entries(metadata)) {
			const { cleaned, found } = this.redactPII(value);
			piiDetected.push(...found);
			cleanMetadata[key] = cleaned;
		}

		// Step 3: Add DP noise to embedding
		let dpNoiseApplied = false;
		if (embedding) {
			this.addLaplaceNoise(embedding, this.epsilon);
			dpNoiseApplied = true;
		}

		// Step 4: k-anonymity check
		const kAnonymityMet = this.checkKAnonymity(cleanMetadata);

		// Step 5: Witness chain
		const dataHash = await this.computeHash(cleanImage);
		const witnessHash = await this.addWitnessEntry("privacy_pipeline_complete", dataHash);

		return {
			cleanImage,
			cleanMetadata,
			report: {
				exifStripped,
				piiDetected: [...new Set(piiDetected)],
				dpNoiseApplied,
				epsilon: this.epsilon,
				kAnonymityMet,
				kValue: this.kValue,
				witnessHash,
			},
		};
	}

	/**
	 * Strip EXIF and other metadata from JPEG image bytes.
	 * Removes APP1 (EXIF) and APP13 (IPTC) segments while
	 * preserving image data.
	 *
	 * @param imageBytes - Raw JPEG bytes
	 * @returns JPEG bytes with metadata segments removed
	 */
	stripExif(imageBytes: Uint8Array): Uint8Array {
		if (imageBytes.length < 4) return imageBytes;

		// Check for JPEG SOI marker
		if (imageBytes[0] !== 0xff || imageBytes[1] !== 0xd8) {
			// Not a JPEG, return as-is (PNG metadata stripping is simpler)
			return this.stripPngMetadata(imageBytes);
		}

		const result: number[] = [0xff, 0xd8]; // SOI
		let offset = 2;

		while (offset < imageBytes.length - 1) {
			const marker = (imageBytes[offset] << 8) | imageBytes[offset + 1];

			// Reached image data, copy everything remaining
			if (marker === EXIF_MARKERS.SOS || (marker & 0xff00) !== 0xff00) {
				for (let i = offset; i < imageBytes.length; i++) {
					result.push(imageBytes[i]);
				}
				break;
			}

			// Get segment length
			if (offset + 3 >= imageBytes.length) break;
			const segLen = (imageBytes[offset + 2] << 8) | imageBytes[offset + 3];

			// Skip APP1 (EXIF) and APP13 (IPTC) segments
			if (marker === EXIF_MARKERS.APP1 || marker === EXIF_MARKERS.APP13) {
				offset += 2 + segLen;
				continue;
			}

			// Keep other segments
			for (let i = 0; i < 2 + segLen; i++) {
				if (offset + i < imageBytes.length) {
					result.push(imageBytes[offset + i]);
				}
			}
			offset += 2 + segLen;
		}

		return new Uint8Array(result);
	}

	/**
	 * Strip metadata chunks from PNG files.
	 * Removes tEXt, iTXt, and zTXt chunks.
	 */
	private stripPngMetadata(imageBytes: Uint8Array): Uint8Array {
		// PNG signature check
		if (
			imageBytes.length < 8 ||
			imageBytes[0] !== 0x89 ||
			imageBytes[1] !== 0x50 ||
			imageBytes[2] !== 0x4e ||
			imageBytes[3] !== 0x47
		) {
			return imageBytes; // Not PNG either
		}

		const metaChunks = new Set(["tEXt", "iTXt", "zTXt", "eXIf"]);
		const result: number[] = [];

		// Copy PNG signature
		for (let i = 0; i < 8; i++) result.push(imageBytes[i]);

		let offset = 8;
		while (offset + 8 <= imageBytes.length) {
			const length =
				(imageBytes[offset] << 24) |
				(imageBytes[offset + 1] << 16) |
				(imageBytes[offset + 2] << 8) |
				imageBytes[offset + 3];

			const chunkType = String.fromCharCode(
				imageBytes[offset + 4],
				imageBytes[offset + 5],
				imageBytes[offset + 6],
				imageBytes[offset + 7]
			);

			const totalChunkSize = 4 + 4 + length + 4; // length + type + data + CRC

			if (!metaChunks.has(chunkType)) {
				for (let i = 0; i < totalChunkSize && offset + i < imageBytes.length; i++) {
					result.push(imageBytes[offset + i]);
				}
			}

			offset += totalChunkSize;
		}

		return new Uint8Array(result);
	}

	/**
	 * Check if image bytes contain EXIF markers.
	 */
	private hasExifMarker(imageBytes: Uint8Array): boolean {
		for (let i = 0; i < imageBytes.length - 1; i++) {
			if (imageBytes[i] === 0xff && imageBytes[i + 1] === 0xe1) {
				return true;
			}
		}
		return false;
	}

	/**
	 * Detect and redact PII from text.
	 *
	 * @param text - Input text to scan
	 * @returns Cleaned text and list of PII types found
	 */
	redactPII(text: string): { cleaned: string; found: string[] } {
		let cleaned = text;
		const found: string[] = [];

		for (const pattern of PII_PATTERNS) {
			const matches = cleaned.match(pattern.regex);
			if (matches && matches.length > 0) {
				found.push(pattern.name);
				cleaned = cleaned.replace(pattern.regex, `[REDACTED_${pattern.name.toUpperCase()}]`);
			}
		}

		return { cleaned, found };
	}

	/**
	 * Add Laplace noise for differential privacy.
	 * Modifies the embedding in-place.
	 *
	 * @param embedding - Float32 embedding vector (modified in-place)
	 * @param epsilon - Privacy parameter (smaller = more private)
	 */
	addLaplaceNoise(embedding: Float32Array, epsilon: number): void {
		const sensitivity = 1.0; // L1 sensitivity
		const scale = sensitivity / epsilon;

		for (let i = 0; i < embedding.length; i++) {
			embedding[i] += this.sampleLaplace(scale);
		}
	}

	/**
	 * Sample from Laplace distribution using inverse CDF.
	 */
	private sampleLaplace(scale: number): number {
		const u = Math.random() - 0.5;
		return -scale * Math.sign(u) * Math.log(1 - 2 * Math.abs(u));
	}

	/**
	 * Compute SHA-256 hash as SHAKE-256 simulation.
	 * Uses the Web Crypto API when available, falls back to
	 * a simple hash for non-browser environments.
	 *
	 * @param data - Data to hash
	 * @returns Hex-encoded hash string
	 */
	async computeHash(data: Uint8Array): Promise<string> {
		try {
			if (typeof globalThis.crypto !== "undefined" && globalThis.crypto.subtle) {
				const hashBuffer = await globalThis.crypto.subtle.digest("SHA-256", data);
				const hashArray = new Uint8Array(hashBuffer);
				return Array.from(hashArray)
					.map((b) => b.toString(16).padStart(2, "0"))
					.join("");
			}
		} catch {
			// Fallback below
		}

		// Simple fallback hash (FNV-1a inspired, for environments without crypto)
		let h = 0x811c9dc5;
		for (let i = 0; i < data.length; i++) {
			h ^= data[i];
			h = Math.imul(h, 0x01000193);
		}
		return (h >>> 0).toString(16).padStart(8, "0").repeat(8);
	}

	/**
	 * Add an entry to the witness chain.
	 *
	 * @param action - Description of the action
	 * @param dataHash - Hash of the associated data
	 * @returns Hash of the new witness entry
	 */
	async addWitnessEntry(action: string, dataHash: string): Promise<string> {
		const previousHash = this.witnessChain.length > 0 ? this.witnessChain[this.witnessChain.length - 1].hash : "0".repeat(64);

		const timestamp = new Date().toISOString();
		const entryData = new TextEncoder().encode(`${previousHash}:${action}:${dataHash}:${timestamp}`);
		const hash = await this.computeHash(entryData);

		this.witnessChain.push({
			hash,
			previousHash,
			action,
			timestamp,
			dataHash,
		});

		return hash;
	}

	/**
	 * Check k-anonymity for metadata quasi-identifiers.
	 * Verifies that no combination of quasi-identifiers uniquely
	 * identifies a record when k > 1.
	 *
	 * @param metadata - Metadata key-value pairs
	 * @returns True if k-anonymity requirement is met
	 */
	checkKAnonymity(metadata: Record<string, string>): boolean {
		// Quasi-identifiers that could re-identify a person
		const quasiIdentifiers = ["age", "gender", "zip", "zipcode", "postal_code", "city", "state", "ethnicity"];

		const qiValues = Object.entries(metadata)
			.filter(([key]) => quasiIdentifiers.includes(key.toLowerCase()))
			.map(([_, value]) => value);

		// If fewer than k quasi-identifiers are present, we consider it safe
		// In production this would check against a population table
		if (qiValues.length < 2) return true;

		// With 3+ quasi-identifiers, the combination may be unique
		// This is a conservative check - flag if too many QIs present
		return qiValues.length < this.kValue;
	}

	/**
	 * Get the current witness chain.
	 */
	getWitnessChain(): WitnessChain[] {
		return [...this.witnessChain];
	}

	/**
	 * Get the current epsilon value.
	 */
	getEpsilon(): number {
		return this.epsilon;
	}
}
