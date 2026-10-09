import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/*.tsbuildinfo"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  // Desktop CSP permits self-hosted fonts, so small WOFF2 assets must stay
  // as files instead of becoming blocked data: URLs in production CSS.
  build: { target: "es2021", assetsInlineLimit: 0 },
});
