import { describe, it, expect } from 'vitest';
import { run } from '../plugin-loader';

describe('plugin-loader', () => {
  // ── 16. No keyGenerator plugin → error ──────────────────────────────
  it('throws when no keyGenerator plugin is provided', async () => {
    await expect(
      // Empty plugins → no keyGenerator found
      run.__test_run_with_config?.({
        core: { actualDir: '/tmp', workingDir: '/tmp' },
        plugins: {},
      }) ??
        // Fallback: call run with a temp config file that has no plugins
        (async () => {
          const fs = await import('node:fs');
          const os = await import('node:os');
          const path = await import('node:path');
          const tmpDir = fs.mkdtempSync(
            path.join(os.tmpdir(), 'kaiki-pl-test-'),
          );
          const configPath = path.join(tmpDir, 'regconfig.json');
          fs.writeFileSync(
            configPath,
            JSON.stringify({
              core: { actualDir: '/tmp', workingDir: '/tmp' },
              plugins: {},
            }),
          );
          try {
            return await run(configPath);
          } finally {
            fs.rmSync(tmpDir, { recursive: true, force: true });
          }
        })(),
    ).rejects.toThrow('No keyGenerator plugin found');
  });
});
