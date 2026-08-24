/**
 * DrAgnes Feedback API Endpoint
 *
 * POST /api/feedback
 *
 * Handles clinician feedback on classifications:
 *   - confirm: Shares confirmed diagnosis to brain as "solution"
 *   - correct: Records correction for model improvement
 *   - biopsy: Marks case as requiring biopsy confirmation
 */

import { error, json } from "@sveltejs/kit";
import type { RequestHandler } from "./$types";
import { shareDiagnosis } from "$lib/dragnes/brain-client";
import type { LesionClass, BodyLocation } from "$lib/dragnes/types";

type FeedbackAction = "confirm" | "correct" | "biopsy";

interface FeedbackRequest {
	/** Feedback action */
	action: FeedbackAction;
	/** Diagnosis record ID */
	diagnosisId: string;
	/** Image embedding vector */
	embedding: number[];
	/** Original predicted lesion class */
	originalClass: LesionClass;
	/** Corrected class (only for "correct" action) */
	correctedClass?: LesionClass;
	/** Body location */
	bodyLocation: BodyLocation;
	/** Model version */
	modelVersion: string;
	/** Confidence of the original classification */
	confidence: number;
	/** Per-class probabilities */
	probabilities: number[];
	/** Clinical notes (will NOT be sent to brain) */
	notes?: string;
}

const VALID_ACTIONS: FeedbackAction[] = ["confirm", "correct", "biopsy"];

export const POST: RequestHandler = async ({ request }) => {
	let body: FeedbackRequest;
	try {
		body = (await request.json()) as FeedbackRequest;
	} catch {
		throw error(400, "Invalid JSON body");
	}

	// Validate required fields
	if (!body.action || !VALID_ACTIONS.includes(body.action)) {
		throw error(400, `Invalid action. Must be one of: ${VALID_ACTIONS.join(", ")}`);
	}

	if (!body.diagnosisId || typeof body.diagnosisId !== "string") {
		throw error(400, "Missing diagnosisId");
	}

	if (!body.embedding || !Array.isArray(body.embedding) || body.embedding.length === 0) {
		throw error(400, "Missing or invalid embedding");
	}

	if (!body.originalClass) {
		throw error(400, "Missing originalClass");
	}

	if (body.action === "correct" && !body.correctedClass) {
		throw error(400, "correctedClass is required for correct action");
	}

	try {
		let shareResult = null;

		// Determine the effective class and confirmation status
		const effectiveClass =
			body.action === "correct" ? (body.correctedClass as LesionClass) : body.originalClass;
		const isConfirmed = body.action === "confirm";

		// Share to brain for confirm and correct actions (not biopsy — awaiting results)
		if (body.action === "confirm" || body.action === "correct") {
			shareResult = await shareDiagnosis(body.embedding, {
				lesionClass: effectiveClass,
				bodyLocation: body.bodyLocation ?? "unknown",
				modelVersion: body.modelVersion ?? "unknown",
				confidence: body.confidence ?? 0,
				probabilities: body.probabilities ?? [],
				confirmed: isConfirmed,
			});
		}

		// Build response
		const response: Record<string, unknown> = {
			success: true,
			action: body.action,
			diagnosisId: body.diagnosisId,
			effectiveClass,
			confirmed: isConfirmed,
		};

		if (shareResult) {
			response.brainMemoryId = shareResult.memoryId;
			response.witnessHash = shareResult.witnessChain[shareResult.witnessChain.length - 1].hash;
			response.queued = shareResult.queued;
		}

		if (body.action === "correct") {
			response.correction = {
				from: body.originalClass,
				to: body.correctedClass,
			};
		}

		if (body.action === "biopsy") {
			response.awaitingBiopsy = true;
		}

		return json(response);
	} catch (err) {
		if (err && typeof err === "object" && "status" in err) {
			throw err;
		}

		console.error("[dragnes/feedback] Error:", err);
		throw error(500, "Failed to process feedback");
	}
};
