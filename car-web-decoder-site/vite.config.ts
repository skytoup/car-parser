import path from "node:path";
import { fileURLToPath } from "node:url";

import react from "@vitejs/plugin-react-swc";
import { defineConfig, type PluginOption, type ViteDevServer } from "vite";
import wasm from "vite-plugin-wasm";

const projectRoot = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = path.resolve(projectRoot, "..");
const carWasmRoot = path.resolve(repoRoot, "car-wasm");

function legacyPwaDevShim(): PluginOption {
  return {
    name: "legacy-pwa-dev-shim",
    configureServer(server: ViteDevServer) {
      server.middlewares.use(
        "/@vite-plugin-pwa/pwa-entry-point-loaded",
        (_req, res) => {
          res.setHeader("Content-Type", "application/javascript");
          res.end("export {};");
        },
      );
    },
  };
}

export default defineConfig({
  plugins: [react(), wasm(), legacyPwaDevShim()],
  build: {
    target: "esnext",
  },
  esbuild: {
    target: "esnext",
  },
  worker: {
    format: "es",
    plugins: () => [wasm()],
  },
  resolve: {
    alias: {
      "@": path.resolve(projectRoot, "src"),
      "@car-wasm/client": path.resolve(carWasmRoot, "js/client.js"),
    },
  },
  server: {
    fs: {
      allow: [repoRoot, carWasmRoot],
    },
  },
  optimizeDeps: {
    exclude: ["@car-wasm/client"],
  },
});
