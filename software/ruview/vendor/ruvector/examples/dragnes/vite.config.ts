import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import Icons from 'unplugin-icons/vite';

export default defineConfig({
	plugins: [
		sveltekit(),
		Icons({ compiler: 'svelte' }),
	],
	build: {
		rollupOptions: {
			external: ['@ruvector/cnn']
		}
	},
	ssr: {
		external: ['@ruvector/cnn']
	},
	test: {
		include: ['tests/**/*.test.ts', 'src/**/*.test.ts']
	}
});
