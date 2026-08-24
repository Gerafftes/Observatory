/**
 * DrAgnes Classification Pipeline Tests
 *
 * Tests for preprocessing, ABCDE scoring, privacy pipeline,
 * and CNN classification with demo fallback.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { DermClassifier } from "../src/lib/dragnes/classifier";
import { computeABCDE } from "../src/lib/dragnes/abcde";
import { PrivacyPipeline } from "../src/lib/dragnes/privacy";
import {
	colorNormalize,
	removeHair,
	segmentLesion,
	resizeBilinear,
	toNCHWTensor,
} from "../src/lib/dragnes/preprocessing";
import type { ClassificationResult, ABCDEScores, SegmentationMask } from "../src/lib/dragnes/types";

// ---- Polyfill ImageData for Node.js ----

if (typeof globalThis.ImageData === "undefined") {
	(globalThis as Record<string, unknown>).ImageData = class ImageData {
		readonly data: Uint8ClampedArray;
		readonly width: number;
		readonly height: number;
		readonly colorSpace: string = "srgb";

		constructor(dataOrWidth: Uint8ClampedArray | number, widthOrHeight: number, height?: number) {
			if (dataOrWidth instanceof Uint8ClampedArray) {
				this.data = dataOrWidth;
				this.width = widthOrHeight;
				this.height = height ?? (dataOrWidth.length / 4 / widthOrHeight);
			} else {
				this.width = dataOrWidth;
				this.height = widthOrHeight;
				this.data = new Uint8ClampedArray(this.width * this.height * 4);
			}
		}
	};
}

// ---- Helpers ----

/** Create a mock ImageData (no DOM required) */
function createMockImageData(width: number, height: number, fill?: { r: number; g: number; b: number }): ImageData {
	const data = new Uint8ClampedArray(width * height * 4);
	const r = fill?.r ?? 128;
	const g = fill?.g ?? 80;
	const b = fill?.b ?? 50;

	for (let i = 0; i < data.length; i += 4) {
		data[i] = r;
		data[i + 1] = g;
		data[i + 2] = b;
		data[i + 3] = 255;
	}

	return new ImageData(data, width, height);
}

/** Create an ImageData with a dark circle (simulated lesion) */
function createLesionImageData(width: number, height: number): ImageData {
	const data = new Uint8ClampedArray(width * height * 4);
	const cx = width / 2;
	const cy = height / 2;
	const radius = Math.min(width, height) / 4;

	for (let y = 0; y < height; y++) {
		for (let x = 0; x < width; x++) {
			const idx = (y * width + x) * 4;
			const dist = Math.sqrt((x - cx) ** 2 + (y - cy) ** 2);

			if (dist < radius) {
				// Dark brown lesion
				data[idx] = 80;
				data[idx + 1] = 40;
				data[idx + 2] = 20;
			} else {
				// Skin-colored background
				data[idx] = 200;
				data[idx + 1] = 160;
				data[idx + 2] = 140;
			}
			data[idx + 3] = 255;
		}
	}

	return new ImageData(data, width, height);
}

// ---- Preprocessing Tests ----

