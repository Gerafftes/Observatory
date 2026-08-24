<script lang="ts">
	import type { ABCDEScores, RiskLevel } from "$lib/dragnes/types";

	interface Props {
		scores: ABCDEScores;
	}

	let { scores }: Props = $props();

	const SIZE = 200;
	const CENTER = SIZE / 2;
	const RADIUS = 70;
	const AXES = [
		{ key: "asymmetry" as const, label: "A", max: 2 },
		{ key: "border" as const, label: "B", max: 8 },
		{ key: "color" as const, label: "C", max: 6 },
		{ key: "diameterMm" as const, label: "D", max: 10 },
		{ key: "evolution" as const, label: "E", max: 2 },
	];

	const ANGLE_OFFSET = -Math.PI / 2; // Start from top
	const STEP = (2 * Math.PI) / AXES.length;

	// Concerning threshold at 50% of max for each axis
	const THRESHOLD = 0.5;

	function getPoint(index: number, ratio: number): { x: number; y: number } {
		const angle = ANGLE_OFFSET + index * STEP;
		return {
			x: CENTER + Math.cos(angle) * RADIUS * ratio,
			y: CENTER + Math.sin(angle) * RADIUS * ratio,
		};
	}

	function polygon(ratios: number[]): string {
		return ratios.map((r, i) => {
			const p = getPoint(i, r);
			return `${p.x},${p.y}`;
		}).join(" ");
	}

	function getLabelPos(index: number): { x: number; y: number } {
		const angle = ANGLE_OFFSET + index * STEP;
		const r = RADIUS + 18;
		return {
			x: CENTER + Math.cos(angle) * r,
			y: CENTER + Math.sin(angle) * r,
		};
	}

	function riskFillColor(level: RiskLevel): string {
		const map: Record<RiskLevel, string> = {
			low: "rgba(34,197,94,0.25)",
			moderate: "rgba(234,179,8,0.25)",
			high: "rgba(249,115,22,0.25)",
			critical: "rgba(239,68,68,0.3)",
		};
		return map[level];
	}

	function riskStrokeColor(level: RiskLevel): string {
		const map: Record<RiskLevel, string> = {
			low: "#22c55e",
			moderate: "#eab308",
			high: "#f97316",
			critical: "#ef4444",
		};
		return map[level];
	}

	$effect(() => {
		// Reactivity hook for scores
		scores;
	});

	const valueRatios = $derived(
		AXES.map((axis) => {
			const val = scores[axis.key];
			return Math.min(val / axis.max, 1);
		})
	);

	const thresholdRatios = AXES.map(() => THRESHOLD);

	const gridLevels = [0.25, 0.5, 0.75, 1.0];
</script>

<div class="flex flex-col items-center gap-3 w-full">
	<svg
		viewBox="0 0 {SIZE} {SIZE}"
		class="w-full max-w-[250px]"
		role="img"
		aria-label="ABCDE radar chart"
	>
		<!-- Grid circles -->
		{#each gridLevels as level}
			<polygon
				points={polygon(AXES.map(() => level))}
				fill="none"
				stroke="currentColor"
				stroke-width="0.5"
				class="text-gray-200 dark:text-gray-700"
			/>
		{/each}

		<!-- Axis lines -->
		{#each AXES as _, i}
			{@const p = getPoint(i, 1)}
			<line
				x1={CENTER}
				y1={CENTER}
				x2={p.x}
				y2={p.y}
				stroke="currentColor"
				stroke-width="0.5"
				class="text-gray-200 dark:text-gray-700"
			/>
		{/each}

		<!-- Threshold polygon (concerning line) -->
		<polygon
			points={polygon(thresholdRatios)}
			fill="none"
			stroke="#f97316"
			stroke-width="1"
			stroke-dasharray="3,3"
			opacity="0.5"
		/>

		<!-- Data polygon -->
		<polygon
			points={polygon(valueRatios)}
			fill={riskFillColor(scores.riskLevel)}
			stroke={riskStrokeColor(scores.riskLevel)}
			stroke-width="2"
		/>

		<!-- Data points -->
		{#each valueRatios as ratio, i}
			{@const p = getPoint(i, ratio)}
			<circle
				cx={p.x}
				cy={p.y}
				r="3"
				fill={riskStrokeColor(scores.riskLevel)}
			/>
		{/each}

		<!-- Labels -->
		{#each AXES as axis, i}
			{@const pos = getLabelPos(i)}
			<text
				x={pos.x}
				y={pos.y}
				text-anchor="middle"
				dominant-baseline="central"
				class="fill-gray-600 text-[11px] font-semibold dark:fill-gray-400"
			>
				{axis.label}
			</text>
		{/each}
	</svg>

	<!-- Total score -->
	<div class="text-center">
		<span
			class="inline-block rounded-full px-3 py-1 text-sm font-bold
				{scores.riskLevel === 'low'
				? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
				: scores.riskLevel === 'moderate'
					? 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900 dark:text-yellow-300'
					: scores.riskLevel === 'high'
						? 'bg-orange-100 text-orange-700 dark:bg-orange-900 dark:text-orange-300'
						: 'bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300'}"
		>
			Total: {scores.totalScore.toFixed(1)}
		</span>
		<p class="mt-1 text-xs text-gray-400">
			Dashed line = concerning threshold
		</p>
	</div>
</div>
