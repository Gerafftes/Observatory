/**
 * DrAgnes Image Preprocessing Pipeline
 *
 * Provides color normalization, hair removal, lesion segmentation,
 * resizing, and ImageNet normalization for dermoscopic images.
 * All operations work on Canvas ImageData (browser-compatible).
 */

import type { ImageTensor, SegmentationMask } from "./types";

/** ImageNet channel means (RGB) */
const IMAGENET_MEAN = [0.485, 0.456, 0.406];
/** ImageNet channel standard deviations (RGB) */
const IMAGENET_STD = [0.229, 0.224, 0.225];
/** Target model input size */
const TARGET_SIZE = 224;

/**
 * Full preprocessing pipeline: normalize color, remove hair,
 * segment lesion, resize to 224x224, and produce NCHW tensor.
 *
 * @param imageData - Raw RGBA ImageData from canvas
 * @returns Preprocessed image tensor in NCHW format
 */
export async function preprocessImage(imageData: ImageData): Promise<ImageTensor> {
	let processed = colorNormalize(imageData);
	processed = removeHair(processed);
	const resized = resizeBilinear(processed, TARGET_SIZE, TARGET_SIZE);
	return toNCHWTensor(resized);
}

/**
 * Shades of Gray color normalization.
 * Estimates illuminant using Minkowski norm (p=6) and
 * normalizes each channel to a reference white.
 *
 * @param imageData - Input RGBA ImageData
 * @returns Color-normalized ImageData
 */
export function colorNormalize(imageData: ImageData): ImageData {
	const { data, width, height } = imageData;
	const result = new Uint8ClampedArray(data.length);
	const p = 6;
	const pixelCount = width * height;

	// Compute Minkowski norm per channel
	let sumR = 0,
		sumG = 0,
		sumB = 0;
	for (let i = 0; i < data.length; i += 4) {
		sumR += Math.pow(data[i] / 255, p);
		sumG += Math.pow(data[i + 1] / 255, p);
		sumB += Math.pow(data[i + 2] / 255, p);
	}

	const normR = Math.pow(sumR / pixelCount, 1 / p);
	const normG = Math.pow(sumG / pixelCount, 1 / p);
	const normB = Math.pow(sumB / pixelCount, 1 / p);
	const maxNorm = Math.max(normR, normG, normB, 1e-10);

	const scaleR = maxNorm / Math.max(normR, 1e-10);
	const scaleG = maxNorm / Math.max(normG, 1e-10);
	const scaleB = maxNorm / Math.max(normB, 1e-10);

	for (let i = 0; i < data.length; i += 4) {
		result[i] = Math.min(255, Math.round(data[i] * scaleR));
		result[i + 1] = Math.min(255, Math.round(data[i + 1] * scaleG));
		result[i + 2] = Math.min(255, Math.round(data[i + 2] * scaleB));
		result[i + 3] = data[i + 3];
	}

	return new ImageData(result, width, height);
}

/**
 * DullRazor-style hair removal simulation.
 * Detects dark thin structures (potential hairs) using
 * morphological blackhat filtering approximation, then
 * inpaints them with surrounding pixel averages.
 *
 * @param imageData - Input RGBA ImageData
 * @returns ImageData with hair artifacts reduced
 */
