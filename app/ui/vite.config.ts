import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [svelte()],
  // keep rust compiler errors visible in the same terminal
  clearScreen: false,
  server: {
    // the desktop shell expects the ui on exactly this port
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
  },
  build: {
    outDir: "build",
    target: "es2022",
  },
});
