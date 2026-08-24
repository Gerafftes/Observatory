<script lang="ts">
	import { onMount, onDestroy } from "svelte";
	import CarbonCamera from "~icons/carbon/camera";
	import CarbonSwitchLayer from "~icons/carbon/switch-layer-2";
	import type { BodyLocation } from "$lib/dragnes/types";

	interface CaptureEvent {
		imageData: ImageData;
		bodyLocation: BodyLocation;
		deviceModel: string;
	}

	interface Props {
		oncapture: (event: CaptureEvent) => void;
	}

	let { oncapture }: Props = $props();

	let videoEl: HTMLVideoElement | undefined = $state(undefined);
	let canvasEl: HTMLCanvasElement | undefined = $state(undefined);
	let stream: MediaStream | null = $state(null);
	let cameraReady: boolean = $state(false);
	let cameraError: string = $state("");

	let bodyLocation: BodyLocation = $state("unknown");
	let deviceModel: string = $state("phone_only");
	let capturedPreview: string | null = $state(null);

	const BODY_LOCATIONS: { value: BodyLocation; label: string }[] = [
		{ value: "head", label: "Head" },
		{ value: "neck", label: "Neck" },
		{ value: "trunk", label: "Trunk" },
		{ value: "upper_extremity", label: "Upper Extremity" },
		{ value: "lower_extremity", label: "Lower Extremity" },
		{ value: "palms_soles", label: "Hands / Feet" },
		{ value: "genital", label: "Genitalia" },
		{ value: "unknown", label: "Unknown" },
	];

	const DEVICE_MODELS = [
		{ value: "phone_only", label: "Phone Only" },
		{ value: "HUD", label: "DermLite HUD" },
		{ value: "DL5", label: "DermLite DL5" },
		{ value: "DL4", label: "DermLite DL4" },
		{ value: "DL200", label: "DermLite DL200" },
	];

	function getConstraints(): MediaStreamConstraints {
		const isDermLite = deviceModel !== "phone_only";
		return {
			video: {
				facingMode: "environment",
				width: { ideal: isDermLite ? 1920 : 1280 },
				height: { ideal: isDermLite ? 1080 : 720 },
			},
			audio: false,
		};
	}

	async function startCamera() {
		cameraError = "";
		cameraReady = false;
		capturedPreview = null;

		try {
			if (stream) {
				stream.getTracks().forEach((t) => t.stop());
			}
			stream = await navigator.mediaDevices.getUserMedia(getConstraints());
			if (videoEl) {
				videoEl.srcObject = stream;
				await videoEl.play();
				cameraReady = true;
			}
		} catch (err) {
			cameraError =
				err instanceof Error ? err.message : "Camera access denied or unavailable.";
		}
	}

	function captureFrame() {
		if (!videoEl || !canvasEl || !cameraReady) return;

		const ctx = canvasEl.getContext("2d");
		if (!ctx) return;

		canvasEl.width = videoEl.videoWidth;
		canvasEl.height = videoEl.videoHeight;
		ctx.drawImage(videoEl, 0, 0);

		const imageData = ctx.getImageData(0, 0, canvasEl.width, canvasEl.height);
		capturedPreview = canvasEl.toDataURL("image/jpeg", 0.9);

		oncapture({ imageData, bodyLocation, deviceModel });
	}

	function retake() {
		capturedPreview = null;
	}

	onMount(() => {
		startCamera();
	});

	onDestroy(() => {
		if (stream) {
			stream.getTracks().forEach((t) => t.stop());
		}
	});
</script>

<div class="flex flex-col gap-4 w-full max-w-lg mx-auto">
	<!-- Camera preview -->
	<div
		class="relative aspect-[4/3] w-full overflow-hidden rounded-xl bg-gray-900 dark:bg-gray-950"
	>
		{#if cameraError}
			<div
				class="absolute inset-0 flex flex-col items-center justify-center gap-3 p-4 text-center"
			>
				<p class="text-sm text-red-400">{cameraError}</p>
				<button
					onclick={startCamera}
					class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 active:bg-blue-800"
				>
					Retry
				</button>
			</div>
		{:else if capturedPreview}
			<img
				src={capturedPreview}
				alt="Captured lesion"
				class="h-full w-full object-contain"
			/>
			<button
				onclick={retake}
				class="absolute bottom-3 right-3 rounded-lg bg-gray-800/80 px-3 py-2 text-sm font-medium text-white backdrop-blur-sm hover:bg-gray-700/80"
			>
				Retake
			</button>
		{:else}
			<!-- svelte-ignore element_invalid_self_closing_tag -->
			<video
				bind:this={videoEl}
				autoplay
				playsinline
				muted
				class="h-full w-full object-cover"
			/>
			{#if !cameraReady}
				<div class="absolute inset-0 flex items-center justify-center">
					<div
						class="h-8 w-8 animate-spin rounded-full border-2 border-white border-t-transparent"
					></div>
				</div>
			{/if}
		{/if}
	</div>

	<!-- Capture button -->
	{#if !capturedPreview && !cameraError}
		<button
			onclick={captureFrame}
			disabled={!cameraReady}
			class="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-white shadow-lg transition-transform active:scale-95 disabled:opacity-40 dark:bg-gray-200"
			aria-label="Capture image"
		>
			<CarbonCamera class="h-7 w-7 text-gray-900" />
		</button>
	{/if}

	<canvas bind:this={canvasEl} class="hidden"></canvas>

	<!-- Selectors -->
	<div class="grid grid-cols-2 gap-3">
		<label class="flex flex-col gap-1">
			<span class="text-xs font-medium text-gray-500 dark:text-gray-400">Body Location</span>
			<select
				bind:value={bodyLocation}
				class="h-11 rounded-lg border border-gray-300 bg-white px-3 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
			>
				{#each BODY_LOCATIONS as loc}
					<option value={loc.value}>{loc.label}</option>
				{/each}
			</select>
		</label>

		<label class="flex flex-col gap-1">
			<span class="text-xs font-medium text-gray-500 dark:text-gray-400">Device</span>
			<select
				bind:value={deviceModel}
				onchange={() => startCamera()}
				class="h-11 rounded-lg border border-gray-300 bg-white px-3 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
			>
				{#each DEVICE_MODELS as dev}
					<option value={dev.value}>{dev.label}</option>
				{/each}
			</select>
		</label>
	</div>
</div>
