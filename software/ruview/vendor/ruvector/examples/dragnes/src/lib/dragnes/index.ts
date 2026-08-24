/**
 * DrAgnes - Dermoscopy CNN Classification Pipeline
 *
 * Browser-based skin lesion classification using MobileNetV3 WASM
 * with ABCDE dermoscopic scoring and privacy-preserving analytics.
 *
 * @module dragnes
 */

// Core classifier
export { DermClassifier } from "./classifier";

// ABCDE scoring
export { computeABCDE } from "./abcde";

// Preprocessing pipeline
export {
	preprocessImage,
	colorNormalize,
	removeHair,
	segmentLesion,
	resizeBilinear,
	toNCHWTensor,
} from "./preprocessing";

// Privacy pipeline
export { PrivacyPipeline } from "./privacy";

// Configuration
export { DRAGNES_CONFIG } from "./config";
export type { DrAgnesConfig } from "./config";

// Types
export type {
	ABCDEScores,
	BodyLocation,
	ClassificationResult,
	ClassProbability,
	DermImage,
	DiagnosisRecord,
	GradCamResult,
	ImageTensor,
	LesionClass,
	LesionClassification,
	PatientEmbedding,
	PrivacyReport,
	RiskLevel,
	SegmentationMask,
	WitnessChain,
} from "./types";

export { LESION_LABELS } from "./types";
