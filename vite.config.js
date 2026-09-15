import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [sveltekit()],

  // Lucide ships Svelte source. Prebundling it makes Vite discover a second
  // Svelte graph after the Tauri webview has already loaded the first one;
  // the optimizer then removes the old chunks while WebKit still references
  // them. Serve the package through the Svelte plugin so one runtime owns the
  // component DOM from the first render onward.
  optimizeDeps: {
    exclude: ["lucide-svelte"],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`, and the landing site,
      //    whose build writes dozens of HTML files at once: each one used to
      //    trigger a full reload of the desk.
      ignored: ["**/src-tauri/**", "**/landing/**", "**/docs/**"],
    },
  },
}));
