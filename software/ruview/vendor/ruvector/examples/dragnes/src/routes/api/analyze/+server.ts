/**
 * DrAgnes Analysis API Endpoint
 *
 * POST /api/analyze
 *
 * Receives an image embedding (NOT raw image) and returns
 * combined classification context from the brain collective
 * enriched with PubMed literature references.
 */

import { error, json } from "@sveltejs/kit";
import type { RequestHandler } from "./$types";
import { searchSimilar, searchLiterature } from "$lib/dragnes/brain-client";
import type { LesionClass } from "$lib/dragnes/types";

/** In-memory rate limiter: IP -> { count, windowStart } */
const rateLimitMap = new Map<string, { count: number; windowStart: number }>();
const RATE_LIMIT_MAX = 100;
const RATE_LIMIT_WINDOW_MS = 60_000;

function checkRateLimit(ip: string): boolean {
	const now = Date.now();
	const entry = rateLimitMap.get(ip);

	if (!entry || now - entry.windowStart > RATE_LIMIT_WINDOW_MS) {
		rateLimitMap.set(ip, { count: 1, windowStart: now });
		return true;
	}

	if (entry.count >= RATE_LIMIT_MAX) {
		return false;
	}

	entry.count++;
	return true;
}

/** Periodically clean up stale rate limit entries */
setInterval(
	() => {
		const now = Date.now();
		for (const [ip, entry] of rateLimitMap) {
			if (now - entry.windowStart > RATE_LIMIT_WINDOW_MS * 2) {
				rateLimitMap.delete(ip);
			}
		}
	},
	5 * 60_000
);

interface AnalyzeRequest {
	embedding: number[];
	lesionClass?: LesionClass;
	k?: number;
}

export const POST: RequestHandler = async ({ request, getClientAddress }) => {
	// Rate limiting
	const clientIp = getClientAddress();
	if (!checkRateLimit(clientIp)) {
		throw error(429, "Rate limit exceeded. Maximum 100 requests per minute.");
	}

	// Parse request body
	let body: AnalyzeRequest;
	try {
		body = (await request.json()) as AnalyzeRequest;
	} catch {
		throw error(400, "Invalid JSON body");
	}

	// Validate embedding
	if (!body.embedding || !Array.isArray(body.embedding) || body.embedding.length === 0) {
		throw error(400, "Missing or invalid embedding array");
	}

	if (!body.embedding.every((v) => typeof v === "number" && isFinite(v))) {
		throw error(400, "Embedding must contain only finite numbers");
	}

	const k = Math.min(Math.max(body.k ?? 5, 1), 20);

	try {
		// Run brain search and literature lookup in parallel
		const [similarCases, literature] = await Promise.all([
			searchSimilar(body.embedding, k),
			body.lesionClass ? searchLiterature(body.lesionClass) : Promise.resolve([]),
		]);

		// Compute consensus from similar cases
		const classCounts: Record<string, number> = {};
		let totalConfidence = 0;
		let confirmedCount = 0;

		for (const c of similarCases) {
			classCounts[c.lesionClass] = (classCounts[c.lesionClass] ?? 0) + 1;
			totalConfidence += c.confidence;
			if (c.confirmed) confirmedCount++;
		}

		const consensusClass =
			Object.entries(classCounts).sort(([, a], [, b]) => b - a)[0]?.[0] ?? null;

		return json({
			similarCases,
			literature,
			consensus: {
				topClass: consensusClass,
				agreement: similarCases.length > 0 ? (classCounts[consensusClass ?? ""] ?? 0) / similarCases.length : 0,
				averageConfidence: similarCases.length > 0 ? totalConfidence / similarCases.length : 0,
				confirmedCount,
				totalMatches: similarCases.length,
			},
		});
	} catch (err) {
		// Re-throw SvelteKit errors
		if (err && typeof err === "object" && "status" in err) {
			throw err;
		}

		console.error("[dragnes/analyze] Error:", err);
		throw error(500, "Analysis failed. The brain may be temporarily unavailable.");
	}
};
