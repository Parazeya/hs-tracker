import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  // `tauri dev` restarts vite on every change, and vite clears the terminal as
  // it starts — taking the cargo errors that caused the restart with it.
  clearScreen: false,
  plugins: [svelte()],
  server: { port: 5176, strictPort: true },
  build: { outDir: 'dist' },
});
