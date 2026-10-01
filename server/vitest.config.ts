import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Test files share prisma/test.db and wipe it between tests, so they must
    // not run concurrently.
    fileParallelism: false,
  },
});
