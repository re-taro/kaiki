import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['__tests__/**/*.test.ts'],
    benchmark: { include: ['__bench__/**/*.bench.ts'] },
    testTimeout: 30_000,
    hookTimeout: 10_000,
  },
});
