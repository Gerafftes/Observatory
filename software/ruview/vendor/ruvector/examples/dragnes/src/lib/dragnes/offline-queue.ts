/**
 * Offline Sync Queue for DrAgnes Brain Contributions
 *
 * Uses IndexedDB to persist brain contributions when the device is offline.
 * Automatically syncs when connectivity returns, with exponential backoff
 * on failures.
 */

/** A queued brain contribution awaiting sync */
export interface QueuedContribution {
	/** Unique queue entry ID */
	id: string;
	/** Brain API endpoint path */
	endpoint: string;
	/** HTTP method */
	method: "POST" | "PUT";
	/** Request body */
	body: Record<string, unknown>;
	/** Number of sync attempts so far */
	attempts: number;
	/** Timestamp when first queued (ISO 8601) */
	queuedAt: string;
	/** Timestamp of last failed attempt (ISO 8601), or null if never attempted */
	lastAttemptAt: string | null;
}

/** Current status of the offline queue */
export interface QueueStatus {
	/** Number of items waiting to sync */
	pending: number;
	/** Whether a sync is currently in progress */
	syncing: boolean;
	/** Timestamp of last successful sync */
	lastSyncAt: string | null;
	/** Number of items that failed on last attempt */
	failedCount: number;
}

const DB_NAME = "dragnes-offline-queue";
const DB_VERSION = 1;
const STORE_NAME = "contributions";
const MAX_ATTEMPTS = 8;
const BASE_DELAY_MS = 1000;

/**
 * Opens (or creates) the IndexedDB database for the queue.
 */
function openDB(): Promise<IDBDatabase> {
	return new Promise((resolve, reject) => {
		const request = indexedDB.open(DB_NAME, DB_VERSION);

		request.onupgradeneeded = () => {
			const db = request.result;
			if (!db.objectStoreNames.contains(STORE_NAME)) {
				db.createObjectStore(STORE_NAME, { keyPath: "id" });
			}
		};

		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
	});
}

/**
 * Generate a unique ID for queue entries.
 */
