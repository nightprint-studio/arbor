import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { DiffFile } from '../types/git';
import type { DiffConfig, DiffMode } from '../types/config';
import { getDiffConfig, setDiffConfig } from '$lib/ipc/config';
import { getCommitFileDiff } from '$lib/ipc/diff';

type StreamStartedPayload = {
  job_id: string;
  tab_id: string;
  staged: boolean;
  total_files: number;
  files: DiffFile[];
};

type StreamFilePayload = {
  job_id: string;
  tab_id: string;
  index: number;
  total: number;
  file: DiffFile;
};

type StreamDonePayload = { job_id: string; tab_id: string };
type StreamErrorPayload = { job_id: string; tab_id: string; error: string };

function createDiffStore() {
  let files = $state<DiffFile[]>([]);
  let selectedFile = $state<DiffFile | null>(null);
  let isLoading = $state(false);
  /// Total files expected for the current streaming load (0 when not streaming).
  let totalExpected = $state(0);
  /// Number of fully-parsed files received so far (for "parsed 12/34" spinner).
  let parsedCount = $state(0);
  /// The job_id of the in-flight streaming request.  Used to ignore stale events
  /// when the user triggers a new load before the previous one finishes.
  let activeJobId = $state<string | null>(null);
  let mode = $state<DiffMode>(
    (localStorage.getItem('arbor:diff-mode') as DiffMode | null) ?? 'split'
  );
  let wordWrap = $state(localStorage.getItem('arbor:word-wrap') === 'true');
  // fullFile and virtThreshold are persisted server-side in
  // ~/.config/arbor/config.toml under [diff] — call loadConfig() on app boot
  // to populate these. Defaults match DiffConfig::default in app_config.rs.
  let fullFile = $state(false);
  let virtThreshold = $state(200);
  let configLoaded = $state(false);
  // Cache the most recently loaded full DiffConfig so set_diff_config can
  // round-trip every field (algorithm/context/word_wrap) — we only mutate
  // the two we own, then echo the rest back to disk.
  let lastLoadedConfig: DiffConfig | null = null;

  async function loadConfig() {
    try {
      const cfg = await getDiffConfig();
      lastLoadedConfig = cfg;
      fullFile = !!cfg.full_file;
      virtThreshold = clampThreshold(cfg.virt_threshold);
      configLoaded = true;
    } catch {
      // First-run / backend not ready yet: keep defaults, retry on next call.
    }
  }

  function clampThreshold(n: number): number {
    if (!Number.isFinite(n)) return 200;
    return Math.max(50, Math.min(100000, Math.floor(n)));
  }

  /** Persist the current fullFile + virtThreshold to disk via IPC. */
  function persistConfig() {
    if (!lastLoadedConfig) {
      // No baseline yet — synthesize one with safe defaults.
      lastLoadedConfig = {
        algorithm: 'myers',
        context_lines: 3,
        word_wrap: false,
        full_file: fullFile,
        virt_threshold: virtThreshold,
      };
    }
    const next: DiffConfig = {
      ...lastLoadedConfig,
      full_file: fullFile,
      virt_threshold: virtThreshold,
    };
    lastLoadedConfig = next;
    void setDiffConfig(next).catch(() => {});
  }

  /// Track which file paths are still awaiting their hunk data (streaming
  /// workdir OR lazy commit-file load). Use a Set so Svelte's reactivity
  /// picks up reference changes on replace.
  let pendingPaths = $state<Set<string>>(new Set());

  /// When non-null, files coming through `setFiles()` are metadata-only and
  /// hunks must be fetched per-file via `getCommitFileDiff`. The dispatcher
  /// also uses this as a guard to discard stale fetch results when the user
  /// jumps to a different commit before a previous fetch resolves.
  let commitContext = $state<{ tabId: string; oid: string } | null>(null);
  /// Bumped on every commit context change. Per-file fetches capture the
  /// value at launch and re-check on completion — stale results are dropped.
  let commitSeq = 0;

  /// Requested selection path for the next streaming load.  Consumed once
  /// `beginStream` runs so the caller can request "load diff and select path X"
  /// even though the files list only arrives via an async event.
  let pendingSelection: string | null = null;

  function setFiles(f: DiffFile[]) {
    files = f;
    selectedFile = f[0] ?? null;
    // In commit-context mode `setFiles` is called with metadata-only entries;
    // pre-populate `pendingPaths` so file rows render the "parsing…" badge
    // and DiffViewer shows the skeleton until hunks arrive.
    if (commitContext) {
      pendingPaths = new Set(f.filter(file => file.hunks.length === 0 && !file.is_binary).map(file => file.path));
    } else {
      pendingPaths = new Set();
    }
    if (selectedFile) ensureFileLoaded(selectedFile.path);
  }

  function selectFile(path: string) {
    selectedFile = files.find(f => f.path === path) ?? null;
    if (selectedFile) ensureFileLoaded(selectedFile.path);
  }

  /// When a commit context is active and the selected file's hunks haven't
  /// been parsed yet, fetch them on demand. Idempotent: if the file already
  /// has hunks, or a fetch is already in flight, this is a no-op.
  function ensureFileLoaded(path: string) {
    const ctx = commitContext;
    if (!ctx) return;
    const idx = files.findIndex(f => f.path === path);
    if (idx === -1) return;
    const file = files[idx];
    if (file.is_binary) return;
    if (file.hunks.length > 0) return;
    // pendingPaths is the "fetch in flight" guard. setFiles populates it
    // initially; if already missing here it means a previous fetch failed —
    // allow a retry by re-adding before kicking off.
    const seenSeq = commitSeq;
    if (!pendingPaths.has(path)) {
      const np = new Set(pendingPaths);
      np.add(path);
      pendingPaths = np;
    }
    void getCommitFileDiff(ctx.tabId, ctx.oid, path)
      .then(parsed => applyCommitFileDetail(parsed, seenSeq))
      .catch(() => {
        if (seenSeq !== commitSeq) return;
        // Drop from pending so the spinner stops; UI will show "No changes".
        if (pendingPaths.has(path)) {
          const np = new Set(pendingPaths);
          np.delete(path);
          pendingPaths = np;
        }
      });
  }

  /// Apply a per-file parse result to the current list. Discards the result
  /// if the commit context changed between request and response (user jumped
  /// to a different commit) so stale hunks never overwrite the new file list.
  function applyCommitFileDetail(parsed: DiffFile, expectedSeq: number) {
    if (expectedSeq !== commitSeq) return;
    const idx = files.findIndex(f => f.path === parsed.path);
    if (idx === -1) return;
    const next = files.slice();
    next[idx] = parsed;
    files = next;
    if (pendingPaths.has(parsed.path)) {
      const np = new Set(pendingPaths);
      np.delete(parsed.path);
      pendingPaths = np;
    }
    if (selectedFile && selectedFile.path === parsed.path) {
      selectedFile = next[idx];
    }
  }

  /// Mark subsequent `setFiles()` calls as metadata-only — file hunks will
  /// be fetched per-path via `getCommitFileDiff`. Call before `setFiles()`.
  /// Bumps the sequence number so any pending fetches from the previous
  /// commit are discarded when they return.
  function setCommitContext(tabId: string, oid: string) {
    commitContext = { tabId, oid };
    commitSeq++;
  }

  /// Switch off commit-context mode (e.g. when switching to workdir/WIP view).
  /// Bumps the sequence so in-flight per-file fetches discard their results.
  function clearCommitContext() {
    commitContext = null;
    commitSeq++;
  }

  function setLoading(v: boolean) {
    isLoading = v;
  }

  function setMode(m: DiffMode) {
    mode = m;
    localStorage.setItem('arbor:diff-mode', m);
  }

  function setWordWrap(v: boolean) {
    wordWrap = v;
    localStorage.setItem('arbor:word-wrap', String(v));
  }

  function setFullFile(v: boolean) {
    if (fullFile === v) return;
    fullFile = v;
    persistConfig();
    // Notify all consumers (StageArea, CommitGraph commit-detail, BranchCompare,
    // MR diff loader, …) that the currently visible diff must be re-fetched
    // because the requested context has changed.
    window.dispatchEvent(new CustomEvent('arbor:reload-diff'));
  }

  function setVirtThreshold(n: number) {
    const clamped = clampThreshold(n);
    if (clamped === virtThreshold) return;
    virtThreshold = clamped;
    persistConfig();
  }

  function clear() {
    files = [];
    selectedFile = null;
    isLoading = false;
    totalExpected = 0;
    parsedCount = 0;
    activeJobId = null;
    pendingPaths = new Set();
    commitContext = null;
    commitSeq++;
  }

  /// Begin a streaming load.  The caller supplies the job_id returned by the
  /// backend and the metadata list from the `-started` event.  The store
  /// replaces its files list with placeholder entries (hunks empty) so the
  /// sidebar renders the list immediately, and tracks `pendingPaths` so the
  /// UI can show a "parsing…" badge on rows whose hunks haven't arrived yet.
  function beginStream(jobId: string, meta: DiffFile[]) {
    // Streaming diff is workdir-only — clear any leftover commit context so
    // lazy fetches don't fight the stream.
    commitContext = null;
    commitSeq++;
    activeJobId = jobId;
    totalExpected = meta.length;
    parsedCount = 0;
    files = meta;
    // Resolve the selected file, honoring an explicit pending selection first,
    // then the previously selected path if still present, then the first file.
    let chosen: DiffFile | null = null;
    if (pendingSelection) {
      chosen = meta.find(f => f.path === pendingSelection) ?? null;
      pendingSelection = null;
    }
    if (!chosen && selectedFile) {
      chosen = meta.find(f => f.path === selectedFile!.path) ?? null;
    }
    selectedFile = chosen ?? meta[0] ?? null;
    pendingPaths = new Set(meta.map(f => f.path));
    isLoading = true;
  }

  /// Record the path the caller wants selected once the next streaming load's
  /// metadata arrives.  Safe to call before invoking `getWorkdirDiffStream`.
  function setPendingSelection(path: string | null) {
    pendingSelection = path;
  }

  /// Replace a placeholder entry with the fully-parsed version.  Ignored if
  /// the job_id doesn't match the active stream (stale event from a previous
  /// request).
  function applyStreamFile(jobId: string, parsed: DiffFile) {
    if (jobId !== activeJobId) return;
    const idx = files.findIndex(f => f.path === parsed.path);
    if (idx === -1) return;
    // Replace by creating a new array so Svelte's reactivity fires.
    const next = files.slice();
    next[idx] = parsed;
    files = next;
    parsedCount += 1;
    // Remove from pending set (create new Set for reactivity).
    if (pendingPaths.has(parsed.path)) {
      const nextPending = new Set(pendingPaths);
      nextPending.delete(parsed.path);
      pendingPaths = nextPending;
    }
    // If the newly-parsed file is the currently selected one, refresh the
    // selection so DiffViewer receives the populated hunks.
    if (selectedFile && selectedFile.path === parsed.path) {
      selectedFile = next[idx];
    }
  }

  function endStream(jobId: string) {
    if (jobId !== activeJobId) return;
    activeJobId = null;
    isLoading = false;
    pendingPaths = new Set();
  }

  function failStream(jobId: string, _err: string) {
    if (jobId !== activeJobId) return;
    activeJobId = null;
    isLoading = false;
    pendingPaths = new Set();
  }

  /// Register the Tauri event listeners.  Returns an unsubscribe function
  /// bundling the three individual listeners so callers can clean up on destroy.
  async function setupListeners(): Promise<UnlistenFn> {
    const unsubStarted = await listen<StreamStartedPayload>('arbor://diff-stream-started', (ev) => {
      beginStream(ev.payload.job_id, ev.payload.files);
    });
    const unsubFile = await listen<StreamFilePayload>('arbor://diff-stream-file', (ev) => {
      applyStreamFile(ev.payload.job_id, ev.payload.file);
    });
    const unsubDone = await listen<StreamDonePayload>('arbor://diff-stream-done', (ev) => {
      endStream(ev.payload.job_id);
    });
    const unsubErr = await listen<StreamErrorPayload>('arbor://diff-stream-error', (ev) => {
      failStream(ev.payload.job_id, ev.payload.error);
    });
    return () => { unsubStarted(); unsubFile(); unsubDone(); unsubErr(); };
  }

  return {
    get files() { return files; },
    get selectedFile() { return selectedFile; },
    get isLoading() { return isLoading; },
    get totalExpected() { return totalExpected; },
    get parsedCount() { return parsedCount; },
    get pendingPaths() { return pendingPaths; },
    get mode() { return mode; },
    get wordWrap() { return wordWrap; },
    get fullFile() { return fullFile; },
    get virtThreshold() { return virtThreshold; },
    get configLoaded() { return configLoaded; },
    loadConfig,
    setFiles,
    selectFile,
    setCommitContext,
    clearCommitContext,
    ensureFileLoaded,
    setLoading,
    setMode,
    setWordWrap,
    setFullFile,
    setVirtThreshold,
    clear,
    beginStream,
    applyStreamFile,
    endStream,
    failStream,
    setPendingSelection,
    setupListeners,
  };
}

export const diffStore = createDiffStore();
