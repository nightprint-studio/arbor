import type { WorktreeInfo, IdeConfig, DetectedIde } from '$lib/types/git';
import { listWorktrees, getIdeConfig } from '$lib/ipc/worktree';
import { listen } from '@tauri-apps/api/event';

function createWorktreeStore() {
  let worktrees      = $state<WorktreeInfo[]>([]);
  let ideConfig      = $state<IdeConfig | null>(null);
  let detectedIdes   = $state<DetectedIde[]>([]);
  let detectionDone  = $state(false);   // true once the startup probe has completed
  let loading        = $state(false);
  let error          = $state<string | null>(null);

  async function load(tabId: string) {
    loading = true;
    error   = null;
    try {
      worktrees = await listWorktrees(tabId);
    } catch (e) {
      error     = `${e}`;
      worktrees = [];
    } finally {
      loading = false;
    }
  }

  /**
   * Load IDE config from disk.
   * Detection is NOT triggered here — it runs once at app startup via
   * `startIdeDetection()` (AppShell.onMount) and results arrive through
   * the `arbor://ide-detection-done` event handled by `setupDetectionListener`.
   */
  async function loadIdeConfig() {
    try {
      ideConfig = await getIdeConfig();
    } catch { /* use defaults */ }
  }

  /**
   * Subscribe to the `arbor://ide-detection-done` event emitted by the
   * background detection job.  Call once from AppShell.onMount.
   * Returns the Tauri unlisten function.
   */
  async function setupDetectionListener() {
    return listen<DetectedIde[]>('arbor://ide-detection-done', (event) => {
      detectedIdes  = event.payload;
      detectionDone = true;
    });
  }

  function clear() {
    worktrees = [];
    error     = null;
  }

  /** Resolve which IDE id to use for a given project type. */
  function resolveIdeId(projectType: string): string | undefined {
    if (!ideConfig) return undefined;
    return ideConfig.language_defaults[projectType] ?? ideConfig.default_ide;
  }

  return {
    get worktrees()     { return worktrees; },
    get ideConfig()     { return ideConfig; },
    get detectedIdes()  { return detectedIdes; },
    get detectionDone() { return detectionDone; },
    get loading()       { return loading; },
    get error()         { return error; },
    load,
    loadIdeConfig,
    setupDetectionListener,
    clear,
    setIdeConfig(cfg: IdeConfig)       { ideConfig    = cfg; },
    setDetectedIdes(d: DetectedIde[])  { detectedIdes = d; detectionDone = true; },
    resolveIdeId,
  };
}

export const worktreeStore = createWorktreeStore();
