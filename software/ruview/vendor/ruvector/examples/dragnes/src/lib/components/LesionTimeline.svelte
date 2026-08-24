<script lang="ts">
	import type { DiagnosisRecord } from "$lib/dragnes/types";
	import { LESION_LABELS } from "$lib/dragnes/types";
	import CarbonCheckmarkFilled from "~icons/carbon/checkmark-filled";
	import CarbonWarning from "~icons/carbon/warning";

	interface Props {
		records: DiagnosisRecord[];
	}

	let { records }: Props = $props();

	function formatDate(iso: string): string {
		try {
			return new Date(iso).toLocaleDateString(undefined, {
				year: "numeric",
				month: "short",
				day: "numeric",
			});
		} catch {
			return iso;
		}
	}

	function confidencePct(val: number): string {
		return `${(val * 100).toFixed(0)}%`;
	}
</script>

<div class="w-full">
	{#if records.length === 0}
		<div
			class="flex flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-gray-300 p-8 dark:border-gray-600"
		>
			<p class="text-sm text-gray-400">No previous records for this lesion</p>
		</div>
	{:else}
		<div class="relative ml-4 border-l-2 border-gray-200 pl-6 dark:border-gray-700">
			{#each records as record, i}
				{@const cls = record.lesionClassification.classification}
				{@const abcde = record.lesionClassification.abcde}
				{@const isLatest = i === 0}

				<div class="relative mb-6 last:mb-0">
					<!-- Timeline dot -->
					<div
						class="absolute -left-[31px] top-1 flex h-4 w-4 items-center justify-center rounded-full
							{isLatest
							? 'bg-blue-500 ring-2 ring-blue-200 dark:ring-blue-800'
							: 'bg-gray-300 dark:bg-gray-600'}"
					>
						{#if isLatest}
							<div class="h-2 w-2 rounded-full bg-white"></div>
						{/if}
					</div>

					<!-- Card -->
					<div
						class="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800
							{isLatest ? 'ring-1 ring-blue-200 dark:ring-blue-800' : ''}"
					>
						<div class="mb-2 flex items-center justify-between">
							<time class="text-xs text-gray-400">{formatDate(record.createdAt)}</time>
							<span
								class="rounded-full px-2 py-0.5 text-[10px] font-semibold
									{abcde.riskLevel === 'low'
									? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
									: abcde.riskLevel === 'moderate'
										? 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900 dark:text-yellow-300'
										: abcde.riskLevel === 'high'
											? 'bg-orange-100 text-orange-700 dark:bg-orange-900 dark:text-orange-300'
											: 'bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300'}"
							>
								{abcde.riskLevel}
							</span>
						</div>

						<p class="text-sm font-semibold text-gray-800 dark:text-gray-200">
							{LESION_LABELS[cls.topClass]}
						</p>
						<p class="text-xs text-gray-500 dark:text-gray-400">
							Confidence: {confidencePct(cls.confidence)} &middot; ABCDE Total: {abcde.totalScore.toFixed(
								1
							)}
						</p>

						{#if record.notes}
							<p class="mt-2 text-xs text-gray-400 italic">{record.notes}</p>
						{/if}

						<!-- Evolution indicator -->
						{#if i > 0 && abcde.evolution > 0}
							<div class="mt-2 flex items-center gap-1 text-xs text-orange-500">
								<CarbonWarning class="h-3 w-3" />
								<span>Evolution detected (delta: {abcde.evolution})</span>
							</div>
						{/if}
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
