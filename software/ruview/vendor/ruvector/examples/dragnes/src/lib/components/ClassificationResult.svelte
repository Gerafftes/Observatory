<script lang="ts">
	import type {
		ClassificationResult,
		LesionClass,
		RiskLevel,
	} from "$lib/dragnes/types";
	import { LESION_LABELS } from "$lib/dragnes/types";
	import type { ABCDEScores } from "$lib/dragnes/types";
	import CarbonCheckmark from "~icons/carbon/checkmark";
	import CarbonEdit from "~icons/carbon/edit";

	interface Props {
		result: ClassificationResult;
		abcde?: ABCDEScores;
		onaction?: (action: string, payload?: unknown) => void;
	}

	let { result, abcde, onaction }: Props = $props();

	let showCorrectDropdown: boolean = $state(false);

	const ALL_CLASSES: LesionClass[] = ["akiec", "bcc", "bkl", "df", "mel", "nv", "vasc"];

	function confidenceColor(confidence: number): string {
		if (confidence >= 0.9) return "bg-green-500";
		if (confidence >= 0.7) return "bg-yellow-500";
		return "bg-red-500";
	}

	function riskColor(level: RiskLevel): string {
		const map: Record<RiskLevel, string> = {
			low: "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200",
			moderate: "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200",
			high: "bg-orange-100 text-orange-800 dark:bg-orange-900 dark:text-orange-200",
			critical: "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200",
		};
		return map[level];
	}

	function pct(v: number): string {
		return `${(v * 100).toFixed(1)}%`;
	}

	function handleAction(action: string, payload?: unknown) {
		onaction?.(action, payload);
	}

	function correctTo(cls: LesionClass) {
		showCorrectDropdown = false;
		handleAction("correct", cls);
	}
</script>

