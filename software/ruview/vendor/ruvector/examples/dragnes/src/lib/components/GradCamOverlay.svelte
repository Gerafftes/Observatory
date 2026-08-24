<script lang="ts">
	import { onMount } from "svelte";

	interface Props {
		imageData: ImageData;
		gradCamData: Uint8Array;
	}

	let { imageData, gradCamData }: Props = $props();

	let canvasEl: HTMLCanvasElement | undefined = $state(undefined);
	let overlayCanvasEl: HTMLCanvasElement | undefined = $state(undefined);
	let containerEl: HTMLDivElement | undefined = $state(undefined);

	let opacity: number = $state(0.5);
	let showHeatmap: boolean = $state(true);
	let scale: number = $state(1);
	let translateX: number = $state(0);
	let translateY: number = $state(0);

	// Pinch-to-zoom state
	let initialPinchDistance: number = 0;
	let initialScale: number = 1;

	function heatmapColor(value: number): [number, number, number, number] {
		const v = value / 255;
		let r: number, g: number, b: number;

		if (v < 0.25) {
			// blue -> cyan
			r = 0;
			g = Math.round(v * 4 * 255);
			b = 255;
		} else if (v < 0.5) {
			// cyan -> green
			r = 0;
			g = 255;
			b = Math.round((1 - (v - 0.25) * 4) * 255);
		} else if (v < 0.75) {
			// green -> yellow
			r = Math.round((v - 0.5) * 4 * 255);
			g = 255;
			b = 0;
		} else {
			// yellow -> red
			r = 255;
			g = Math.round((1 - (v - 0.75) * 4) * 255);
			b = 0;
		}

		return [r, g, b, Math.round(v * 200)];
	}

	function renderCanvas() {
		if (!canvasEl || !overlayCanvasEl) return;

		// Draw original image
		const ctx = canvasEl.getContext("2d");
		if (!ctx) return;
		canvasEl.width = imageData.width;
		canvasEl.height = imageData.height;
		ctx.putImageData(imageData, 0, 0);

		// Draw heatmap overlay
		const octx = overlayCanvasEl.getContext("2d");
		if (!octx) return;
		overlayCanvasEl.width = imageData.width;
		overlayCanvasEl.height = imageData.height;

		const heatmapImageData = octx.createImageData(imageData.width, imageData.height);

		// gradCamData may be a smaller resolution (e.g., 224x224), scale to image size
		const gcW = Math.round(Math.sqrt(gradCamData.length));
		const gcH = gcW;
		const scaleX = gcW / imageData.width;
		const scaleY = gcH / imageData.height;

		for (let y = 0; y < imageData.height; y++) {
			for (let x = 0; x < imageData.width; x++) {
				const gcX = Math.min(Math.floor(x * scaleX), gcW - 1);
				const gcY = Math.min(Math.floor(y * scaleY), gcH - 1);
				const gcIdx = gcY * gcW + gcX;
				const val = gradCamData[gcIdx] ?? 0;
				const [r, g, b, a] = heatmapColor(val);

				const idx = (y * imageData.width + x) * 4;
				heatmapImageData.data[idx] = r;
				heatmapImageData.data[idx + 1] = g;
				heatmapImageData.data[idx + 2] = b;
				heatmapImageData.data[idx + 3] = a;
			}
		}

		octx.putImageData(heatmapImageData, 0, 0);
	}

	$effect(() => {
		// Re-render when imageData or gradCamData changes
		if (imageData && gradCamData) {
			renderCanvas();
		}
	});

	onMount(() => {
		renderCanvas();
	});

	function handleTouchStart(e: TouchEvent) {
		if (e.touches.length === 2) {
			const dx = e.touches[0].clientX - e.touches[1].clientX;
			const dy = e.touches[0].clientY - e.touches[1].clientY;
			initialPinchDistance = Math.sqrt(dx * dx + dy * dy);
			initialScale = scale;
		}
	}

	function handleTouchMove(e: TouchEvent) {
		if (e.touches.length === 2) {
			e.preventDefault();
			const dx = e.touches[0].clientX - e.touches[1].clientX;
			const dy = e.touches[0].clientY - e.touches[1].clientY;
			const distance = Math.sqrt(dx * dx + dy * dy);
			const newScale = initialScale * (distance / initialPinchDistance);
			scale = Math.max(1, Math.min(5, newScale));
		}
	}

	function handleTouchEnd() {
		if (scale <= 1.05) {
			scale = 1;
			translateX = 0;
			translateY = 0;
		}
	}
</script>

<div class="flex flex-col gap-3 w-full">
	<!-- Canvas container with pinch-to-zoom -->
	<div
		bind:this={containerEl}
		class="relative aspect-square w-full overflow-hidden rounded-xl bg-gray-900"
		ontouchstart={handleTouchStart}
		ontouchmove={handleTouchMove}
		ontouchend={handleTouchEnd}
		role="img"
		aria-label="Dermoscopic image with Grad-CAM heatmap overlay"
	>
		<canvas
			bind:this={canvasEl}
			class="absolute inset-0 h-full w-full object-contain"
			style="transform: scale({scale}) translate({translateX}px, {translateY}px)"
		></canvas>
		{#if showHeatmap}
			<canvas
				bind:this={overlayCanvasEl}
				class="absolute inset-0 h-full w-full object-contain"
				style="opacity: {opacity}; transform: scale({scale}) translate({translateX}px, {translateY}px)"
			></canvas>
		{/if}
	</div>

	<!-- Controls -->
	<div class="flex items-center gap-3">
		<button
			onclick={() => (showHeatmap = !showHeatmap)}
			class="shrink-0 rounded-lg px-3 py-2 text-sm font-medium transition-colors
				{showHeatmap
				? 'bg-blue-600 text-white'
				: 'bg-gray-200 text-gray-700 dark:bg-gray-700 dark:text-gray-300'}"
		>
			{showHeatmap ? "Hide" : "Show"} Heatmap
		</button>

		{#if showHeatmap}
			<label class="flex flex-1 items-center gap-2">
				<span class="text-xs text-gray-500 dark:text-gray-400">Opacity</span>
				<input
					type="range"
					min="0"
					max="1"
					step="0.05"
					bind:value={opacity}
					class="h-2 flex-1 cursor-pointer appearance-none rounded-full bg-gray-300 dark:bg-gray-600
						[&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-blue-500"
				/>
			</label>
		{/if}
	</div>

	<!-- Color scale legend -->
	{#if showHeatmap}
		<div class="flex items-center gap-2 text-xs text-gray-400">
			<span>Low</span>
			<div
				class="h-2 flex-1 rounded-full"
				style="background: linear-gradient(to right, #0000ff, #00ffff, #00ff00, #ffff00, #ff0000)"
			></div>
			<span>High</span>
		</div>
	{/if}
</div>
