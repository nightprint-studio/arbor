/**
 * Bennu's Git Blame annotations — who last touched each line of the open file.
 *
 * The data is Corvus's: the blame is asked of its backend by file path, with no tab involved
 * (`getFileBlameByPath`), and Corvus keeps the repository registered for a while so asking
 * again is cheap. This store owns only what Bennu adds: whether the margin is on, one result
 * per file, and the shaping of a blame into the short strings the gutter draws.
 *
 * ## It is HEAD's blame
 *
 * `git blame` answers for the committed file, so a line typed since the last commit has no
 * author yet. The gutter anchors each annotation to its line, which keeps the rest in step
 * while you type; to see a fresh answer after committing, toggle the margin off and on.
 *
 * "On" is a session toggle, deliberately not a setting: it is something you turn on to answer
 * one question about one file, not a preference about how Bennu looks.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { getFileBlameByPath } from '$lib/ipc/corvus/blameByPath';
import type { BlameLine } from '$lib/types/corvus/git';
import type { LineAnnotation } from '$lib/components/shared/ui/code-editor/line-annotations';
import { relativeTime } from '$lib/utils/diff-formatter';
import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

type Entry =
  | { state: 'loading' }
  | { state: 'ready'; lines: BlameLine[] }
  | { state: 'none'; reason: string };

/** Git's null object id — what libgit2 reports for a line that is not committed. */
const NOT_COMMITTED = /^0+$/;

/** A stable colour per commit, so a run of lines reads as one change at a glance. */
export function commitAccent(commitOid: string): string {
  let h = 0;
  for (let i = 0; i < commitOid.length; i++) h = (h * 31 + commitOid.charCodeAt(i)) >>> 0;
  return `hsl(${h % 360} 45% 55%)`;
}

/**
 * Shape a blame into editor annotations.
 *
 * Every line gets its colour bar — that is what shows where a run ends. The text after the code is
 * written once per run (its first line) rather than down forty identical lines; every other line
 * keeps it as `detail`, which the editor shows only while the caret is on it. The text is the
 * author and age; the commit message joins it only when the user asked for that (`showMessage`) —
 * it is always in the hover.
 */
export function blameToAnnotations(lines: readonly BlameLine[], showMessage = false): LineAnnotation[] {
  return lines.map((l) => {
    const uncommitted = NOT_COMMITTED.test(l.commit_oid);
    const when = relativeTime(l.timestamp);
    const who = `${l.author_name}, ${when}`;
    const text = uncommitted ? 'Not committed yet' : showMessage ? `${who} · ${l.summary}` : who;
    return {
      line: l.line_no,
      inline: l.is_group_start ? text : undefined,
      detail: text,
      tooltip: uncommitted
        ? 'This line is not committed yet'
        : `${l.short_oid}  ${l.summary}\n${l.author_name} <${l.author_email}>\n${when}`,
      accent: uncommitted ? 'var(--warning)' : commitAccent(l.commit_oid),
    };
  });
}

function createBlameStore() {
  let enabled = $state(false);
  // Replaced wholesale, never mutated: a 5 000-line blame must not become a deep reactive proxy.
  let entries = $state.raw<Record<string, Entry>>({});

  function set(path: string, entry: Entry) {
    entries = { ...entries, [path]: entry };
  }

  async function load(path: string) {
    set(path, { state: 'loading' });
    try {
      set(path, { state: 'ready', lines: await getFileBlameByPath(path) });
    } catch (e) {
      set(path, { state: 'none', reason: String(e) });
    }
  }

  return {
    get enabled() { return enabled; },

    /** Why `path` shows no blame, when it was asked for and nothing came back. */
    noneReason(path: string | null): string | null {
      const e = path ? entries[path] : undefined;
      return e?.state === 'none' ? e.reason : null;
    },

    isLoading(path: string | null): boolean {
      return path ? entries[path]?.state === 'loading' : false;
    },

    /** The gutter content for `path`: empty while off, loading, or unavailable. */
    annotationsFor(path: string | null): LineAnnotation[] {
      const e = enabled && path ? entries[path] : undefined;
      return e?.state === 'ready' ? blameToAnnotations(e.lines, bennuSettingsStore.blameShowMessage) : [];
    },

    /** Turn the margin on or off. Turning it on forgets earlier answers, so a commit made
     *  since is reflected — the off/on cycle IS the refresh. */
    toggle(path: string | null) {
      enabled = !enabled;
      entries = {};
      if (enabled && path) void load(path);
    },

    /** Called as the active file changes: fetch its blame the first time it is shown. */
    ensure(path: string | null) {
      if (enabled && path && !entries[path]) void load(path);
    },
  };
}

export const bennuBlameStore = createBlameStore();
