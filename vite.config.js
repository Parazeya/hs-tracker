import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  // `tauri dev` restarts vite on every change, and vite clears the terminal as
  // it starts — taking the cargo errors that caused the restart with it.
  clearScreen: false,
  plugins: [svelte()],
  server: {
    port: 5176,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    outDir: 'dist',
    // The datamined tables are not code: they are a quarter of a megabyte of
    // generated literals that no amount of editing here will shrink, and with
    // them in the same chunk the app's own code disappears into the number
    // rollup prints. Kept apart, each chunk stands for what it is — and the
    // webview reads both off the disk beside the executable, so the split
    // costs one more file open and nothing else.
    rollupOptions: { output: { manualChunks: (id) => (id.endsWith('src/items.js') ? 'items' : undefined) } },
  },
});
