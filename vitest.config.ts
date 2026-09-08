import { configDefaults, defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
    maxWorkers: 2,
    environment: "jsdom",
    setupFiles: ["@testing-library/jest-dom/vitest"],
    exclude: [
      ...configDefaults.exclude,
      ".worktrees/**",
      "sidecar/parser-host/**",
      "vendor/kordoc/**",
    ],
  },
});
