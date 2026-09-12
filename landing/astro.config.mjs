// @ts-check
import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";

// Static output on purpose: the page is content that changes when someone edits
// it, not per request. It compiles to files nginx serves without a Node process
// on the box, the same shape as theremoteledger.org.
export default defineConfig({
  site: "https://principia.ndelucien.com",
  output: "static",
  // `file`, not `directory`: directory output makes /privacy a folder, nginx
  // 301s to /privacy/, and that redirect is built from the container's listen
  // port. Emitting privacy.html removes the redirect.
  build: { inlineStylesheets: "always", format: "file" },
  trailingSlash: "never",
  devToolbar: { enabled: false },
  integrations: [sitemap({ lastmod: new Date() })],
  // Pin the tsconfig. Vite 8 resolves one per transformed file by walking up
  // the tree, and this folder lives inside the desk's repository whose own
  // tsconfig extends a generated .svelte-kit file that a fresh checkout does
  // not have; without the pin the landing build failed in CI on that file.
  vite: { tsconfig: "./tsconfig.json" },
});
