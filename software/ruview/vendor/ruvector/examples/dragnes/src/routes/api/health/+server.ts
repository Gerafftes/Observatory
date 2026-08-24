import { json } from "@sveltejs/kit";
import { DRAGNES_CONFIG } from "$lib/dragnes/config";

export async function GET() {
	return json({
		status: "ok",
		version: DRAGNES_CONFIG.modelVersion,
		backbone: DRAGNES_CONFIG.cnnBackbone,
		classes: DRAGNES_CONFIG.classes.length,
		privacy: {
			dpEpsilon: DRAGNES_CONFIG.privacy.dpEpsilon,
			kAnonymity: DRAGNES_CONFIG.privacy.kAnonymity,
		},
		timestamp: new Date().toISOString(),
	});
}
