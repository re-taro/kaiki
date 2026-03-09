import * as fs from 'node:fs';
import * as path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  BLUE_PNG,
  createTempDir,
  GREEN_PNG,
  loadNativeRun,
  mockKeyGenerator,
  mockNotifier,
  mockPublisher,
  RED_PNG,
  setupFixture,
} from './helpers';

const run = loadNativeRun();

let tmpDir: string;
let cleanup: () => void;

beforeEach(() => {
  ({ dir: tmpDir, cleanup } = createTempDir());
});

afterEach(() => {
  cleanup();
});

/**
 * Helper: wrap run() so that synchronous throws (e.g. config validation)
 * are converted to rejected promises for use with `rejects.toThrow`.
 */
async function runAsync(
  opts: Parameters<typeof run>[0],
): ReturnType<typeof run> {
  return run(opts);
}

describe('run() pipeline', () => {
  // ── 1. Full pipeline happy path ─────────────────────────────────────
  it('full pipeline with publisher + notifier: pass/fail mix', async () => {
    const expectedImages = {
      'pass.png': RED_PNG(),
      'fail.png': RED_PNG(),
    };
    const { config } = setupFixture(
      tmpDir,
      {
        'pass.png': RED_PNG(), // same as expected → pass
        'fail.png': BLUE_PNG(), // different from expected → fail
        'new.png': GREEN_PNG(), // no expected → new
      },
      expectedImages,
    );

    const kg = mockKeyGenerator('expected-abc', 'actual-xyz');
    const pub = mockPublisher(expectedImages);
    const notif = mockNotifier();

    const result = await run({
      config,
      keyGenerator: kg.mock,
      publisher: pub.mock,
      notifiers: [notif.mock],
    });

    // Comparison shape
    expect(result.comparison.passedItems).toContain('pass.png');
    expect(result.comparison.failedItems).toContain('fail.png');
    expect(result.comparison.newItems).toContain('new.png');
    expect(result.reportUrl).toBe('https://example.com/report');
    expect(result.hasFailures).toBe(true);

    // Publisher was called
    expect(pub.fetchCalls).toHaveLength(1);
    expect(pub.fetchCalls[0].key).toBe('expected-abc');
    expect(pub.publishCalls).toHaveLength(1);
    expect(pub.publishCalls[0].key).toBe('actual-xyz');

    // Notifier was called with correct params
    expect(notif.calls).toHaveLength(1);
    expect(notif.calls[0].failedItems).toContain('fail.png');
    expect(notif.calls[0].newItems).toContain('new.png');
    expect(notif.calls[0].reportUrl).toBe('https://example.com/report');
    expect(notif.calls[0].currentSha).toBe('actual-xyz');
  });

  // ── 2. Minimal config (no publisher, no notifiers) ──────────────────
  it('minimal config: no publisher/notifiers, null expectedKey → all new', async () => {
    const { config } = setupFixture(tmpDir, {
      'a.png': RED_PNG(),
      'b.png': BLUE_PNG(),
    });

    const kg = mockKeyGenerator(null, 'sha-123');

    const result = await run({
      config,
      keyGenerator: kg.mock,
    });

    expect(result.comparison.newItems).toHaveLength(2);
    expect(result.comparison.failedItems).toHaveLength(0);
    expect(result.comparison.passedItems).toHaveLength(0);
    expect(result.comparison.deletedItems).toHaveLength(0);
    expect(result.reportUrl).toBeUndefined();
    expect(result.hasFailures).toBe(false);
  });

  // ── 3. Image diff failures + diff file generation ───────────────────
  it('diff failures produce diff image files', async () => {
    const expectedImages = { 'img.png': RED_PNG() };
    const { config, workingDir } = setupFixture(
      tmpDir,
      { 'img.png': BLUE_PNG() },
      expectedImages,
    );

    const kg = mockKeyGenerator('key-1', 'key-2');
    const pub = mockPublisher(expectedImages);

    const result = await run({
      config,
      keyGenerator: kg.mock,
      publisher: pub.mock,
    });

    expect(result.comparison.failedItems).toContain('img.png');
    expect(result.comparison.diffItems).toContain('img.png');

    // Diff image file should exist
    const diffPath = path.join(workingDir, 'diff', 'img.png');
    expect(fs.existsSync(diffPath)).toBe(true);
  });

  // ── 4. KeyGenerator reject → Promise rejection ──────────────────────
  it('getExpectedKey rejection propagates', async () => {
    const { config } = setupFixture(tmpDir, { 'a.png': RED_PNG() });

    await expect(
      run({
        config,
        keyGenerator: {
          getExpectedKey: async () => {
            throw new Error('keygen boom');
          },
          getActualKey: async () => 'sha',
        },
      }),
    ).rejects.toThrow('keygen boom');
  });

  // ── 5. Publisher.fetch failure → rejection ──────────────────────────
  it('publisher.fetch failure propagates', async () => {
    const { config } = setupFixture(tmpDir, { 'a.png': RED_PNG() });

    await expect(
      run({
        config,
        keyGenerator: mockKeyGenerator('key', 'sha').mock,
        publisher: {
          // napi TSFN error-first callback: (err, args)
          fetch: async () => {
            throw new Error('fetch boom');
          },
          publish: async () => ({ reportUrl: undefined }),
        },
      }),
    ).rejects.toThrow('fetch boom');
  });

  // ── 6. Notifier failure does NOT break pipeline ─────────────────────
  it('notifier failure is absorbed, pipeline succeeds', async () => {
    const { config } = setupFixture(tmpDir, { 'a.png': RED_PNG() });

    const kg = mockKeyGenerator(null, 'sha');

    const result = await run({
      config,
      keyGenerator: kg.mock,
      notifiers: [
        {
          // napi TSFN error-first callback: (err, params)
          notify: async () => {
            throw new Error('notify boom');
          },
        },
      ],
    });

    // Pipeline completes despite notifier error
    expect(result.comparison).toBeDefined();
    expect(result.comparison.newItems).toContain('a.png');
  });

  // ── 7. NotifyParams structure validation ────────────────────────────
  it('notifier receives correct NotifyParams structure', async () => {
    const expectedImages = { 'pass.png': RED_PNG() };
    const { config } = setupFixture(
      tmpDir,
      {
        'pass.png': RED_PNG(),
        'new.png': GREEN_PNG(),
      },
      expectedImages,
    );

    const kg = mockKeyGenerator('exp-key', 'act-key');
    const pub = mockPublisher(expectedImages, 'https://report.test');
    const notif = mockNotifier();

    await run({
      config,
      keyGenerator: kg.mock,
      publisher: pub.mock,
      notifiers: [notif.mock],
    });

    expect(notif.calls).toHaveLength(1);
    const params = notif.calls[0];

    expect(Array.isArray(params.failedItems)).toBe(true);
    expect(Array.isArray(params.newItems)).toBe(true);
    expect(Array.isArray(params.deletedItems)).toBe(true);
    expect(Array.isArray(params.passedItems)).toBe(true);
    expect(typeof params.currentSha).toBe('string');
    expect(params.reportUrl).toBe('https://report.test');
    // prNumber comes from CI detection; may be a number in CI (GITHUB_REF) or null/undefined locally
    expect(params.prNumber == null || typeof params.prNumber === 'number').toBe(true);
  });

  // ── 8. Config validation error (missing core) ──────────────────────
  it('config without core field → invalid config error', async () => {
    const kg = mockKeyGenerator(null, 'sha');

    // run() throws synchronously for config validation; wrap in async for rejects
    await expect(
      runAsync({
        config: { plugins: {} } as any,
        keyGenerator: kg.mock,
      }),
    ).rejects.toThrow('invalid config');
  });

  // ── 9. getExpectedKey returns "" → treated as None ──────────────────
  it('getExpectedKey "" → treated as no expected key, fetch skipped', async () => {
    const { config } = setupFixture(tmpDir, { 'a.png': RED_PNG() });

    const pub = mockPublisher();
    const kg = mockKeyGenerator('', 'sha');

    const result = await run({
      config,
      keyGenerator: kg.mock,
      publisher: pub.mock,
    });

    // Empty string treated as None → no fetch call
    expect(pub.fetchCalls).toHaveLength(0);
    expect(result.comparison.newItems).toContain('a.png');
  });

  // ── 10. getExpectedKey returns undefined → Option<String> None ──────
  it('getExpectedKey undefined → no expected key', async () => {
    const { config } = setupFixture(tmpDir, { 'a.png': RED_PNG() });

    const pub = mockPublisher();

    const result = await run({
      config,
      keyGenerator: {
        getExpectedKey: async () => undefined as any,
        getActualKey: async () => 'sha',
      },
      publisher: pub.mock,
    });

    expect(pub.fetchCalls).toHaveLength(0);
    expect(result.comparison.newItems).toContain('a.png');
  });

  // ── 11. publisher.publish failure → rejection ───────────────────────
  it('publisher.publish failure propagates', async () => {
    const { config } = setupFixture(tmpDir, { 'a.png': RED_PNG() });

    await expect(
      run({
        config,
        keyGenerator: mockKeyGenerator(null, 'sha').mock,
        publisher: {
          fetch: async () => {},
          publish: async () => {
            throw new Error('publish boom');
          },
        },
      }),
    ).rejects.toThrow('publish boom');
  });

  // ── 12. Multiple notifiers all receive same params ──────────────────
  it('multiple notifiers (3) all receive identical params', async () => {
    const { config } = setupFixture(tmpDir, { 'a.png': RED_PNG() });

    const kg = mockKeyGenerator(null, 'sha-multi');
    const n1 = mockNotifier();
    const n2 = mockNotifier();
    const n3 = mockNotifier();

    await run({
      config,
      keyGenerator: kg.mock,
      notifiers: [n1.mock, n2.mock, n3.mock],
    });

    expect(n1.calls).toHaveLength(1);
    expect(n2.calls).toHaveLength(1);
    expect(n3.calls).toHaveLength(1);

    // All three receive the same params
    expect(n1.calls[0].currentSha).toBe('sha-multi');
    expect(n2.calls[0].currentSha).toBe('sha-multi');
    expect(n3.calls[0].currentSha).toBe('sha-multi');
    expect(n1.calls[0].newItems).toEqual(n2.calls[0].newItems);
    expect(n2.calls[0].newItems).toEqual(n3.calls[0].newItems);
  });

  // ── 13. Zero actual images → all fields empty ──────────────────────
  it('zero actual images → all result arrays empty', async () => {
    const { config } = setupFixture(tmpDir, {});

    const kg = mockKeyGenerator(null, 'sha');

    const result = await run({
      config,
      keyGenerator: kg.mock,
    });

    expect(result.comparison.failedItems).toHaveLength(0);
    expect(result.comparison.newItems).toHaveLength(0);
    expect(result.comparison.deletedItems).toHaveLength(0);
    expect(result.comparison.passedItems).toHaveLength(0);
    expect(result.comparison.actualItems).toHaveLength(0);
    expect(result.hasFailures).toBe(false);
  });

  // ── 14. reportUrl undefined from publisher → undefined in result ────
  it('reportUrl undefined from publisher → undefined in result', async () => {
    const { config } = setupFixture(tmpDir, { 'a.png': RED_PNG() });

    const kg = mockKeyGenerator(null, 'sha');
    const pub = mockPublisher({}, null);

    const result = await run({
      config,
      keyGenerator: kg.mock,
      publisher: pub.mock,
    });

    // napi Option<String> None maps to undefined; loose equality covers both
    expect(result.reportUrl == null).toBe(true);
  });

  // ── 15. Completely empty config {} → validation error ───────────────
  it('completely empty config {} → invalid config error', async () => {
    const kg = mockKeyGenerator(null, 'sha');

    // run() throws synchronously for config validation; wrap in async for rejects
    await expect(
      runAsync({
        config: {} as any,
        keyGenerator: kg.mock,
      }),
    ).rejects.toThrow('invalid config');
  });
});
