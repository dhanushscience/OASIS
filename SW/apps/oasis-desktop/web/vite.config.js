import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Builds into ../ui: app.html (the main window), welcome.html (the first-run window), assets/*, fonts/*.
// overlay.html in ../ui is written by hand and is never touched.
export default defineConfig({
  plugins: [svelte()],
  base: './',
  build: {
    outDir: '../ui',
    emptyOutDir: false,
    rollupOptions: { input: { app: 'app.html', welcome: 'welcome.html' } },
  },
  server: { port: 5173, strictPort: true },
});
