import { describe, bench, beforeAll, afterAll } from 'vitest';
import {
  makePng,
  RED_PNG,
  BLUE_PNG,
  createTempDir,
  setupFixture,
  mockKeyGenerator,
  mockPublisher,
  mockNotifier,
  loadNativeRun,
} from '../__tests__/helpers';

const run = loadNativeRun();

// ── Bench 1: TSFN overhead (minimal pipeline) ──────────────────────────

describe('TSFN overhead', () => {
  let tmpDir: string;
  let cleanup: () => void;

  beforeAll(() => {
    ({ dir: tmpDir, cleanup } = createTempDir());
  });

  afterAll(() => {
    cleanup();
  });

  bench(
    'minimal pipeline: expectedKey=null, 0 images',
    async () => {
      const { config } = setupFixture(tmpDir, {});
      const kg = mockKeyGenerator(null, 'sha');
      await run({ config, keyGenerator: kg.mock });
    },
    { iterations: 100 },
  );
});

// ── Bench 2: E2E pipeline (realistic small project) ─────────────────────

describe('E2E pipeline', () => {
  let tmpDir: string;
  let cleanup: () => void;

  beforeAll(() => {
    ({ dir: tmpDir, cleanup } = createTempDir());
  });

  afterAll(() => {
    cleanup();
  });

  bench(
    '5 images (3 pass + 2 fail) + publisher + notifier',
    async () => {
      const expected: Record<string, Buffer> = {
        'pass1.png': RED_PNG(),
        'pass2.png': RED_PNG(),
        'pass3.png': RED_PNG(),
        'fail1.png': RED_PNG(),
        'fail2.png': RED_PNG(),
      };
      const actual: Record<string, Buffer> = {
        'pass1.png': RED_PNG(),
        'pass2.png': RED_PNG(),
        'pass3.png': RED_PNG(),
        'fail1.png': BLUE_PNG(),
        'fail2.png': BLUE_PNG(),
      };

      const { config } = setupFixture(tmpDir, actual, expected);
      const kg = mockKeyGenerator('exp', 'act');
      const pub = mockPublisher(expected);
      const notif = mockNotifier();

      await run({
        config,
        keyGenerator: kg.mock,
        publisher: pub.mock,
        notifiers: [notif.mock],
      });
    },
    { iterations: 20 },
  );
});

// ── Bench 3: Scaling (10 / 50 / 100 images) ────────────────────────────

describe('Scaling', () => {
  function makeImageSet(count: number): {
    actual: Record<string, Buffer>;
    expected: Record<string, Buffer>;
  } {
    const actual: Record<string, Buffer> = {};
    const expected: Record<string, Buffer> = {};
    const red = makePng(255, 0, 0, 4, 4);
    const blue = makePng(0, 0, 255, 4, 4);

    for (let i = 0; i < count; i++) {
      const name = `img-${String(i).padStart(4, '0')}.png`;
      expected[name] = red;
      // Half pass, half fail
      actual[name] = i % 2 === 0 ? red : blue;
    }
    return { actual, expected };
  }

  for (const [count, iterations] of [
    [10, 20],
    [50, 10],
    [100, 5],
  ] as const) {
    describe(`${count} images`, () => {
      let tmpDir: string;
      let cleanup: () => void;

      beforeAll(() => {
        ({ dir: tmpDir, cleanup } = createTempDir());
      });

      afterAll(() => {
        cleanup();
      });

      bench(
        `${count} images (half pass / half fail, 4x4 PNG)`,
        async () => {
          const { actual, expected } = makeImageSet(count);
          const { config } = setupFixture(tmpDir, actual, expected);
          const kg = mockKeyGenerator('exp', 'act');
          const pub = mockPublisher(expected);

          await run({
            config,
            keyGenerator: kg.mock,
            publisher: pub.mock,
          });
        },
        { iterations },
      );
    });
  }
});