describe("Preprocessing Pipeline", () => {
	describe("colorNormalize", () => {
		it("should normalize color channels", () => {
			const input = createMockImageData(10, 10, { r: 200, g: 100, b: 50 });
			const result = colorNormalize(input);

			expect(result.width).toBe(10);
			expect(result.height).toBe(10);
			expect(result.data.length).toBe(input.data.length);
			// The dominant channel (R) should remain high
			expect(result.data[0]).toBeGreaterThan(0);
		});

		it("should preserve image dimensions", () => {
			const input = createMockImageData(50, 30);
			const result = colorNormalize(input);

			expect(result.width).toBe(50);
			expect(result.height).toBe(30);
		});

		it("should handle uniform images without error", () => {
			const input = createMockImageData(10, 10, { r: 128, g: 128, b: 128 });
			const result = colorNormalize(input);

			expect(result.data.length).toBe(400);
		});
	});

	describe("removeHair", () => {
		it("should return image of same dimensions", () => {
			const input = createMockImageData(20, 20);
			const result = removeHair(input);

			expect(result.width).toBe(20);
			expect(result.height).toBe(20);
			expect(result.data.length).toBe(input.data.length);
		});

		it("should not modify bright images significantly", () => {
			const input = createMockImageData(10, 10, { r: 200, g: 180, b: 170 });
			const result = removeHair(input);

			// Bright pixels should not be detected as hair
			let diffSum = 0;
			for (let i = 0; i < input.data.length; i++) {
				diffSum += Math.abs(result.data[i] - input.data[i]);
			}
			expect(diffSum).toBe(0);
		});
	});

	describe("segmentLesion", () => {
		it("should produce binary mask", () => {
			const input = createLesionImageData(50, 50);
			const seg = segmentLesion(input);

			expect(seg.width).toBe(50);
			expect(seg.height).toBe(50);
			expect(seg.mask.length).toBe(2500);

			// All values should be 0 or 1
			for (let i = 0; i < seg.mask.length; i++) {
				expect(seg.mask[i]).toBeGreaterThanOrEqual(0);
				expect(seg.mask[i]).toBeLessThanOrEqual(1);
			}
		});

		it("should detect lesion area", () => {
			const input = createLesionImageData(100, 100);
			const seg = segmentLesion(input);

			expect(seg.areaPixels).toBeGreaterThan(0);
			expect(seg.boundingBox.w).toBeGreaterThan(0);
			expect(seg.boundingBox.h).toBeGreaterThan(0);
		});
	});

	describe("resizeBilinear", () => {
		it("should resize to target dimensions", () => {
			const input = createMockImageData(100, 80);
			const result = resizeBilinear(input, 224, 224);

			expect(result.width).toBe(224);
			expect(result.height).toBe(224);
			expect(result.data.length).toBe(224 * 224 * 4);
		});

		it("should handle downscaling", () => {
			const input = createMockImageData(500, 400);
			const result = resizeBilinear(input, 50, 40);

			expect(result.width).toBe(50);
			expect(result.height).toBe(40);
		});
	});

	describe("toNCHWTensor", () => {
		it("should produce correct tensor shape", () => {
			const input = createMockImageData(224, 224);
			const tensor = toNCHWTensor(input);

			expect(tensor.shape).toEqual([1, 3, 224, 224]);
			expect(tensor.data.length).toBe(3 * 224 * 224);
			expect(tensor.data).toBeInstanceOf(Float32Array);
		});

		it("should apply ImageNet normalization", () => {
			// Pure white image: RGB = (255, 255, 255)
			const input = createMockImageData(4, 4, { r: 255, g: 255, b: 255 });
			const tensor = toNCHWTensor(input);

			// After normalization: (1.0 - mean) / std
			const expectedR = (1.0 - 0.485) / 0.229;
			expect(tensor.data[0]).toBeCloseTo(expectedR, 3);
		});
	});
});

// ---- Classification Tests ----

describe("DermClassifier", () => {
	let classifier: DermClassifier;

	beforeEach(async () => {
		classifier = new DermClassifier();
		await classifier.init();
	});

	it("should initialize in demo mode (no WASM available)", () => {
		expect(classifier.isInitialized()).toBe(true);
		expect(classifier.isWasmLoaded()).toBe(false);
	});

	it("should classify and return 7 class probabilities", async () => {
		const imageData = createLesionImageData(100, 100);
		const result = await classifier.classify(imageData);

		expect(result.probabilities).toHaveLength(7);
		expect(result.topClass).toBeDefined();
		expect(result.confidence).toBeGreaterThan(0);
		expect(result.confidence).toBeLessThanOrEqual(1);
		expect(result.usedWasm).toBe(false);
		expect(result.modelId).toBe("demo-color-texture");
	});

	it("should return probabilities summing to 1", async () => {
		const imageData = createLesionImageData(80, 80);
		const result = await classifier.classify(imageData);

		const sum = result.probabilities.reduce((acc, p) => acc + p.probability, 0);
		expect(sum).toBeCloseTo(1.0, 5);
	});

	it("should sort probabilities in descending order", async () => {
		const imageData = createMockImageData(64, 64);
		const result = await classifier.classify(imageData);

		for (let i = 1; i < result.probabilities.length; i++) {
			expect(result.probabilities[i - 1].probability).toBeGreaterThanOrEqual(
				result.probabilities[i].probability
			);
		}
	});

	it("should report inference time", async () => {
		const imageData = createMockImageData(50, 50);
		const result = await classifier.classify(imageData);

		expect(result.inferenceTimeMs).toBeGreaterThanOrEqual(0);
	});

	it("should include all HAM10000 classes", async () => {
		const imageData = createMockImageData(30, 30);
		const result = await classifier.classify(imageData);

		const classNames = result.probabilities.map((p) => p.className);
		expect(classNames).toContain("akiec");
		expect(classNames).toContain("bcc");
		expect(classNames).toContain("bkl");
		expect(classNames).toContain("df");
		expect(classNames).toContain("mel");
		expect(classNames).toContain("nv");
		expect(classNames).toContain("vasc");
	});

	it("should generate Grad-CAM after classification", async () => {
		const imageData = createLesionImageData(60, 60);
		await classifier.classify(imageData);
		const gradCam = await classifier.getGradCam();

		expect(gradCam.heatmap.width).toBe(224);
		expect(gradCam.heatmap.height).toBe(224);
		expect(gradCam.overlay.width).toBe(224);
		expect(gradCam.overlay.height).toBe(224);
		expect(gradCam.targetClass).toBeDefined();
	});

	it("should throw if getGradCam called without classify", async () => {
		const freshClassifier = new DermClassifier();
		await freshClassifier.init();

		await expect(freshClassifier.getGradCam()).rejects.toThrow("No image classified yet");
	});
});

