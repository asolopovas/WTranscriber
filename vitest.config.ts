import { fileURLToPath, URL } from "node:url";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

const r = (p: string) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      "@": r("./src"),
      "@components": r("./src/components"),
      "@composables": r("./src/composables"),
      "@utils": r("./src/utils"),
      "@styles": r("./src/styles"),
    },
  },
  test: {
    environment: "happy-dom",
    include: ["src/**/*.{test,spec}.ts"],
    globals: false,
    css: false,
  },
});
