import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  // The wasm package is a local file: dependency; don't pre-bundle it.
  optimizeDeps: { exclude: ['maptool-wasm'] },
  server: {
    fs: {
      // The dev server only serves files under web/ by default and answers 403 for
      // the wasm build output in ../crates. Setting `allow` replaces that default,
      // so web/ has to be listed again.
      allow: ['.', fileURLToPath(new URL('../crates/maptool-wasm/pkg', import.meta.url))],
    },
  },
});
