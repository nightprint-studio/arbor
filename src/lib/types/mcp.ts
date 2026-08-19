/**
 * MCP (the AI tool surface) — the shapes the shell exposes.
 *
 * Mirrors `src-tauri/src/config/app_config.rs` (`McpConfig` and friends) and
 * `src-tauri/src/mcp/`. Field names are the serde ones: snake_case on the wire.
 */

/** What happens when a tool of a given safety class is called. */
export type McpDecision = 'allow' | 'ask' | 'deny';

/** How project scope is decided. */
export type McpScopeMode = 'open_projects' | 'by_product' | 'allowlist' | 'anywhere';

/** Per-class policy. */
export interface McpPolicy {
  /** Tools that only observe. */
  read: McpDecision;
  /** Tools that change something recoverable. */
  write: McpDecision;
  /** Tools that delete, rewrite in bulk, or run code. */
  destructive: McpDecision;
}

/** Which paths the tools may touch. */
export interface McpScope {
  mode: McpScopeMode;
}

/** A policy where each class may decline to have an opinion. `null` → inherit. */
export interface McpPolicyOverride {
  read: McpDecision | null;
  write: McpDecision | null;
  destructive: McpDecision | null;
}

/**
 * One project's own answer to "which backends" and "what may they do".
 *
 * Kept in the user's profile keyed by path rather than in the project's own
 * `.arbor/config.toml`: a permission file inside a repository is one that gets
 * committed, and a shared repo could then hand `destructive: allow` to everyone who
 * clones it. What an AI client may do here is a fact about this user's trust in this
 * checkout.
 *
 * Every field is an override and inherit is the default, so tightening the global
 * settings still tightens every project that never disagreed with them.
 */
export interface McpProjectRule {
  /** Absolute root. Longest matching root wins when projects nest. */
  root: string;
  /** Kept alongside the path so a moved or unmounted project still reads as a name. */
  name: string;
  /** Program id → allowed here. Absent → inherit the global switch. */
  products: Record<string, boolean>;
  policy: McpPolicyOverride;
  /**
   * Tool name → decision. The most specific thing a rule can say, and it outranks the
   * class: it exists to express what a class cannot ("this project may be written to,
   * but only by the file-saving tool"), so a class that beat it would make it
   * decorative. Sparse — a rule that listed every tool would stop following the tool
   * set as it grows.
   */
  tools: Record<string, McpDecision>;
}

/** An empty rule for `root`: in the list, bound by scope, and inheriting everything. */
export function emptyProjectRule(root: string, name?: string): McpProjectRule {
  return {
    root,
    name: name ?? root.split(/[/\\]/).filter(Boolean).pop() ?? root,
    products: {},
    policy: { read: null, write: null, destructive: null },
    tools: {},
  };
}

/** Whether a rule says anything at all beyond putting the project on the list. */
export function ruleIsEmpty(rule: McpProjectRule): boolean {
  return (
    Object.keys(rule.products).length === 0 &&
    Object.keys(rule.tools ?? {}).length === 0 &&
    rule.policy.read === null &&
    rule.policy.write === null &&
    rule.policy.destructive === null
  );
}

/**
 * The rule governing `path` — longest matching root, mirroring `policy::rule_for`
 * on the Rust side so the UI never promises a resolution the backend won't make.
 */
export function ruleFor(rules: McpProjectRule[], path: string): McpProjectRule | null {
  let best: McpProjectRule | null = null;
  for (const rule of rules) {
    if (!rule.root) continue;
    if (path !== rule.root && !path.startsWith(rule.root.replace(/[/\\]$/, '') + '/')) continue;
    if (!best || rule.root.length > best.root.length) best = rule;
  }
  return best;
}

/** The whole AI-surface configuration. Everything defaults closed. */
export interface McpConfig {
  enabled: boolean;
  port: number;
  /** Bearer token clients present. Minted on first enable and kept — it lives in the
   *  client's own config, so rotating it is a deliberate act, not a side effect. */
  token: string;
  /** Program id (`bennu`, `tyto`) → exposed. Absent means off. */
  products: Record<string, boolean>;
  scope: McpScope;
  /** What a project that says nothing of its own gets. */
  policy: McpPolicy;
  /** Per-project rules — and, under `allowlist`, the scope list itself. */
  projects: McpProjectRule[];
  /** Seconds a consent prompt waits before answering "no". */
  consent_timeout_secs: number;
  /** Cap on one tool result, in bytes. */
  max_result_bytes: number;
}

/** Whether the endpoint is up, and where. */
export interface McpStatus {
  running: boolean;
  port: number;
  token: string;
  url: string;
  /** Why it isn't running, when it isn't. */
  detail: string | null;
}

