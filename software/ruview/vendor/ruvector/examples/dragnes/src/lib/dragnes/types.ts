/**
 * DrAgnes Type Definitions
 *
 * All TypeScript interfaces for the dermoscopy CNN classification pipeline.
 * Follows ADR-117 type specifications.
 */

/** HAM10000 lesion classes */
export type LesionClass = "akiec" | "bcc" | "bkl" | "df" | "mel" | "nv" | "vasc";

/** Human-readable labels for each lesion class */
export const LESION_LABELS: Record<LesionClass, string> = {
	akiec: "Actinic Keratosis / Intraepithelial Carcinoma",
	bcc: "Basal Cell Carcinoma",
	bkl: "Benign Keratosis",
	df: "Dermatofibroma",
	mel: "Melanoma",
	nv: "Melanocytic Nevus",
	vasc: "Vascular Lesion",
};

/** Risk level derived from ABCDE scoring */
export type RiskLevel = "low" | "moderate" | "high" | "critical";

/** Body location for lesion mapping */
export type BodyLocation =
	| "head"
	| "neck"
	| "trunk"
	| "upper_extremity"
	| "lower_extremity"
	| "palms_soles"
	| "genital"
	| "unknown";

/** Raw dermoscopic image container */
export interface DermImage {
	/** Canvas ImageData (RGBA pixels) */
	imageData: ImageData;
	/** Original width before preprocessing */
	originalWidth: number;
	/** Original height before preprocessing */
	originalHeight: number;
	/** Capture timestamp (ISO 8601) */
	capturedAt: string;
	/** DermLite magnification factor (default 10x) */
	magnification: number;
	/** Body location of the lesion */
	location: BodyLocation;
}

/** Per-class probability in classification result */
export interface ClassProbability {
	/** Lesion class identifier */
	className: LesionClass;
	/** Probability score [0, 1] */
	probability: number;
	/** Human-readable label */
	label: string;
}

/** Full classification result from the CNN */
export interface ClassificationResult {
	/** Top predicted class */
	topClass: LesionClass;
	/** Confidence of top prediction [0, 1] */
	confidence: number;
	/** Probabilities for all 7 classes, sorted descending */
	probabilities: ClassProbability[];
	/** Model identifier used */
	modelId: string;
	/** Inference time in milliseconds */
	inferenceTimeMs: number;
	/** Whether the WASM model was used (vs demo fallback) */
	usedWasm: boolean;
}

/** Grad-CAM attention heatmap result */
export interface GradCamResult {
	/** Heatmap as RGBA ImageData (224x224) */
	heatmap: ImageData;
	/** Overlay of heatmap on original image */
	overlay: ImageData;
	/** Target class the heatmap explains */
	targetClass: LesionClass;
}

/** ABCDE dermoscopic scoring */
export interface ABCDEScores {
	/** Asymmetry score (0-2) */
	asymmetry: number;
	/** Border irregularity score (0-8) */
	border: number;
	/** Color score (1-6) */
	color: number;
	/** Diameter in millimeters */
	diameterMm: number;
	/** Evolution delta score (0 if no previous image) */
	evolution: number;
	/** Total ABCDE score */
	totalScore: number;
	/** Derived risk level */
	riskLevel: RiskLevel;
	/** Colors detected in the lesion */
	colorsDetected: string[];
}

/** Lesion classification record combining CNN + ABCDE */
export interface LesionClassification {
	/** Unique record ID */
	id: string;
	/** CNN classification result */
	classification: ClassificationResult;
	/** ABCDE scoring */
	abcde: ABCDEScores;
	/** Preprocessed image dimensions */
	imageSize: { width: number; height: number };
	/** Timestamp of analysis */
	analyzedAt: string;
}

/** Full diagnosis record for persistence */
export interface DiagnosisRecord {
	/** Unique record ID */
	id: string;
	/** Patient-local pseudonymous ID */
	pseudoId: string;
	/** Lesion classification */
	lesionClassification: LesionClassification;
	/** Body location */
	location: BodyLocation;
	/** Free-text clinical notes (encrypted at rest) */
	notes: string;
	/** Witness chain hash for audit trail */
	witnessHash: string;
	/** Creation timestamp */
	createdAt: string;
}

/** Patient embedding for privacy-preserving analytics */
export interface PatientEmbedding {
	/** Pseudonymous patient ID */
	pseudoId: string;
	/** Differentially private embedding vector */
	embedding: Float32Array;
	/** Epsilon value used for DP noise */
	epsilon: number;
	/** Timestamp of embedding generation */
	generatedAt: string;
}

/** Link in the witness audit chain */
export interface WitnessChain {
	/** Hash of this entry */
	hash: string;
	/** Hash of the previous entry */
	previousHash: string;
	/** Action performed */
	action: string;
	/** Timestamp */
	timestamp: string;
	/** Data hash (SHAKE-256 simulation) */
	dataHash: string;
}

/** Privacy analysis report */
export interface PrivacyReport {
	/** Whether EXIF data was stripped */
	exifStripped: boolean;
	/** PII items detected and removed */
	piiDetected: string[];
	/** Whether DP noise was applied */
	dpNoiseApplied: boolean;
	/** Epsilon used for DP */
	epsilon: number;
	/** k-anonymity check result */
	kAnonymityMet: boolean;
	/** k value used */
	kValue: number;
	/** Witness chain hash */
	witnessHash: string;
}

/** Preprocessed image tensor in NCHW format */
export interface ImageTensor {
	/** Float32 data in NCHW layout [1, 3, 224, 224] */
	data: Float32Array;
	/** Tensor shape */
	shape: [1, 3, 224, 224];
}

/** Lesion segmentation mask */
export interface SegmentationMask {
	/** Binary mask (1 = lesion, 0 = background) */
	mask: Uint8Array;
	/** Mask width */
	width: number;
	/** Mask height */
	height: number;
	/** Bounding box of the lesion */
	boundingBox: { x: number; y: number; w: number; h: number };
	/** Area of the lesion in pixels */
	areaPixels: number;
}