<div class="flex flex-col gap-5 w-full">
	<!-- Top prediction -->
	<div class="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
		<div class="mb-2 flex items-center justify-between">
			<h3 class="text-sm font-semibold text-gray-500 dark:text-gray-400">Top Prediction</h3>
			{#if abcde}
				<span
					class="rounded-full px-2.5 py-0.5 text-xs font-semibold {riskColor(abcde.riskLevel)}"
				>
					{abcde.riskLevel.toUpperCase()}
				</span>
			{/if}
		</div>
		<p class="text-lg font-bold text-gray-900 dark:text-gray-100">
			{LESION_LABELS[result.topClass]}
		</p>
		<div class="mt-2 flex items-center gap-2">
			<div class="h-2.5 flex-1 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700">
				<div
					class="h-full rounded-full transition-all {confidenceColor(result.confidence)}"
					style="width: {result.confidence * 100}%"
				></div>
			</div>
			<span class="text-sm font-medium text-gray-600 dark:text-gray-300">
				{pct(result.confidence)}
			</span>
		</div>
		<p class="mt-1 text-xs text-gray-400">
			{result.inferenceTimeMs}ms &middot; {result.usedWasm ? "WASM" : "Demo"}
		</p>
	</div>

	<!-- All class probabilities -->
	<div class="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
		<h3 class="mb-3 text-sm font-semibold text-gray-500 dark:text-gray-400">
			Class Probabilities
		</h3>
		<div class="flex flex-col gap-2">
			{#each result.probabilities as prob}
				<div class="flex items-center gap-2">
					<span
						class="w-10 shrink-0 text-right text-xs font-mono text-gray-500 dark:text-gray-400"
					>
						{prob.className}
					</span>
					<div
						class="h-2 flex-1 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700"
					>
						<div
							class="h-full rounded-full bg-blue-500 dark:bg-blue-400 transition-all"
							style="width: {prob.probability * 100}%"
						></div>
					</div>
					<span class="w-12 text-right text-xs text-gray-500 dark:text-gray-400">
						{pct(prob.probability)}
					</span>
				</div>
			{/each}
		</div>
	</div>

	<!-- ABCDE Breakdown -->
	{#if abcde}
		<div
			class="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800"
		>
			<h3 class="mb-3 text-sm font-semibold text-gray-500 dark:text-gray-400">
				ABCDE Score Breakdown
			</h3>
			<div class="grid grid-cols-5 gap-2 text-center">
				{#each [
					{ key: "A", label: "Asymmetry", val: abcde.asymmetry, max: 2 },
					{ key: "B", label: "Border", val: abcde.border, max: 8 },
					{ key: "C", label: "Color", val: abcde.color, max: 6 },
					{ key: "D", label: "Diameter", val: abcde.diameterMm, max: 10 },
					{ key: "E", label: "Evolution", val: abcde.evolution, max: 2 },
				] as item}
					<div class="flex flex-col items-center gap-1">
						<span class="text-xs text-gray-400">{item.key}</span>
						<div
							class="flex h-10 w-10 items-center justify-center rounded-full border-2 text-sm font-bold
								{item.val / item.max > 0.6
								? 'border-red-400 text-red-600 dark:text-red-400'
								: item.val / item.max > 0.3
									? 'border-yellow-400 text-yellow-600 dark:text-yellow-400'
									: 'border-green-400 text-green-600 dark:text-green-400'}"
						>
							{item.key === "D" ? item.val.toFixed(1) : item.val}
						</div>
						<span class="text-[10px] text-gray-400">{item.label}</span>
					</div>
				{/each}
			</div>
			<div class="mt-3 text-center">
				<span class="text-sm font-medium text-gray-600 dark:text-gray-300">
					Total Score: {abcde.totalScore.toFixed(1)}
				</span>
			</div>
		</div>
	{/if}

	<!-- Similar Cases placeholder -->
	<div class="rounded-xl border border-dashed border-gray-300 bg-gray-50 p-4 text-center dark:border-gray-600 dark:bg-gray-800/50">
		<p class="text-sm text-gray-400">Similar Cases (brain search)</p>
		<p class="text-xs text-gray-300 dark:text-gray-500">Coming soon</p>
	</div>

	<!-- Action buttons -->
	<div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
		<button
			onclick={() => handleAction("confirm")}
			class="flex h-11 items-center justify-center gap-1.5 rounded-lg bg-green-600 text-sm font-medium text-white hover:bg-green-700 active:bg-green-800"
		>
			<CarbonCheckmark class="h-4 w-4" /> Confirm
		</button>

		<div class="relative">
			<button
				onclick={() => (showCorrectDropdown = !showCorrectDropdown)}
				class="flex h-11 w-full items-center justify-center gap-1.5 rounded-lg bg-yellow-600 text-sm font-medium text-white hover:bg-yellow-700 active:bg-yellow-800"
			>
				<CarbonEdit class="h-4 w-4" /> Correct
			</button>
			{#if showCorrectDropdown}
				<div
					class="absolute left-0 top-12 z-10 w-56 rounded-lg border border-gray-200 bg-white p-1 shadow-lg dark:border-gray-600 dark:bg-gray-800"
				>
					{#each ALL_CLASSES as cls}
						<button
							onclick={() => correctTo(cls)}
							class="flex w-full items-center rounded px-3 py-2 text-left text-sm hover:bg-gray-100 dark:hover:bg-gray-700
								{cls === result.topClass ? 'font-semibold text-blue-600 dark:text-blue-400' : 'text-gray-700 dark:text-gray-300'}"
						>
							{cls} &mdash; {LESION_LABELS[cls]}
						</button>
					{/each}
				</div>
			{/if}
		</div>

		<button
			onclick={() => handleAction("biopsy")}
			class="flex h-11 items-center justify-center rounded-lg bg-orange-600 text-sm font-medium text-white hover:bg-orange-700 active:bg-orange-800"
		>
			Biopsy
		</button>

		<button
			onclick={() => handleAction("refer")}
			class="flex h-11 items-center justify-center rounded-lg bg-blue-600 text-sm font-medium text-white hover:bg-blue-700 active:bg-blue-800"
		>
			Refer
		</button>
	</div>

	<button
		onclick={() => handleAction("dismiss")}
		class="self-center text-sm text-gray-400 underline hover:text-gray-600 dark:hover:text-gray-300"
	>
		Dismiss
	</button>
</div>