export function removeHair(imageData: ImageData): ImageData {
	const { data, width, height } = imageData;
	const result = new Uint8ClampedArray(data);

	// Convert to grayscale for detection
	const gray = new Uint8Array(width * height);
	for (let i = 0; i < gray.length; i++) {
		const idx = i * 4;
		gray[i] = Math.round(0.299 * data[idx] + 0.587 * data[idx + 1] + 0.114 * data[idx + 2]);
	}

	// Detect hair-like pixels: dark, thin structures
	// Use directional variance — hair pixels have high variance in one direction
	const hairMask = new Uint8Array(width * height);
	const kernelSize = 5;
	const halfK = Math.floor(kernelSize / 2);

	for (let y = halfK; y < height - halfK; y++) {
		for (let x = halfK; x < width - halfK; x++) {
			const idx = y * width + x;
			const centerVal = gray[idx];

			// Skip bright pixels (not hair)
			if (centerVal > 80) continue;

			// Check horizontal and vertical line patterns
			let hCount = 0;
			let vCount = 0;
			for (let k = -halfK; k <= halfK; k++) {
				if (gray[y * width + (x + k)] < 80) hCount++;
				if (gray[(y + k) * width + x] < 80) vCount++;
			}

			// Hair-like if dark in one direction but not the other
			const isHorizontalHair = hCount >= kernelSize - 1 && vCount <= 2;
			const isVerticalHair = vCount >= kernelSize - 1 && hCount <= 2;

			if (isHorizontalHair || isVerticalHair) {
				hairMask[idx] = 1;
			}
		}
	}

	// Inpaint hair pixels with average of non-hair neighbors
	const radius = 3;
	for (let y = radius; y < height - radius; y++) {
		for (let x = radius; x < width - radius; x++) {
			const idx = y * width + x;
			if (hairMask[idx] !== 1) continue;

			let sumR = 0,
				sumG = 0,
				sumB = 0,
				count = 0;
			for (let dy = -radius; dy <= radius; dy++) {
				for (let dx = -radius; dx <= radius; dx++) {
					const ni = (y + dy) * width + (x + dx);
					if (hairMask[ni] === 0) {
						const pi = ni * 4;
						sumR += data[pi];
						sumG += data[pi + 1];
						sumB += data[pi + 2];
						count++;
					}
				}
			}
			if (count > 0) {
				const pi = idx * 4;
				result[pi] = Math.round(sumR / count);
				result[pi + 1] = Math.round(sumG / count);
				result[pi + 2] = Math.round(sumB / count);
			}
		}
	}

	return new ImageData(result, width, height);
}

/**
 * Otsu thresholding + morphological operations for lesion segmentation.
 *
 * @param imageData - Input RGBA ImageData
 * @returns Binary segmentation mask with bounding box
 */
export function segmentLesion(imageData: ImageData): SegmentationMask {
	const { data, width, height } = imageData;

	// Convert to grayscale
	const gray = new Uint8Array(width * height);
	for (let i = 0; i < gray.length; i++) {
		const idx = i * 4;
		gray[i] = Math.round(0.299 * data[idx] + 0.587 * data[idx + 1] + 0.114 * data[idx + 2]);
	}

	// Otsu's threshold
	const threshold = otsuThreshold(gray);

	// Binary mask (lesion = darker than or equal to threshold)
	const mask = new Uint8Array(width * height);
	for (let i = 0; i < gray.length; i++) {
		mask[i] = gray[i] <= threshold ? 1 : 0;
	}

	// Morphological closing (dilate then erode) to fill gaps
	const closed = morphClose(mask, width, height, 3);

	// Compute bounding box and area
	let minX = width,
		minY = height,
		maxX = 0,
		maxY = 0;
	let area = 0;
	for (let y = 0; y < height; y++) {
		for (let x = 0; x < width; x++) {
			if (closed[y * width + x] === 1) {
				area++;
				if (x < minX) minX = x;
				if (x > maxX) maxX = x;
				if (y < minY) minY = y;
				if (y > maxY) maxY = y;
			}
		}
	}

	return {
		mask: closed,
		width,
		height,
		boundingBox: {
			x: minX,
			y: minY,
			w: Math.max(0, maxX - minX + 1),
			h: Math.max(0, maxY - minY + 1),
		},
		areaPixels: area,
	};
}

/**
 * Otsu's method for automatic threshold selection.
 * Maximizes inter-class variance of foreground/background.
 */
function otsuThreshold(gray: Uint8Array): number {
	const histogram = new Int32Array(256);
	for (let i = 0; i < gray.length; i++) {
		histogram[gray[i]]++;
	}

	const total = gray.length;
	let sumAll = 0;
	for (let i = 0; i < 256; i++) sumAll += i * histogram[i];

	let sumBg = 0;
	let weightBg = 0;
	let maxVariance = 0;
	let bestThreshold = 0;

	for (let t = 0; t < 256; t++) {
		weightBg += histogram[t];
		if (weightBg === 0) continue;
		const weightFg = total - weightBg;
		if (weightFg === 0) break;

		sumBg += t * histogram[t];
		const meanBg = sumBg / weightBg;
		const meanFg = (sumAll - sumBg) / weightFg;
		const variance = weightBg * weightFg * (meanBg - meanFg) * (meanBg - meanFg);

		if (variance > maxVariance) {
			maxVariance = variance;
			bestThreshold = t;
		}
	}

	return bestThreshold;
}

