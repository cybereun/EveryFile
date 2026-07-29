import { configDefaults, defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["@testing-library/jest-dom/vitest"],
    exclude: [
      ...configDefaults.exclude,
      "sidecar/parser-host/**",
      "vendor/kordoc/**",
    ],
  },
});
