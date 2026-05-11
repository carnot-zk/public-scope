import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    include: ["tests/**/*.test.ts", "tests/carnot-engine-audit.ts"],
    exclude: ["dist/**", "node_modules/**"],
    testTimeout: 60000,
    maxConcurrency: 5,
  },
});
