import { defineConfig } from "vite";

export default defineConfig({
  root: "frontend",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
});