/**
 * Morphological closing: dilate then erode.
 */
function morphClose(mask: Uint8Array, width: number, height: number, radius: number): Uint8Array {
	return morphErode(morphDilate(mask, width, height, radius), width, height, radius);
}

function morphDilate(mask: Uint8Array, w: number, h: number, r: number): Uint8Array {
	const out = new Uint8Array(w * h);
	for (let y = 0; y < h; y++) {
		for (let x = 0; x < w; x++) {
			let val = 0;
			for (let dy = -r; dy <= r && !val; dy++) {
				for (let dx = -r; dx <= r && !val; dx++) {
					const ny = y + dy,
						nx = x + dx;
					if (ny >= 0 && ny < h && nx >= 0 && nx < w && mask[ny * w + nx] === 1) {
						val = 1;
					}
				}
			}
			out[y * w + x] = val;
		}
	}
	return out;
}

function morphErode(mask: Uint8Array, w: number, h: number, r: number): Uint8Array {
	const out = new Uint8Array(w * h);
	for (let y = 0; y < h; y++) {
		for (let x = 0; x < w; x++) {
			let val = 1;
			for (let dy = -r; dy <= r && val; dy++) {
				for (let dx = -r; dx <= r && val; dx++) {
					const ny = y + dy,
						nx = x + dx;
					if (ny < 0 || ny >= h || nx < 0 || nx >= w || mask[ny * w + nx] === 0) {
						val = 0;
					}
				}
			}
			out[y * w + x] = val;
		}
	}
	return out;
}

/**
 * Bilinear interpolation resize.
 *
 * @param imageData - Input RGBA ImageData
 * @param targetW - Target width
 * @param targetH - Target height
 * @returns Resized ImageData
 */
export function resizeBilinear(imageData: ImageData, targetW: number, targetH: number): ImageData {
	const { data, width: srcW, height: srcH } = imageData;
	const result = new Uint8ClampedArray(targetW * targetH * 4);

	const xRatio = srcW / targetW;
	const yRatio = srcH / targetH;

	for (let y = 0; y < targetH; y++) {
		for (let x = 0; x < targetW; x++) {
			const srcX = x * xRatio;
			const srcY = y * yRatio;
			const x0 = Math.floor(srcX);
			const y0 = Math.floor(srcY);
			const x1 = Math.min(x0 + 1, srcW - 1);
			const y1 = Math.min(y0 + 1, srcH - 1);
			const dx = srcX - x0;
			const dy = srcY - y0;

			const dstIdx = (y * targetW + x) * 4;
			for (let c = 0; c < 4; c++) {
				const topLeft = data[(y0 * srcW + x0) * 4 + c];
				const topRight = data[(y0 * srcW + x1) * 4 + c];
				const botLeft = data[(y1 * srcW + x0) * 4 + c];
				const botRight = data[(y1 * srcW + x1) * 4 + c];

				const top = topLeft + (topRight - topLeft) * dx;
				const bot = botLeft + (botRight - botLeft) * dx;
				result[dstIdx + c] = Math.round(top + (bot - top) * dy);
			}
		}
	}

	return new ImageData(result, targetW, targetH);
}

/**
 * Convert RGBA ImageData to NCHW Float32 tensor with ImageNet normalization.
 *
 * @param imageData - 224x224 RGBA ImageData
 * @returns NCHW tensor [1, 3, 224, 224] normalized to ImageNet stats
 */
export function toNCHWTensor(imageData: ImageData): ImageTensor {
	const { data, width, height } = imageData;
	const channelSize = width * height;
	const tensorData = new Float32Array(3 * channelSize);

	for (let i = 0; i < channelSize; i++) {
		const px = i * 4;
		// R channel
		tensorData[i] = (data[px] / 255 - IMAGENET_MEAN[0]) / IMAGENET_STD[0];
		// G channel
		tensorData[channelSize + i] = (data[px + 1] / 255 - IMAGENET_MEAN[1]) / IMAGENET_STD[1];
		// B channel
		tensorData[2 * channelSize + i] = (data[px + 2] / 255 - IMAGENET_MEAN[2]) / IMAGENET_STD[2];
	}

	return {
		data: tensorData,
		shape: [1, 3, 224, 224],
	};
}
