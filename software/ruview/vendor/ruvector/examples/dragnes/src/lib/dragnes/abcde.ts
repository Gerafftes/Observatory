/**
 * DrAgnes ABCDE Dermoscopic Scoring
 *
 * Implements the ABCDE rule for dermoscopic evaluation:
 * - Asymmetry (0-2): Bilateral symmetry analysis
 * - Border (0-8): Border irregularity in 8 segments
 * - Color (1-6): Distinct color count
 * - Diameter: Lesion diameter in mm
 * - Evolution: Change tracking over time
 */

import type { ABCDEScores, RiskLevel, SegmentationMask } from "./types";
import { segmentLesion } from "./preprocessing";

/** Color ranges in RGB for ABCDE color scoring */
const ABCDE_COLORS: Record<string, { min: [number, number, number]; max: [number, number, number] }> = {
	white: { min: [200, 200, 200], max: [255, 255, 255] },
	red: { min: [150, 30, 30], max: [255, 100, 100] },
	"light-brown": { min: [140, 90, 50], max: [200, 150, 100] },
	"dark-brown": { min: [50, 20, 10], max: [140, 80, 50] },
	"blue-gray": { min: [80, 90, 110], max: [160, 170, 190] },
	black: { min: [0, 0, 0], max: [50, 50, 50] },
};

/**
 * Compute full ABCDE scores for a dermoscopic image.
 *
 * @param imageData - RGBA ImageData of the lesion
 * @param magnification - DermLite magnification factor (default 10)
 * @param previousMask - Previous segmentation mask for evolution scoring
 * @returns ABCDE scores with risk level
 */
export async function computeABCDE(
	imageData: ImageData,
	magnification: number = 10,
	previousMask?: SegmentationMask
): Promise<ABCDEScores> {
	const segmentation = segmentLesion(imageData);

	const asymmetry = scoreAsymmetry(segmentation);
	const border = scoreBorder(segmentation);
	const { score: color, detected: colorsDetected } = scoreColor(imageData, segmentation);
	const diameterMm = computeDiameter(segmentation, magnification);
	const evolution = previousMask ? scoreEvolution(segmentation, previousMask) : 0;

	const totalScore = asymmetry + border + color + (diameterMm > 6 ? 1 : 0) + evolution;

	return {
		asymmetry,
		border,
		color,
		diameterMm,
		evolution,
		totalScore,
		riskLevel: deriveRiskLevel(totalScore),
		colorsDetected,
	};
}

/**
 * Score asymmetry by comparing halves across both axes.
 * 0 = symmetric, 1 = asymmetric on one axis, 2 = asymmetric on both.
 */
function scoreAsymmetry(seg: SegmentationMask): number {
	const { mask, width, height, boundingBox: bb } = seg;
	if (bb.w === 0 || bb.h === 0) return 0;

	const centerX = bb.x + bb.w / 2;
	const centerY = bb.y + bb.h / 2;

	let mismatchH = 0,
		totalH = 0;
	let mismatchV = 0,
		totalV = 0;

	// Horizontal axis symmetry (top vs bottom)
	for (let y = bb.y; y < centerY; y++) {
		const mirrorY = Math.round(2 * centerY - y);
		if (mirrorY < 0 || mirrorY >= height) continue;
		for (let x = bb.x; x < bb.x + bb.w; x++) {
			totalH++;
			if (mask[y * width + x] !== mask[mirrorY * width + x]) {
				mismatchH++;
			}
		}
	}

	// Vertical axis symmetry (left vs right)
	for (let y = bb.y; y < bb.y + bb.h; y++) {
		for (let x = bb.x; x < centerX; x++) {
			const mirrorX = Math.round(2 * centerX - x);
			if (mirrorX < 0 || mirrorX >= width) continue;
			totalV++;
			if (mask[y * width + x] !== mask[y * width + mirrorX]) {
				mismatchV++;
			}
		}
	}

	const thresholdRatio = 0.2;
	const asymH = totalH > 0 && mismatchH / totalH > thresholdRatio ? 1 : 0;
	const asymV = totalV > 0 && mismatchV / totalV > thresholdRatio ? 1 : 0;

	return asymH + asymV;
}

/**
 * Score border irregularity across 8 radial segments.
 * Each segment scores 0 (regular) or 1 (irregular), max 8.
 */
