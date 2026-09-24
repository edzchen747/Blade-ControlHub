import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The built bundle is embedded into blade-controlhub.exe by Tauri, so it is
// served from the `tauri://localhost` custom protocol: relative asset paths and
// no code splitting keep it loadable without a real web server.
export default defineConfig({
  plugins: [svelte()],
  base: "./",
  clearScreen: false,
  build: {
    target: "chrome110",
    outDir: "dist",
    emptyOutDir: true,
    assetsInlineLimit: 4096,
    rollupOptions: { output: { inlineDynamicImports: true } },
  },
  server: { port: 5173, strictPort: true },
});
