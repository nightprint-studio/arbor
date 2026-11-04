/**
 * Bennu regex store — the Regex Tester's session state and what it keeps between sessions.
 *
 * Two kinds of state, kept apart on purpose:
 *   • **Session** — whether the modal is open and the pattern it was opened with, plus the
 *     write-back target when it was opened from a literal in the editor (`target`).
 *   • **Persisted** — the detection level, the named patterns and the recent ones. They live in the
 *     per-profile Bennu config (`regex_*` keys), never in `localStorage`: a read-modify-write of the
 *     whole config, like the settings store's, so the two stay independent but do not clobber each
 *     other's keys.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md · "Store pattern").
 */

import { getBennuConfig, setBennuConfig, type BennuRegexPattern } from '$lib/ipc/bennu/config';
import type { RegexDetection } from '$lib/components/bennu/inner-lang/detect';
import type { Flavour } from '$lib/components/bennu/regex/types';

export const REGEX_DETECTIONS = ['off', 'context', 'names'] as const;

const RECENT_LIMIT = 20;

/** Where an edited pattern goes back to: the literal(s) it was read from, in one buffer. */
export interface RegexWriteBack {
  file: string;
  /** Applies the new pattern to the literal it came from. Returns false when the buffer moved on. */
  apply: (pattern: string) => boolean;
}

export interface RegexOpenRequest {
  pattern: string;
  flavour: Flavour;
  flags: string;
  target: RegexWriteBack | null;
  /** The pattern is a fragment (`"a" + x + "b"`): writing back is allowed, diagnostics are not trusted. */
  partial?: boolean;
}

export type KeptPattern = BennuRegexPattern;

function createRegexStore() {
  let detection = $state<RegexDetection>('context');
  let saved = $state<KeptPattern[]>([]);
  let recent = $state<KeptPattern[]>([]);
  let request = $state<RegexOpenRequest | null>(null);

  async function persist(patch: { regex_detection?: string; regex_saved?: KeptPattern[]; regex_recent?: KeptPattern[] }) {
    try {
      const cur = await getBennuConfig();
      await setBennuConfig({ ...cur, ...patch });
    } catch {
      /* BE absent — the choice holds for this session */
    }
  }

  const sameEntry = (a: KeptPattern, b: KeptPattern) =>
    a.pattern === b.pattern && a.flags === b.flags && a.flavour === b.flavour;

  return {
    get detection() { return detection; },
    setDetection(v: RegexDetection) { detection = v; void persist({ regex_detection: v }); },
    get saved() { return saved; },
    get recent() { return recent; },
    /** The open request, or null when the tester is closed. */
    get request() { return request; },
    open(req: RegexOpenRequest) { request = req; },
    close() { request = null; },

    /** Keep `entry` under its name (replacing one of the same name). */
    save(entry: KeptPattern) {
      saved = [entry, ...saved.filter((s) => s.name !== entry.name)];
      void persist({ regex_saved: saved });
    },
    remove(name: string) {
      saved = saved.filter((s) => s.name !== name);
      void persist({ regex_saved: saved });
    },
    /** Remember a pattern that was actually tried, newest first, without duplicates. */
    remember(entry: KeptPattern) {
      if (!entry.pattern.trim()) return;
      recent = [entry, ...recent.filter((r) => !sameEntry(r, entry))].slice(0, RECENT_LIMIT);
      void persist({ regex_recent: recent });
    },
    clearRecent() {
      recent = [];
      void persist({ regex_recent: [] });
    },

    async loadConfig() {
      try {
        const cfg = await getBennuConfig();
        detection = (REGEX_DETECTIONS as readonly string[]).includes(cfg.regex_detection)
          ? (cfg.regex_detection as RegexDetection)
          : 'context';
        saved = cfg.regex_saved ?? [];
        recent = cfg.regex_recent ?? [];
      } catch {
        /* keep defaults — BE absent / not ready */
      }
    },
  };
}

export const bennuRegexStore = createRegexStore();
