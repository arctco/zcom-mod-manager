import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import pkg from "./package.json";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  // The sidebar reads its version from package.json so a release bump
  // only has to happen in one place.
  define: { __APP_VERSION__: JSON.stringify(pkg.version) },
  // Assets stay as files rather than inlined data: URIs so the strict
  // `img-src 'self'` content security policy keeps working.
  build: { target: ["es2021", "chrome100", "safari13"], assetsInlineLimit: 0 }
});