/** One prompt the launcher must show. Arrives on `arbor://mcp-consent`. */
export interface McpConsentRequest {
  id: string;
  tool: string;
  title: string;
  program: string;
  safety: 'read' | 'write' | 'destructive';
  description: string;
  /** Pretty-printed JSON — the actual thing being approved. */
  arguments: string;
}

/**
 * One recorded call. Arrives on `arbor://mcp-call`, and via `get_mcp_audit`.
 *
 * A row exists from the moment the call arrives, so the same `id` is delivered several
 * times as it moves — waiting, asking, running, and finally how it ended. Consumers
 * upsert on `id` rather than appending.
 */
export interface McpAuditEntry {
  /** Process-unique. The key to upsert on — not stable across restarts. */
  id: number;
  /** The run that produced it. Compare with `McpActivityLog.run` for "this session". */
  run: number;
  at: number;
  tool: string;
  program: string;
  safety: 'read' | 'write' | 'destructive';
  /**
   * Where the call is, or how it ended. The first three are live states: `waiting` is
   * "arrived, not yet decided", `asking` is "in front of your consent prompt", `running`
   * is "in the backend".
   */
  outcome:
    | 'waiting'
    | 'asking'
    | 'running'
    | 'allowed'
    | 'asked_allowed'
    | 'asked_denied'
    | 'denied'
    | 'timed_out'
    | 'failed'
    /** Arbor stopped while it was still open. Nothing will ever close it. */
    | 'interrupted';
  arguments: string;
  duration_ms: number | null;
  detail: string | null;
  /** What the backend has said about itself while running, oldest first, capped. */
  progress: string[];
}

/** The log, plus the run reading it — so "this session" needs no guessing. */
export interface McpActivityLog {
  run: number;
  entries: McpAuditEntry[];
}

/**
 * The three safety classes, in escalating order, with the words the UI uses for them.
 *
 * One list because the same three rows appear on the defaults page, on every project's
 * rule, and in the consent prompt's tone — and three hand-written copies is how "Modify"
 * becomes "Write" on one of them.
 */
export const MCP_SAFETY_TIERS: {
  key: keyof McpPolicy;
  title: string;
  blurb: string;
}[] = [
  { key: 'read', title: 'Read', blurb: 'Observe without changing anything: list files, read source, check state.' },
  { key: 'write', title: 'Modify', blurb: 'Change something you can undo: write a file, rename a capture.' },
  { key: 'destructive', title: 'Destructive', blurb: 'Delete, rewrite in bulk, or run code — including builds and tests.' },
];

/**
 * A client that has introduced itself to the endpoint.
 *
 * Introductions, not presence: the transport issues no session ids, so only `initialize`
 * names anyone. A later call cannot be attributed, and a client that went away leaves
 * nothing behind to notice — which is why the UI says "last seen" and not "connected".
 */
export interface McpClientRecord {
  /** What it calls itself. Free text it chose — a label, not a verified identity. */
  name: string;
  version: string;
  /** The protocol revision the handshake settled on. */
  protocol: string;
  first_seen_ms: number;
  last_seen_ms: number;
  /** Handshakes this run. Climbing on its own means something keeps reconnecting. */
  handshakes: number;
}

/** Who is on the other end of the endpoint. */
export interface McpClients {
  clients: McpClientRecord[];
  /** Notification streams open right now — the only live figure here. */
  open_streams: number;
  /**
   * Authenticated requests this run, and when the last arrived (0 = never).
   *
   * The floor of "is anything talking to this", and the only form the answer can take
   * after a restart: a client that introduced itself to the previous process has no reason
   * to do it again, so its calls name nobody while still plainly being calls.
   */
  requests: number;
  last_request_ms: number;
  running: boolean;
}

/** One tool, as the AI tools reference lists it. */
export interface McpToolSummary {
  /** The name a client calls. */
  name: string;
  /**
   * The backend handler behind it — the string the call log reports. Differs from `name`
   * wherever a method name was not unique or not legible across products.
   */
  method: string;
  title: string;
  description: string;
  safety: 'read' | 'write' | 'destructive';
  idempotent: boolean;
}

/** One program's contribution to the AI surface. */
export interface McpProgramTools {
  program: string;
  /** Whether this program is exposed right now. Its tools are listed either way. */
  exposed: boolean;
  tools: McpToolSummary[];
  /** Why the list is empty, when it is. */
  detail: string | null;
}

/** The products that can be exposed, with the labels the settings panel shows. */
export const MCP_PRODUCTS: { id: string; name: string; blurb: string }[] = [
  {
    id: 'bennu',
    name: 'Bennu',
    blurb: 'Read and navigate Java and Rust projects: files in their real encoding, declared types, TODOs, index state.',
  },
  {
    id: 'tyto',
    name: 'Tyto',
    blurb: 'See the screen: capture a monitor or a window, and read its accessibility layout.',
  },
];
