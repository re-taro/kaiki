import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { PNG } from 'pngjs';

// ── PNG generation ──────────────────────────────────────────────────────

/** Create a solid-color PNG buffer. */
export function makePng(
  r: number,
  g: number,
  b: number,
  width = 2,
  height = 2,
): Buffer {
  const png = new PNG({ width, height });
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const idx = (width * y + x) << 2;
      png.data[idx] = r;
      png.data[idx + 1] = g;
      png.data[idx + 2] = b;
      png.data[idx + 3] = 255;
    }
  }
  return PNG.sync.write(png);
}

export const RED_PNG = () => makePng(255, 0, 0);
export const BLUE_PNG = () => makePng(0, 0, 255);
export const GREEN_PNG = () => makePng(0, 255, 0);

// ── Temp directory ──────────────────────────────────────────────────────

export function createTempDir(): { dir: string; cleanup: () => void } {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kaiki-test-'));
  return {
    dir,
    cleanup: () => fs.rmSync(dir, { recursive: true, force: true }),
  };
}

// ── Fixture setup ───────────────────────────────────────────────────────

interface FixtureConfig {
  core: {
    actualDir: string;
    workingDir: string;
  };
  plugins: Record<string, never>;
}

/**
 * Build actual/working/expected directories and return a config object
 * suitable for passing to `run()`.
 *
 * @param tmpDir       Root temporary directory
 * @param actualImages Map of filename → PNG buffer for actual images
 * @param expectedImages Map of filename → PNG buffer for expected images (placed via publisher mock)
 */
export function setupFixture(
  tmpDir: string,
  actualImages: Record<string, Buffer>,
  expectedImages?: Record<string, Buffer>,
): {
  config: FixtureConfig;
  actualDir: string;
  workingDir: string;
  expectedDir: string;
  expectedImages: Record<string, Buffer>;
} {
  const actualDir = path.join(tmpDir, 'actual');
  const workingDir = path.join(tmpDir, 'working');
  const expectedDir = path.join(workingDir, 'expected');

  fs.mkdirSync(actualDir, { recursive: true });
  fs.mkdirSync(workingDir, { recursive: true });
  fs.mkdirSync(expectedDir, { recursive: true });

  for (const [name, buf] of Object.entries(actualImages)) {
    fs.writeFileSync(path.join(actualDir, name), buf);
  }

  return {
    config: {
      core: {
        actualDir,
        workingDir,
      },
      plugins: {},
    },
    actualDir,
    workingDir,
    expectedDir,
    expectedImages: expectedImages ?? {},
  };
}

// ── Mock factories ──────────────────────────────────────────────────────

export function mockKeyGenerator(
  expectedKey: string | null | undefined,
  actualKey: string,
) {
  let expectedKeyCalls = 0;
  let actualKeyCalls = 0;
  return {
    mock: {
      getExpectedKey: async () => {
        expectedKeyCalls++;
        return expectedKey;
      },
      getActualKey: async () => {
        actualKeyCalls++;
        return actualKey;
      },
    },
    get expectedKeyCalls() {
      return expectedKeyCalls;
    },
    get actualKeyCalls() {
      return actualKeyCalls;
    },
  };
}

/**
 * Create a publisher mock.
 *
 * `fetch` copies `expectedImages` into `destDir` (simulating storage download).
 * `publish` records the call and returns a reportUrl.
 */
export function mockPublisher(
  expectedImages?: Record<string, Buffer>,
  reportUrl?: string | null,
) {
  let fetchCalls: Array<{ key: string; destDir: string }> = [];
  let publishCalls: Array<{ key: string; sourceDir: string }> = [];

  // Default reportUrl to a non-null string
  const effectiveReportUrl = arguments.length < 2 ? 'https://example.com/report' : reportUrl;

  return {
    mock: {
      fetch: async (_err: unknown, args: { key: string; destDir: string }) => {
        fetchCalls.push(args);
        if (expectedImages) {
          fs.mkdirSync(args.destDir, { recursive: true });
          for (const [name, buf] of Object.entries(expectedImages)) {
            fs.writeFileSync(path.join(args.destDir, name), buf);
          }
        }
      },
      publish: async (_err: unknown, args: { key: string; sourceDir: string }) => {
        publishCalls.push(args);
        // napi Option<String> cannot deserialize JS null; use undefined for None
        return { reportUrl: effectiveReportUrl === null ? undefined : effectiveReportUrl };
      },
    },
    get fetchCalls() {
      return fetchCalls;
    },
    get publishCalls() {
      return publishCalls;
    },
    reset() {
      fetchCalls = [];
      publishCalls = [];
    },
  };
}

/** Create a notifier mock that records all notify calls. */
export function mockNotifier() {
  type NotifyCallParams = {
    failedItems: string[];
    newItems: string[];
    deletedItems: string[];
    passedItems: string[];
    reportUrl?: string | null;
    currentSha: string;
    prNumber?: number | null;
  };
  let calls: NotifyCallParams[] = [];
  return {
    mock: {
      notify: async (_err: unknown, params: NotifyCallParams) => {
        calls.push(params);
      },
    },
    get calls() {
      return calls;
    },
    reset() {
      calls = [];
    },
  };
}

// ── Native module loader ────────────────────────────────────────────────

/**
 * Load the native `run` function.
 * Throws a clear error if the native module is not built.
 */
export function loadNativeRun(): typeof import('../index.js').run {
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const mod = require('../index.js');
    return mod.run;
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    throw new Error(
      `Native module not built. Run "pnpm run build:debug" first.\n${msg}`,
    );
  }
}
