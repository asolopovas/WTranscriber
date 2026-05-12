import { defineConfig } from "@playwright/test";
import { DEV_HOST, DEV_URL } from "./dev.config";

export default defineConfig({
  testDir: "e2e",
  timeout: 30_000,
  use: {
    baseURL: DEV_URL,
    trace: "on-first-retry",
  },
  webServer: {
    command: `bun run dev --host ${DEV_HOST}`,
    url: DEV_URL,
    reuseExistingServer: true,
    timeout: 30_000,
  },
});
