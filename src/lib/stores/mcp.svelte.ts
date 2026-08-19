/**
 * Reactive mirror of the AI tool surface: its settings, whether the endpoint is up,
 * the pending consent prompt, and the call log.
 *
 * The consent queue is the part that matters. Two calls can be waiting at once (an
 * agent runs tools in parallel), and each has its own timeout running backend-side —
 * so prompts are a **queue**, shown one at a time, and answering one reveals the next
 * rather than dismissing it.
 */

import {
  clearMcpAudit,
  getMcpAudit,
  getMcpConfig,
  getMcpStatus,
  respondMcpConsent,
  regenerateMcpToken,
  revokeMcpSessionGrants,
  setMcpConfig,
} from '$lib/ipc/mcp';
import type {
  McpAuditEntry, McpConfig, McpConsentRequest, McpProjectRule, McpStatus,
} from '$lib/types/mcp';
import { ruleFor } from '$lib/types/mcp';

const DEFAULTS: McpConfig = {
  enabled: false,
  port: 8787,
  token: '',
  products: {},
  scope: { mode: 'open_projects' },
  policy: { read: 'allow', write: 'ask', destructive: 'deny' },
  projects: [],
  consent_timeout_secs: 120,
  max_result_bytes: 256 * 1024,
};

const NO_STATUS: McpStatus = { running: false, port: 0, token: '', url: '', detail: null };

/** How many audit rows the launcher keeps in memory. The shell caps its own at 500. */
const AUDIT_CAP = 500;

