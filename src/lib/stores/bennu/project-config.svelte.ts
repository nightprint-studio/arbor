/**
 * Bennu per-project configuration — the SEAM for user overrides of the resolved
 * project facts (JDK level, source encoding, source/output roots, excluded
 * directories). Shaped field-for-field to map onto a future `[bennu]` section in
 * the per-repo `<repo>/.arbor/config.toml`.
 *
 * MOCK — persist to per-project `<repo>/.arbor/config.toml` later. For now this
 * is an in-memory rune store keyed by project root, so overrides survive tab
 * switches within a session but are NOT written to disk. When the BE lands, swap
 * the in-memory map for `get_bennu_project_config` / `set_bennu_project_config`
 * IPC (mirrors the app-config pattern in CLAUDE.md rule #11) — consumers don't
 * change because they already read/write through this store's getters/methods.
 *
 * Rune store — the single approved shape: private `$state`, a returned object of
 * getters + methods (CLAUDE.md · "Store pattern").
 */

import { SvelteMap } from 'svelte/reactivity';

/** Sentinel value for "no override — use the resolved / auto value". */
export const AUTO = '__auto__';

/**
 * The per-project override document. Every field is an override on top of the
 * BE-resolved `ProjectInfo`; `AUTO` (or empty) means "defer to the resolved
 * value". This shape maps 1:1 to the planned `[bennu]` TOML table.
 */
export interface BennuProjectConfig {
  /** JDK language-level override, or `AUTO` to use the pom-resolved level. */
  jdkOverride: string;
  /** Source-encoding override, or `AUTO` to use the pom-resolved encoding. */
  encodingOverride: string;
  /** Source root (relative to project root). MOCK default `src/main/java`. */
  sourceRoot: string;
  /** Compiler output root (relative to project root). MOCK default `target/classes`. */
  outputRoot: string;
  /** Comma-separated directory names excluded from indexing/search. */
  excludedDirs: string;
}

/** The blank document a project starts from (all overrides deferred to resolved). */
export function defaultConfig(): BennuProjectConfig {
  return {
    jdkOverride:      AUTO,
    encodingOverride: AUTO,
    // MOCK — real roots will come from the BE (pom `build/sourceDirectory`,
    // `build/outputDirectory`). These are the Maven conventional defaults.
    sourceRoot:       'src/main/java',
    outputRoot:       'target/classes',
    excludedDirs:     'target, .git, .idea, node_modules',
  };
}

function createProjectConfigStore() {
  // MOCK — in-memory, keyed by project root. Replace with per-repo persistence.
  const byRoot = new SvelteMap<string, BennuProjectConfig>();

  /** The config for a project root, creating a default document on first read. */
  function ensure(root: string): BennuProjectConfig {
    let cfg = byRoot.get(root);
    if (!cfg) {
      cfg = defaultConfig();
      byRoot.set(root, cfg);
    }
    return cfg;
  }

  return {
    /** Read the config for `root` (materialises defaults on first access). */
    get(root: string): BennuProjectConfig {
      return ensure(root);
    },

    /**
     * Apply a full config document for `root`.
     * MOCK — persist to `<repo>/.arbor/config.toml` later (this only mutates the
     * in-memory map). Returns the stored copy.
     */
    apply(root: string, cfg: BennuProjectConfig): BennuProjectConfig {
      const stored = { ...cfg };
      byRoot.set(root, stored);
      return stored;
    },

    /** Reset a project's overrides back to the resolved/auto defaults. */
    reset(root: string): BennuProjectConfig {
      const cfg = defaultConfig();
      byRoot.set(root, cfg);
      return cfg;
    },
  };
}

export const bennuProjectConfigStore = createProjectConfigStore();