function generateId(): string {
	return `q_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Calculate exponential backoff delay in milliseconds.
 */
function backoffDelay(attempt: number): number {
	return Math.min(BASE_DELAY_MS * Math.pow(2, attempt), 60_000);
}

/**
 * OfflineQueue manages brain contributions that could not be sent immediately.
 *
 * Usage:
 *   const queue = new OfflineQueue("https://pi.ruv.io");
 *   await queue.enqueue("/v1/memories", { title: "...", ... });
 *   await queue.sync(); // or let the online listener handle it
 */
export class OfflineQueue {
	private brainBaseUrl: string;
	private syncing = false;
	private lastSyncAt: string | null = null;
	private failedCount = 0;
	private onlineHandler: (() => void) | null = null;

	constructor(brainBaseUrl: string) {
		this.brainBaseUrl = brainBaseUrl.replace(/\/$/, "");
		this.registerOnlineListener();
	}

	/**
	 * Add a contribution to the offline queue.
	 *
	 * @param endpoint - API path (e.g. "/v1/memories")
	 * @param body - Request body to send when online
	 * @param method - HTTP method (default POST)
	 */
	async enqueue(
		endpoint: string,
		body: Record<string, unknown>,
		method: "POST" | "PUT" = "POST"
	): Promise<void> {
		const db = await openDB();
		const entry: QueuedContribution = {
			id: generateId(),
			endpoint,
			method,
			body,
			attempts: 0,
			queuedAt: new Date().toISOString(),
			lastAttemptAt: null,
		};

		return new Promise((resolve, reject) => {
			const tx = db.transaction(STORE_NAME, "readwrite");
			tx.objectStore(STORE_NAME).add(entry);
			tx.oncomplete = () => {
				db.close();
				resolve();
			};
			tx.onerror = () => {
				db.close();
				reject(tx.error);
			};
		});
	}

	/**
	 * Attempt to sync all queued contributions to the brain.
	 * Uses exponential backoff per item on failure.
	 * Items that exceed MAX_ATTEMPTS are discarded.
	 *
	 * @returns Number of successfully synced items
	 */
	async sync(): Promise<number> {
		if (this.syncing) {
			return 0;
		}

		this.syncing = true;
		this.failedCount = 0;
		let synced = 0;

		try {
			const db = await openDB();
			const items = await this.getAllItems(db);
			db.close();

			for (const item of items) {
				// Check if enough time has passed since last attempt (backoff)
				if (item.lastAttemptAt) {
					const elapsed = Date.now() - new Date(item.lastAttemptAt).getTime();
					const requiredDelay = backoffDelay(item.attempts);
					if (elapsed < requiredDelay) {
						continue;
					}
				}

				try {
					const response = await fetch(`${this.brainBaseUrl}${item.endpoint}`, {
						method: item.method,
						headers: { "Content-Type": "application/json" },
						body: JSON.stringify(item.body),
					});

					if (response.ok) {
						await this.removeItem(item.id);
						synced++;
					} else {
						await this.markAttempt(item);
					}
				} catch {
					await this.markAttempt(item);
				}
			}

			if (synced > 0) {
				this.lastSyncAt = new Date().toISOString();
			}
		} finally {
			this.syncing = false;
		}

		return synced;
	}

	/**
	 * Get the current queue status.
	 */
	async getStatus(): Promise<QueueStatus> {
		try {
			const db = await openDB();
			const count = await this.getCount(db);
			db.close();

			return {
				pending: count,
				syncing: this.syncing,
				lastSyncAt: this.lastSyncAt,
				failedCount: this.failedCount,
			};
		} catch {
			return {
				pending: 0,
				syncing: this.syncing,
				lastSyncAt: this.lastSyncAt,
				failedCount: this.failedCount,
			};
		}
	}

	/**
	 * Remove the online event listener. Call when disposing the queue.
	 */
	destroy(): void {
		if (this.onlineHandler && typeof window !== "undefined") {
			window.removeEventListener("online", this.onlineHandler);
			this.onlineHandler = null;
		}
	}

	// ---- Private helpers ----

	private registerOnlineListener(): void {
		if (typeof window === "undefined") {
			return;
		}

		this.onlineHandler = () => {
			void this.sync();
		};
		window.addEventListener("online", this.onlineHandler);
	}

	private getAllItems(db: IDBDatabase): Promise<QueuedContribution[]> {
		return new Promise((resolve, reject) => {
			const tx = db.transaction(STORE_NAME, "readonly");
			const request = tx.objectStore(STORE_NAME).getAll();
			request.onsuccess = () => resolve(request.result as QueuedContribution[]);
			request.onerror = () => reject(request.error);
		});
	}

	private getCount(db: IDBDatabase): Promise<number> {
		return new Promise((resolve, reject) => {
			const tx = db.transaction(STORE_NAME, "readonly");
			const request = tx.objectStore(STORE_NAME).count();
			request.onsuccess = () => resolve(request.result);
			request.onerror = () => reject(request.error);
		});
	}

	private async removeItem(id: string): Promise<void> {
		const db = await openDB();
		return new Promise((resolve, reject) => {
			const tx = db.transaction(STORE_NAME, "readwrite");
			tx.objectStore(STORE_NAME).delete(id);
			tx.oncomplete = () => {
				db.close();
				resolve();
			};
			tx.onerror = () => {
				db.close();
				reject(tx.error);
			};
		});
	}

	private async markAttempt(item: QueuedContribution): Promise<void> {
		const updated: QueuedContribution = {
			...item,
			attempts: item.attempts + 1,
			lastAttemptAt: new Date().toISOString(),
		};

		// Discard items that have exceeded max attempts
		if (updated.attempts >= MAX_ATTEMPTS) {
			await this.removeItem(item.id);
			this.failedCount++;
			return;
		}

		const db = await openDB();
		return new Promise((resolve, reject) => {
			const tx = db.transaction(STORE_NAME, "readwrite");
			tx.objectStore(STORE_NAME).put(updated);
			tx.oncomplete = () => {
				db.close();
				this.failedCount++;
				resolve();
			};
			tx.onerror = () => {
				db.close();
				reject(tx.error);
			};
		});
	}
}
