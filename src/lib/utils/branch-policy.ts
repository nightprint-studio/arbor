// Shared branch-creation policy.
//
// Centralises the "branch name must reference a ticket" enforcement so every
// path that creates a branch (command palette, CreateBranchModal, plugin
// shortcuts) goes through the same check. Mirrors the rule in GitFlowPanel:
// enforcement is active only when BOTH `gitflow.require_ticket_branch` is on
// AND a ticket tracker is configured.

import { getGitFlowConfig } from '$lib/ipc/corvus/gitflow';
import { getTicketLinkConfig } from '$lib/ipc/corvus/ticket_links';

export interface BranchPolicy {
  /** `gitflow.require_ticket_branch`. */
  requireTicket: boolean;
  /** Currently configured tracker (`linear`, `jira`, `github`, `gitlab`, …). */
  tracker:       string | null;
  /** Compiled regex used to test branch names. `null` when:
   *  - no tracker is configured (matches GitFlowPanel: enforcement skipped), or
   *  - a custom_pattern is set but doesn't compile in the JS engine.
   *
   *  When `null`, `assertBranchNameAllowed` returns ok regardless of the flag
   *  — same lenient behaviour as the existing GitFlow start form. */
  ticketRegex:   RegExp | null;
  /** True when `ticketRegex` is a built-in default (Linear/Jira/GitHub/GitLab).
   *  Those patterns end in `\b`, but `_` is a word char, so a branch like
   *  `feature/ABC-1_description` would fail the trailing boundary and be
   *  wrongly rejected. Mirror the Rust `parse_text` fix by normalising
   *  `_`→space before testing. Never applied to a user `custom_pattern`
   *  (the author owns its exact shape). */
  normalizeUnderscore: boolean;
}

// Mirrors the defaults used by `parse_text` in `crates/products/corvus/git/src/tickets.rs`.
// Linear/Jira: `\b[A-Z][A-Z0-9]*-\d+\b` — e.g. ABO-123, ENG-7.
// GitHub/GitLab: `#\d+` — e.g. #123.
// NB: these use `\b`; `_` is a word char, so callers normalise `_`→space
// before testing (see `normalizeUnderscore`) to match the Rust behaviour.
const DEFAULT_PATTERNS: Record<string, RegExp> = {
  linear: /\b[A-Z][A-Z0-9]*-\d+\b/,
  jira:   /\b[A-Z][A-Z0-9]*-\d+\b/,
  github: /#\d+\b/,
  gitlab: /#\d+\b/,
};

export async function getBranchPolicy(tabId: string): Promise<BranchPolicy> {
  let requireTicket = false;
  let tracker: string | null = null;
  let ticketRegex: RegExp | null = null;
  let normalizeUnderscore = false;

  try {
    const cfg = await getGitFlowConfig(tabId);
    requireTicket = !!cfg.require_ticket_branch;
  } catch { /* missing config is fine — leave flag off */ }

  if (!requireTicket) return { requireTicket, tracker, ticketRegex, normalizeUnderscore };

  try {
    const tlc = await getTicketLinkConfig(tabId);
    tracker = tlc.tracker ?? null;
    if (tlc.custom_pattern) {
      try { ticketRegex = new RegExp(tlc.custom_pattern); }
      catch (e) {
        // Rust-flavoured patterns may not compile in JS — skip enforcement
        // rather than block the user. Backend validation still runs.
        console.warn(`[branch-policy] custom_pattern doesn't compile in JS: ${e}`);
        ticketRegex = null;
      }
    } else if (tracker && tracker in DEFAULT_PATTERNS) {
      ticketRegex = DEFAULT_PATTERNS[tracker];
      normalizeUnderscore = true;
    }
  } catch { /* no tracker — return with regex null (no enforcement) */ }

  return { requireTicket, tracker, ticketRegex, normalizeUnderscore };
}

/** Returns `null` when `name` is allowed; otherwise a user-facing error. */
export function assertBranchNameAllowed(name: string, policy: BranchPolicy): string | null {
  if (!policy.requireTicket) return null;
  if (!policy.tracker)       return null;
  if (!policy.ticketRegex)   return null;
  // Built-in patterns end in `\b`, which fails when a ticket id is followed by
  // `_` (a word char) — normalise `_`→space first so `feature/ABC-1_desc` is
  // recognised. Custom patterns are tested verbatim.
  const target = policy.normalizeUnderscore ? name.replace(/_/g, ' ') : name;
  if (policy.ticketRegex.test(target)) return null;

  const example = policy.tracker === 'github' || policy.tracker === 'gitlab'
    ? '#123'
    : 'ABC-123';
  return `Branch name must reference a ticket (e.g. ${example})`;
}
