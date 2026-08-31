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
    // Nothing under src-tauri is vite's to watch, and target/ is where cargo
    // works: three gigabytes of it here, most of them rewritten on every
    // build. Left in, the watcher walks all of it at startup and sooner or
    // later reaches an object file or hs_tracker.exe while the linker still
    // holds it open — and a watch on a locked file is not a warning, it is an
    // EBUSY that takes the whole vite process down. `tauri dev` runs vite as
    // its beforeDevCommand, so that reads as the build failing, with the real
    // cause thirty lines above the error it prints.
    //
    // It never fires on a fresh clone: target/ does not exist yet when vite
    // first looks, so there is nothing to walk. It starts once the tree has
    // been built, which is to say from the second `npm start` onwards.
    watch: { ignored: ['**/src-tauri/**', '**/dist/**'] },
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
