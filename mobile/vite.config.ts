import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// `tauri android dev` / `tauri ios dev` on a real device set this to the
// laptop's LAN address so the phone can reach the dev server.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1431 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
