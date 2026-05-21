import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import topLevelAwait from "vite-plugin-top-level-await";
import wasm from "vite-plugin-wasm";

export default defineConfig({
  base: "",
  resolve: {
    alias: {
      "monaco-vim": fileURLToPath(
        new URL("./node_modules/monaco-vim/dist/index.mjs", import.meta.url),
      ),
    },
  },
  build: {
    chunkSizeWarningLimit: 1000,
  },
  optimizeDeps: {
    exclude: ["monaco-vim"],
  },
  plugins: [wasm(), topLevelAwait(), react()],
  server: {
    proxy: {
      "/api": {
        target: "http://127.0.0.1:3030",
        changeOrigin: true,
        secure: false,
        ws: true,
      },
    },
  },
});