// ---- ABCDE Scoring Tests ----

describe("ABCDE Scoring", () => {
	it("should return valid score structure", async () => {
		const imageData = createLesionImageData(100, 100);
		const scores = await computeABCDE(imageData, 10);

		expect(scores.asymmetry).toBeGreaterThanOrEqual(0);
		expect(scores.asymmetry).toBeLessThanOrEqual(2);
		expect(scores.border).toBeGreaterThanOrEqual(0);
		expect(scores.border).toBeLessThanOrEqual(8);
		expect(scores.color).toBeGreaterThanOrEqual(1);
		expect(scores.color).toBeLessThanOrEqual(6);
		expect(scores.diameterMm).toBeGreaterThan(0);
		expect(scores.evolution).toBe(0); // No previous image
	});

	it("should assign risk level based on total score", async () => {
		const imageData = createLesionImageData(100, 100);
		const scores = await computeABCDE(imageData);

		const validLevels = ["low", "moderate", "high", "critical"];
		expect(validLevels).toContain(scores.riskLevel);
	});

	it("should return detected colors", async () => {
		const imageData = createLesionImageData(100, 100);
		const scores = await computeABCDE(imageData);

		expect(Array.isArray(scores.colorsDetected)).toBe(true);
	});

	it("should compute diameter relative to magnification", async () => {
		const imageData = createLesionImageData(100, 100);
		const scores10x = await computeABCDE(imageData, 10);
		const scores20x = await computeABCDE(imageData, 20);

		// Higher magnification = smaller apparent diameter
		expect(scores20x.diameterMm).toBeLessThan(scores10x.diameterMm);
	});
});

// ---- Privacy Pipeline Tests ----

