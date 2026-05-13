import process from "node:process";
import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";
import { DEV_PORT, HMR_PORT } from "./dev.config";

const r = (p: string) => fileURLToPath(new URL(p, import.meta.url));

const host = process.env.TAURI_DEV_HOST;
const androidDev = process.env.TAURI_ENV_PLATFORM === "android" || Boolean(host);

export default defineConfig(() => ({
  plugins: [tailwindcss(), vue()],

  build: {
    rolldownOptions: {
      checks: {
        pluginTimings: false,
      },
    },
  },

  resolve: {
    alias: {
      "@": r("./src"),
      "@components": r("./src/components"),
      "@composables": r("./src/composables"),
      "@utils": r("./src/utils"),
      "@styles": r("./src/styles"),
    },
  },

  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: HMR_PORT,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
      usePolling: androidDev,
      interval: androidDev ? 250 : undefined,
    },
  },
}));
