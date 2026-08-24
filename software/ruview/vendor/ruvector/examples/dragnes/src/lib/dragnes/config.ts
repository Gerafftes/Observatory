/**
 * DrAgnes Configuration
 *
 * Central configuration for the dermoscopy classification pipeline.
 */

import type { LesionClass } from "./types";

export interface DrAgnesConfig {
	modelVersion: string;
	cnnBackbone: string;
	inputSize: number;
	classes: LesionClass[];
	privacy: {
		dpEpsilon: number;
		kAnonymity: number;
		stripExif: boolean;
		localOnly: boolean;
	};
}

export const DRAGNES_CONFIG: DrAgnesConfig = {
	modelVersion: "v1.0.0-demo",
	cnnBackbone: "MobileNetV3-Small",
	inputSize: 224,
	classes: ["akiec", "bcc", "bkl", "df", "mel", "nv", "vasc"],
	privacy: {
		dpEpsilon: 1.0,
		kAnonymity: 5,
		stripExif: true,
		localOnly: true,
	},
};