describe("PrivacyPipeline", () => {
	let pipeline: PrivacyPipeline;

	beforeEach(() => {
		pipeline = new PrivacyPipeline(1.0, 5);
	});

	describe("EXIF Stripping", () => {
		it("should return bytes for non-JPEG/PNG input", () => {
			const data = new Uint8Array([0x00, 0x01, 0x02, 0x03]);
			const result = pipeline.stripExif(data);

			expect(result).toBeInstanceOf(Uint8Array);
			expect(result.length).toBe(4);
		});

		it("should strip APP1 marker from JPEG", () => {
			// Minimal JPEG with fake EXIF APP1 segment
			const jpeg = new Uint8Array([
				0xff, 0xd8, // SOI
				0xff, 0xe1, // APP1 (EXIF)
				0x00, 0x04, // Length 4
				0x45, 0x78, // Data
				0xff, 0xda, // SOS
				0x00, 0x02, // Length
				0xff, 0xd9, // EOI
			]);

			const result = pipeline.stripExif(jpeg);

			// APP1 segment should be removed
			let hasApp1 = false;
			for (let i = 0; i < result.length - 1; i++) {
				if (result[i] === 0xff && result[i + 1] === 0xe1) {
					hasApp1 = true;
				}
			}
			expect(hasApp1).toBe(false);
		});
	});

	describe("PII Detection", () => {
		it("should detect email addresses", () => {
			const { cleaned, found } = pipeline.redactPII("Contact: john@example.com for info");

			expect(found).toContain("email");
			expect(cleaned).toContain("[REDACTED_EMAIL]");
			expect(cleaned).not.toContain("john@example.com");
		});

		it("should detect phone numbers", () => {
			const { cleaned, found } = pipeline.redactPII("Call 555-123-4567");

			expect(found).toContain("phone");
			expect(cleaned).toContain("[REDACTED_PHONE]");
		});

		it("should detect SSN patterns", () => {
			const { cleaned, found } = pipeline.redactPII("SSN: 123-45-6789");

			expect(found).toContain("ssn");
			expect(cleaned).not.toContain("123-45-6789");
		});

		it("should detect MRN patterns", () => {
			const { cleaned, found } = pipeline.redactPII("MRN: 12345678");

			expect(found).toContain("mrn");
			expect(cleaned).not.toContain("12345678");
		});

		it("should return empty found array for clean text", () => {
			const { cleaned, found } = pipeline.redactPII("Normal medical notes about lesion size");

			expect(found).toHaveLength(0);
			expect(cleaned).toBe("Normal medical notes about lesion size");
		});
	});

	describe("Differential Privacy", () => {
		it("should add Laplace noise to embedding", () => {
			const embedding = new Float32Array([1.0, 2.0, 3.0, 4.0, 5.0]);
			const original = new Float32Array(embedding);

			pipeline.addLaplaceNoise(embedding, 1.0);

			// At least some values should have changed
			let changed = false;
			for (let i = 0; i < embedding.length; i++) {
				if (Math.abs(embedding[i] - original[i]) > 1e-10) {
					changed = true;
					break;
				}
			}
			expect(changed).toBe(true);
		});

		it("should preserve embedding length", () => {
			const embedding = new Float32Array(128);
			pipeline.addLaplaceNoise(embedding, 1.0);

			expect(embedding.length).toBe(128);
		});
	});

	describe("k-Anonymity", () => {
		it("should pass with few quasi-identifiers", () => {
			const metadata = { notes: "Normal lesion", location: "arm" };
			expect(pipeline.checkKAnonymity(metadata)).toBe(true);
		});

		it("should flag many quasi-identifiers", () => {
			const metadata = {
				age: "45",
				gender: "M",
				zip: "90210",
				city: "Beverly Hills",
				state: "CA",
				ethnicity: "Caucasian",
			};
			expect(pipeline.checkKAnonymity(metadata)).toBe(false);
		});
	});

	describe("Full Pipeline", () => {
		it("should process image with metadata", async () => {
			const imageBytes = new Uint8Array([0x00, 0x01, 0x02]);
			const metadata = { notes: "Patient john@test.com has a lesion" };

			const { cleanMetadata, report } = await pipeline.process(imageBytes, metadata);

			expect(report.piiDetected).toContain("email");
			expect(cleanMetadata.notes).not.toContain("john@test.com");
			expect(report.witnessHash).toBeDefined();
			expect(report.witnessHash.length).toBeGreaterThan(0);
		});

		it("should apply DP noise when embedding provided", async () => {
			const imageBytes = new Uint8Array([0x00]);
			const embedding = new Float32Array([1.0, 2.0, 3.0]);

			const { report } = await pipeline.process(imageBytes, {}, embedding);

			expect(report.dpNoiseApplied).toBe(true);
			expect(report.epsilon).toBe(1.0);
		});
	});

	describe("Witness Chain", () => {
		it("should build chain with linked hashes", async () => {
			const data1 = new Uint8Array([1, 2, 3]);
			const data2 = new Uint8Array([4, 5, 6]);

			const hash1 = await pipeline.computeHash(data1);
			await pipeline.addWitnessEntry("action1", hash1);

			const hash2 = await pipeline.computeHash(data2);
			await pipeline.addWitnessEntry("action2", hash2);

			const chain = pipeline.getWitnessChain();
			expect(chain).toHaveLength(2);
			expect(chain[1].previousHash).toBe(chain[0].hash);
		});
	});
});
