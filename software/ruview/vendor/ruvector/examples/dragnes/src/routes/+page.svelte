<script lang="ts">
	import { onMount } from 'svelte';

	let DrAgnesPanel: any = $state(null);
	let loadError: string | null = $state(null);
	let loading: boolean = $state(true);

	onMount(async () => {
		try {
			const mod = await import('$lib/components/DrAgnesPanel.svelte');
			DrAgnesPanel = mod.default;
		} catch (err) {
			loadError = err instanceof Error ? err.message : 'Failed to load';
		} finally {
			loading = false;
		}
	});
</script>

<div class="flex h-dvh w-full flex-col">
	<header class="flex items-center gap-3 border-b border-gray-200 bg-white px-4 py-3 dark:border-gray-700 dark:bg-gray-900">
		<div>
			<h1 class="text-lg font-bold text-gray-900 dark:text-gray-100">DrAgnes</h1>
			<p class="text-xs text-gray-400">Dermatology Intelligence -- powered by RuVector</p>
		</div>
	</header>
	<main class="flex-1 overflow-hidden">
		{#if loading}
			<div class="flex h-full items-center justify-center">
				<div class="h-8 w-8 animate-spin rounded-full border-2 border-blue-500 border-t-transparent"></div>
			</div>
		{:else if loadError}
			<div class="flex h-full items-center justify-center p-6">
				<div class="rounded-xl border border-red-200 bg-red-50 p-6 text-center dark:border-red-800 dark:bg-red-950">
					<h2 class="mb-2 font-semibold text-red-700 dark:text-red-400">Failed to load</h2>
					<p class="text-sm text-red-600">{loadError}</p>
					<button onclick={() => window.location.reload()} class="mt-4 rounded-lg bg-red-600 px-4 py-2 text-sm text-white hover:bg-red-700">Retry</button>
				</div>
			</div>
		{:else if DrAgnesPanel}
			<DrAgnesPanel />
		{/if}
	</main>
</div>
