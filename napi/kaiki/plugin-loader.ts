import * as fs from "node:fs";
import * as path from "node:path";
import { run as nativeRun } from "./index.js";

/**
 * A reg-suit plugin factory function. When called, it returns a holder object
 * that may contain keyGenerator, publisher, and/or notifier instances.
 */
type PluginFactory = () => PluginHolder;

interface PluginHolder {
  keyGenerator?: {
    init(config: Record<string, unknown>): void;
    getExpectedKey(): Promise<string | null>;
    getActualKey(): Promise<string>;
  };
  publisher?: {
    init(config: Record<string, unknown>): void;
    fetch(args: { key: string; destDir: string }): Promise<void>;
    publish(args: {
      key: string;
      sourceDir: string;
    }): Promise<{ reportUrl?: string | null }>;
  };
  notifier?: {
    init(config: Record<string, unknown>): void;
    notify(params: {
      failedItems: string[];
      newItems: string[];
      deletedItems: string[];
      passedItems: string[];
      reportUrl?: string | null;
      currentSha: string;
      prNumber?: number | null;
    }): Promise<void>;
  };
}

interface RegConfig {
  core: Record<string, unknown>;
  plugins: Record<string, Record<string, unknown>>;
}

/**
 * Load and run kaiki with reg-suit plugin support.
 *
 * Reads the regconfig.json configuration file, loads any configured reg-suit
 * plugins via `require()`, initializes them, and then delegates to the native
 * Rust `run()` function with the extracted callbacks.
 *
 * @param configPath - Path to regconfig.json (default: 'regconfig.json')
 * @returns The pipeline result from the native runner
 */
export async function run(configPath = "regconfig.json") {
  const resolvedPath = path.resolve(configPath);
  const raw = fs.readFileSync(resolvedPath, "utf-8");
  const config: RegConfig = JSON.parse(raw);

  let keyGenerator:
    | {
        getExpectedKey: () => Promise<string | null>;
        getActualKey: () => Promise<string>;
      }
    | undefined;

  let publisher:
    | {
        fetch: (args: { key: string; destDir: string }) => Promise<void>;
        publish: (args: {
          key: string;
          sourceDir: string;
        }) => Promise<{ reportUrl?: string | null }>;
      }
    | undefined;

  const notifiers: Array<{
    notify: (params: {
      failedItems: string[];
      newItems: string[];
      deletedItems: string[];
      passedItems: string[];
      reportUrl?: string | null;
      currentSha: string;
      prNumber?: number | null;
    }) => Promise<void>;
  }> = [];

  // Load and initialize plugins
  for (const [name, options] of Object.entries(config.plugins ?? {})) {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const factory: PluginFactory = require(name);
    const holder = factory();

    if (holder.keyGenerator) {
      holder.keyGenerator.init(options);
      keyGenerator = {
        getExpectedKey: () => holder.keyGenerator!.getExpectedKey(),
        getActualKey: () => holder.keyGenerator!.getActualKey(),
      };
    }

    if (holder.publisher) {
      holder.publisher.init(options);
      publisher = {
        fetch: (args) => holder.publisher!.fetch(args),
        publish: (args) => holder.publisher!.publish(args),
      };
    }

    if (holder.notifier) {
      holder.notifier.init(options);
      notifiers.push({
        notify: (params) => holder.notifier!.notify(params),
      });
    }
  }

  if (!keyGenerator) {
    throw new Error(
      "No keyGenerator plugin found. At least one plugin must provide a keyGenerator.",
    );
  }

  return nativeRun({
    config,
    keyGenerator,
    publisher,
    notifiers: notifiers.length > 0 ? notifiers : undefined,
  });
}