function scoreBorder(seg: SegmentationMask): number {
	const { mask, width, height, boundingBox: bb } = seg;
	if (bb.w === 0 || bb.h === 0) return 0;

	const cx = bb.x + bb.w / 2;
	const cy = bb.y + bb.h / 2;

	// Collect border pixels
	const borderPixels: Array<{ x: number; y: number; angle: number }> = [];
	for (let y = bb.y; y < bb.y + bb.h; y++) {
		for (let x = bb.x; x < bb.x + bb.w; x++) {
			if (mask[y * width + x] !== 1) continue;
			// Check if it's a border pixel (has a background neighbor)
			let isBorder = false;
			for (const [dx, dy] of [
				[0, 1],
				[0, -1],
				[1, 0],
				[-1, 0],
			]) {
				const nx = x + dx,
					ny = y + dy;
				if (nx < 0 || nx >= width || ny < 0 || ny >= height || mask[ny * width + nx] === 0) {
					isBorder = true;
					break;
				}
			}
			if (isBorder) {
				const angle = Math.atan2(y - cy, x - cx);
				borderPixels.push({ x, y, angle });
			}
		}
	}

	if (borderPixels.length === 0) return 0;

	// Divide into 8 segments (45 degrees each)
	const segments = Array.from({ length: 8 }, () => [] as number[]);
	for (const bp of borderPixels) {
		let normalizedAngle = bp.angle + Math.PI; // [0, 2*PI]
		const segIdx = Math.min(7, Math.floor((normalizedAngle / (2 * Math.PI)) * 8));
		const dist = Math.sqrt((bp.x - cx) ** 2 + (bp.y - cy) ** 2);
		segments[segIdx].push(dist);
	}

	// Score each segment: irregular if coefficient of variation > 0.3
	let irregularCount = 0;
	for (const seg of segments) {
		if (seg.length < 3) continue;
		const mean = seg.reduce((a, b) => a + b, 0) / seg.length;
		if (mean < 1) continue;
		const variance = seg.reduce((a, b) => a + (b - mean) ** 2, 0) / seg.length;
		const cv = Math.sqrt(variance) / mean;
		if (cv > 0.3) irregularCount++;
	}

	return irregularCount;
}

/**
 * Score color variety within the lesion.
 * Counts which of 6 dermoscopic colors are present.
 * Returns score (1-6) and list of detected colors.
 */
function scoreColor(
	imageData: ImageData,
	seg: SegmentationMask
): { score: number; detected: string[] } {
	const { data } = imageData;
	const { mask, width } = seg;
	const colorPresent = new Map<string, number>();

	// Sample lesion pixels
	for (let i = 0; i < mask.length; i++) {
		if (mask[i] !== 1) continue;
		const px = i * 4;
		const r = data[px],
			g = data[px + 1],
			b = data[px + 2];

		for (const [name, range] of Object.entries(ABCDE_COLORS)) {
			if (
				r >= range.min[0] &&
				r <= range.max[0] &&
				g >= range.min[1] &&
				g <= range.max[1] &&
				b >= range.min[2] &&
				b <= range.max[2]
			) {
				colorPresent.set(name, (colorPresent.get(name) || 0) + 1);
			}
		}
	}

	// Only count colors present in at least 5% of lesion pixels
	const minPixels = seg.areaPixels * 0.05;
	const detected = Array.from(colorPresent.entries())
		.filter(([_, count]) => count >= minPixels)
		.map(([name]) => name);

	return {
		score: Math.max(1, Math.min(6, detected.length)),
		detected,
	};
}

/**
 * Compute lesion diameter in millimeters.
 * Uses the bounding box diagonal and known magnification factor.
 *
 * @param seg - Segmentation mask with bounding box
 * @param magnification - DermLite magnification (default 10x)
 * @returns Diameter in millimeters
 */
function computeDiameter(seg: SegmentationMask, magnification: number): number {
	const { boundingBox: bb } = seg;
	// Diagonal of bounding box in pixels
	const diagonalPx = Math.sqrt(bb.w ** 2 + bb.h ** 2);
	// Assume ~40 pixels per mm at 10x magnification (calibration constant)
	const pxPerMm = 4 * magnification;
	return Math.round((diagonalPx / pxPerMm) * 10) / 10;
}

/**
 * Score evolution by comparing current and previous segmentation masks.
 * Returns 0 (no significant change) or 1 (significant change detected).
 */
function scoreEvolution(current: SegmentationMask, previous: SegmentationMask): number {
	if (current.width !== previous.width || current.height !== previous.height) {
		return 0;
	}

	// Compute Jaccard similarity between masks
	let intersection = 0,
		union = 0;
	for (let i = 0; i < current.mask.length; i++) {
		const a = current.mask[i],
			b = previous.mask[i];
		if (a === 1 || b === 1) union++;
		if (a === 1 && b === 1) intersection++;
	}

	const jaccard = union > 0 ? intersection / union : 1;

	// Also check area change
	const areaRatio =
		previous.areaPixels > 0 ? Math.abs(current.areaPixels - previous.areaPixels) / previous.areaPixels : 0;

	// Significant change if Jaccard < 0.8 or area changed > 20%
	return jaccard < 0.8 || areaRatio > 0.2 ? 1 : 0;
}

/**
 * Derive risk level from total ABCDE score.
 *
 * @param totalScore - Combined ABCDE score
 * @returns Risk level classification
 */
function deriveRiskLevel(totalScore: number): RiskLevel {
	if (totalScore <= 3) return "low";
	if (totalScore <= 6) return "moderate";
	if (totalScore <= 9) return "high";
	return "critical";
}
