/**
 * DrAgnes Similar Cases Lookup Endpoint
 *
 * GET /api/similar/[id]
 *
 * Searches the brain for cases similar to a given embedding ID.
 * Supports filtering by body location and lesion class via query params.
 */

import { error, json } from "@sveltejs/kit";
import type { RequestHandler } from "./$types";
import { searchSimilar } from "$lib/dragnes/brain-client";
import type { LesionClass, BodyLocation } from "$lib/dragnes/types";

const VALID_LESION_CLASSES: LesionClass[] = ["akiec", "bcc", "bkl", "df", "mel", "nv", "vasc"];

const VALID_BODY_LOCATIONS: BodyLocation[] = [
	"head",
	"neck",
	"trunk",
	"upper_extremity",
	"lower_extremity",
	"palms_soles",
	"genital",
	"unknown",
];

export const GET: RequestHandler = async ({ params, url }) => {
	const { id } = params;

	if (!id || id.trim().length === 0) {
		throw error(400, "Missing case ID");
	}

	// Parse query parameters
	const k = Math.min(Math.max(parseInt(url.searchParams.get("k") ?? "5", 10) || 5, 1), 50);
	const filterClass = url.searchParams.get("class") as LesionClass | null;
	const filterLocation = url.searchParams.get("location") as BodyLocation | null;

	// Validate filter values if provided
	if (filterClass && !VALID_LESION_CLASSES.includes(filterClass)) {
		throw error(400, `Invalid lesion class filter. Must be one of: ${VALID_LESION_CLASSES.join(", ")}`);
	}

	if (filterLocation && !VALID_BODY_LOCATIONS.includes(filterLocation)) {
		throw error(400, `Invalid body location filter. Must be one of: ${VALID_BODY_LOCATIONS.join(", ")}`);
	}

	try {
		// Use the ID as a seed to create a deterministic lookup embedding.
		// In production this would resolve to the stored embedding for the case.
		const seedEmbedding = idToEmbedding(id);

		// Request more results than needed so we can filter
		const fetchK = filterClass || filterLocation ? k * 3 : k;
		let results = await searchSimilar(seedEmbedding, fetchK);

		// Apply filters
		if (filterClass) {
			results = results.filter((r) => r.lesionClass === filterClass);
		}

		if (filterLocation) {
			results = results.filter((r) => r.bodyLocation === filterLocation);
		}

		// Trim to requested k
		results = results.slice(0, k);

		return json({
			caseId: id,
			similar: results,
			filters: {
				class: filterClass,
				location: filterLocation,
			},
			total: results.length,
		});
	} catch (err) {
		if (err && typeof err === "object" && "status" in err) {
			throw err;
		}

		console.error("[dragnes/similar] Error:", err);
		throw error(500, "Failed to search for similar cases");
	}
};

/**
 * Convert a case ID string into a deterministic embedding for lookup.
 * Uses a simple hash-based approach to generate a stable numeric vector.
 */
function idToEmbedding(id: string, dimensions = 128): number[] {
	const embedding: number[] = [];
	let hash = 0;

	for (let i = 0; i < id.length; i++) {
		hash = (hash * 31 + id.charCodeAt(i)) | 0;
	}

	for (let i = 0; i < dimensions; i++) {
		// Use a deterministic pseudo-random sequence seeded by the hash
		hash = (hash * 1103515245 + 12345) | 0;
		embedding.push(((hash >> 16) & 0x7fff) / 0x7fff - 0.5);
	}

	return embedding;
}
