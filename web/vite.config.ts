import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  // The wasm package is a local file: dependency; don't pre-bundle it.
  optimizeDeps: { exclude: ['maptool-wasm'] },
});
