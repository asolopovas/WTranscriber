import process from "node:process";
import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

const r = (p: string) => fileURLToPath(new URL(p, import.meta.url));

const host = process.env.TAURI_DEV_HOST;

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
      ignored: ["**/src-tauri/**"],
    },
  },
}));