function createMcpStore() {
  let config = $state<McpConfig>({ ...DEFAULTS });
  let status = $state<McpStatus>({ ...NO_STATUS });
  let loaded = $state(false);
  let saving = $state(false);
  let audit = $state<McpAuditEntry[]>([]);
  /** The run this process is on — what makes an inherited row distinguishable. */
  let auditRun = $state(0);
  let queue = $state<McpConsentRequest[]>([]);

  /**
   * Collapse rows that share an identity, keeping the more informative one.
   *
   * The log is a file two processes can both rewrite, so it can come back holding the
   * same call twice — once finished, once as it looked while still open. The rendering
   * is a KEYED each on `(run, id)`, and a duplicate key there is a hard crash that takes
   * the whole overlay tree down with it. A log that disagrees with itself is worth one
   * row shown once; it is not worth a white screen.
   *
   * Which copy wins: a finished row over a live/interrupted one, then the one that
   * collected more progress. Both are "this is the copy that still knows something".
   */
  function dedupe(rows: McpAuditEntry[]): McpAuditEntry[] {
    const LIVE = new Set(['waiting', 'asking', 'running', 'interrupted']);
    const best = new Map<string, McpAuditEntry>();
    for (const row of rows) {
      const key = `${row.run}:${row.id}`;
      const kept = best.get(key);
      if (!kept) { best.set(key, row); continue; }
      const richer =
        LIVE.has(kept.outcome) && !LIVE.has(row.outcome) ? row
        : !LIVE.has(kept.outcome) && LIVE.has(row.outcome) ? kept
        : (row.progress?.length ?? 0) > (kept.progress?.length ?? 0) ? row
        : kept;
      best.set(key, richer);
    }
    return [...best.values()];
  }

  /** In flight, so N callers on one window produce one round-trip and one answer. */
  let loading: Promise<void> | null = null;

  async function load() {
    try {
      const [cfg, st, log] = await Promise.all([getMcpConfig(), getMcpStatus(), getMcpAudit()]);
      config = cfg;
      status = st;
      audit = dedupe(log.entries);
      auditRun = log.run;
    } catch {
      // A profile with no MCP section reads as the defaults, which are all-closed.
      config = { ...DEFAULTS };
      status = { ...NO_STATUS };
    }
    loaded = true;
  }

  /**
   * Load unless this window already did.
   *
   * The store is per-window (each webview has its own module instance), so a window
   * that never loaded holds the all-closed defaults — which look exactly like a real,
   * deliberately locked-down config. Any consumer outside the home surface has to come
   * through here before reading or writing.
   */
  function ensureLoaded(): Promise<void> {
    if (loaded) return Promise.resolve();
    loading ??= load().finally(() => { loading = null; });
    return loading;
  }

  /**
   * Persist and reconcile. Re-reads from disk on failure so the UI reverts to the last
   * good value rather than showing a setting that was never saved, and rethrows so the
   * caller can toast.
   */
  async function save(next: McpConfig) {
    // The guard that makes the per-window store safe to write from anywhere. Every
    // write is a whole-config write (that is what the backend command takes), so
    // saving from a window that never loaded would push the all-closed defaults over
    // the user's real settings — turning "add a rule to this project" into "wipe the
    // endpoint, the products and every other project". One check here covers every
    // caller, present and future.
    if (!loaded) {
      throw new Error('Arbor has not read its AI-access settings in this window yet.');
    }
    saving = true;
    try {
      status = await setMcpConfig(next);
      config = next;
    } catch (e) {
      await load();
      throw e;
    } finally {
      saving = false;
    }
  }

  /** Change one field and persist. */
  async function patch(change: Partial<McpConfig>) {
    await save({ ...config, ...change });
  }

  /** Turn one product's exposure on or off. */
  async function setProduct(id: string, exposed: boolean) {
    await patch({ products: { ...config.products, [id]: exposed } });
  }

  /** Set one safety class's policy. */
  async function setPolicy(tier: keyof McpConfig['policy'], decision: McpConfig['policy'][keyof McpConfig['policy']]) {
    await patch({ policy: { ...config.policy, [tier]: decision } });
  }

  // ── Per-project rules ─────────────────────────────────────────────────────

  /** The rule governing `path`, or `null` — same longest-root resolution as the backend. */
  function projectRule(path: string): McpProjectRule | null {
    return ruleFor(config.projects, path);
  }

  /**
   * Write a rule, replacing any existing one for the same root.
   *
   * Matched on root rather than on identity because the same project reaches this from
   * two places — the settings list and the product window it is open in — and those two
   * must not be able to produce two rules that disagree about one folder.
   */
  async function saveProject(rule: McpProjectRule) {
    const rest = config.projects.filter((p) => p.root !== rule.root);
    await patch({ projects: [...rest, rule].sort((a, b) => a.root.localeCompare(b.root)) });
  }

  async function removeProject(root: string) {
    await patch({ projects: config.projects.filter((p) => p.root !== root) });
  }

  // ── Consent queue ─────────────────────────────────────────────────────────

  function enqueue(request: McpConsentRequest) {
    // A duplicate id would mean the backend asked twice for one call; ignore it rather
    // than showing the same prompt twice.
    if (queue.some((q) => q.id === request.id)) return;
    queue = [...queue, request];
  }

  async function answer(allow: boolean, remember = false) {
    const current = queue[0];
    if (!current) return;
    queue = queue.slice(1);
    try {
      await respondMcpConsent(current.id, current.tool, allow, remember);
    } catch {
      // The call already timed out backend-side, which means it was denied. Nothing
      // to correct here — the audit log will show it.
    }
  }

  // ── Audit ─────────────────────────────────────────────────────────────────

  /**
   * Fold one delivery into the log.
   *
   * Upsert, not append: a call is delivered several times as it moves from waiting to
   * however it ended, and appending each would show one call as five rows counting down
   * its own lifetime.
   */
  function record(entry: McpAuditEntry) {
    // `(run, id)`, not `id`: the backend's counter restarts with the process, so a row
    // carried over from an earlier run collides with this run's first call.
    const at = audit.findIndex((e) => e.id === entry.id && e.run === entry.run);
    if (at === -1) {
      audit = [entry, ...audit].slice(0, AUDIT_CAP);
      return;
    }
    const next = [...audit];
    next[at] = entry;
    audit = next;
  }

  /** Re-read the log. The live mirror only fills while a window is listening, and the
   *  backend's ring is the real record — so a panel opening asks rather than assumes. */
  async function refreshAudit() {
    const log = await getMcpAudit();
    audit = dedupe(log.entries);
    auditRun = log.run;
  }

  async function clearAudit() {
    await clearMcpAudit();
    audit = [];
  }

  /** Rotate the credential. Reloads, because the config now holds a different token. */
  async function regenerateToken() {
    status = await regenerateMcpToken();
    config = await getMcpConfig();
  }

  async function revokeGrants() {
    await revokeMcpSessionGrants();
  }

  async function refreshStatus() {
    status = await getMcpStatus();
  }

  return {
    get config() { return config; },
    get status() { return status; },
    get loaded() { return loaded; },
    get saving() { return saving; },
    get audit() { return audit; },
    get auditRun() { return auditRun; },
    /** The prompt to show, if any. */
    get pending() { return queue[0] ?? null; },
    /** How many more are behind it. */
    get queued() { return Math.max(0, queue.length - 1); },
    load,
    ensureLoaded,
    save,
    patch,
    setProduct,
    setPolicy,
    projectRule,
    saveProject,
    removeProject,
    enqueue,
    answer,
    record,
    refreshAudit,
    clearAudit,
    regenerateToken,
    revokeGrants,
    refreshStatus,
  };
}

export const mcpStore = createMcpStore();
