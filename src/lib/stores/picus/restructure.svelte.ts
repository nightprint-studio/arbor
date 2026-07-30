/**
 * Structural search and replace, as the workspace holds it.
 *
 * ## Finding and rewriting are two decisions, not one
 *
 * A pattern with no replacement is a **query over the repository** — "every INSERT
 * on this table, with its columns and its values" — and that is a use of its own,
 * which is why the results are exportable and why nothing about the flow requires
 * a template. The replacement, when there is one, is checked against every match
 * *before* a preview is asked for: each row carries what it would become, so a
 * template that indexes past the end of a list says so on the row it fails, not as
 * one error for the whole migration.
 *
 * ## Anything that moves invalidates the preview
 *
 * The pattern, the template and the scope all feed the same key. Editing any of
 * them drops the preview rather than leaving a diff on screen that describes an
 * earlier version of the question — which the write would refuse anyway, and being
 * refused at the last step is a worse way to find out.
 */

import {
  structuralApply,
  structuralFind,
  structuralPreview,
  type FoundMatch,
  type RestructurePreview,
  type RestructureScope,
} from '$lib/ipc/picus/restructure';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

import { picusProjectStore } from './project.svelte';

function createRestructureStore() {
  let pattern = $state('');
  let replacement = $state('');
  let scope = $state<RestructureScope>({});

  let matches = $state<FoundMatch[]>([]);
  let placeholders = $state<string[]>([]);
  let scanned = $state(0);
  let searched = $state(false);
  let searching = $state(false);
  let searchError = $state<string | null>(null);

  let preview = $state<RestructurePreview | null>(null);
  let previewing = $state(false);
  let previewError = $state<string | null>(null);
  /** The question the current preview answers. Anything else makes it stale. */
  let previewKey = $state('');

  let writing = $state(false);

  /**
   * Which placeholder the matches are grouped by, and which group is shown.
   *
   * This is the half of the feature that is not about rewriting at all. Group by
   * `$cols$` over every INSERT on a table and the distinct values *are* the
   * distinct column orders in use: one group of nine thousand is the convention,
   * a group of eleven is the deviation nobody noticed. Finding those is worth more
   * than rewriting them blind, and it is the reading that tells you whether a
   * rewrite is even the right answer.
   */
  let groupBy = $state<string | null>(null);
  let groupValue = $state<string | null>(null);

  /** The question the matches on screen answer. */
  let matchKey = $state('');
  let findSeq = 0;

  /** Normalised for comparison: two column lists differing only in whitespace or
   *  in casing are the same list, and grouping them apart would report a conflict
   *  that is not one. */
  function normalise(value: string): string {
    return value.replace(/\s+/g, ' ').trim().toLowerCase();
  }

  /** Everything the answer depends on, in one string. */
  const key = $derived(
    JSON.stringify({ pattern: pattern.trim(), replacement: replacement.trim(), scope }),
  );

  return {
    get pattern() { return pattern; },
    get replacement() { return replacement; },
    get scope() { return scope; },
    get matches() { return matches; },
    get placeholders() { return placeholders; },
    get scanned() { return scanned; },
    get searched() { return searched; },
    get searching() { return searching; },
    get searchError() { return searchError; },
    get preview() { return preview; },
    get previewing() { return previewing; },
    get previewError() { return previewError; },
    get writing() { return writing; },

    get groupBy() { return groupBy; },
    get groupValue() { return groupValue; },

    /**
     * The distinct values of the grouping capture, commonest first.
     *
     * Commonest first because that ordering **is** the answer: the top row is what
     * the repository does, and everything under it is what somebody did once. A
     * count alone would not say which is which.
     */
    get groups(): { value: string; count: number; files: number; deviant: boolean }[] {
      if (!groupBy) return [];
      const by = new Map<string, { value: string; count: number; files: Set<string> }>();
      for (const match of matches) {
        const raw = match.captures[groupBy] ?? '';
        const key = normalise(raw);
        const entry = by.get(key) ?? { value: raw, count: 0, files: new Set<string>() };
        entry.count += 1;
        entry.files.add(match.path);
        by.set(key, entry);
      }
      const ordered = [...by.values()].sort((a, b) => b.count - a.count);
      return ordered.map((g, index) => ({
        value: g.value,
        count: g.count,
        files: g.files.size,
        // Everything that is not the commonest. Deliberately not a percentage
        // threshold: with two shapes at 50/50 there is no convention to deviate
        // from, and calling one of them wrong would be inventing an answer.
        deviant: index > 0 && ordered.length > 1,
      }));
    },

    /** The matches actually shown — every one, or the group that is selected. */
    get visibleMatches(): FoundMatch[] {
      if (!groupBy || groupValue === null) return matches;
      // Copied out of the closure variable: narrowing does not survive into a
      // callback for something that is still assignable from elsewhere.
      const name = groupBy;
      const wanted = normalise(groupValue);
      return matches.filter((m) => normalise(m.captures[name] ?? '') === wanted);
    },

    setGroupBy(name: string | null) {
      groupBy = name;
      groupValue = null;
    },
    showGroup(value: string | null) { groupValue = value; },

    /** Matches whose replacement could not be rendered — the rows to fix first. */
    get failing() { return matches.filter((m) => m.problem); },
    /** Files the matches are spread over, which is the number that decides whether
     *  this is a small edit or a migration. */
    get fileCount() { return new Set(matches.map((m) => m.path)).size; },

    /** The preview describes an older question. */
    get stale() { return !!preview && previewKey !== key; },
    /** The matches describe an older question. Kept on screen rather than cleared:
     *  they are the last answer to a *similar* one and re-running is one click,
     *  whereas emptying the table on every keystroke makes the pattern box
     *  unusable. Marked, so nobody reads them as current. */
    get matchesStale() { return searched && matchKey !== key; },

    get canPreview() {
      return (
        !!pattern.trim() &&
        !!replacement.trim() &&
        matches.length > 0 &&
        this.failing.length === 0 &&
        picusProjectStore.attached
      );
    },

    setPattern(text: string) {
      pattern = text;
      this.invalidate();
    },
    setReplacement(text: string) {
      replacement = text;
      this.invalidate();
    },
    setScope(next: RestructureScope) {
      scope = next;
      this.invalidate();
    },

    /**
     * The question changed. The preview goes — a diff on screen that describes an
     * earlier version of the question is the one thing this flow must never show —
     * and the matches stay, marked stale by `matchesStale`.
     */
    invalidate() {
      preview = null;
      previewError = null;
    },

    async find() {
      const root = picusProjectStore.root;
      if (!root || !pattern.trim()) return;
      const mine = ++findSeq;
      searching = true;
      searchError = null;
      try {
        const found = await structuralFind(
          root,
          pattern,
          replacement.trim() || undefined,
          scope,
        );
        if (mine !== findSeq) return;
        matches = found.matches;
        placeholders = found.placeholders;
        scanned = found.scanned;
        searched = true;
        matchKey = key;
        // The grouping is about the previous answer; keeping it would filter the
        // new one by a value that may not occur in it at all.
        groupValue = null;
        if (groupBy && !found.placeholders.includes(groupBy)) groupBy = null;
        preview = null;
      } catch (e) {
        if (mine !== findSeq) return;
        matches = [];
        placeholders = [];
        scanned = 0;
        searched = true;
        matchKey = key;
        searchError = String(e);
      } finally {
        if (mine === findSeq) searching = false;
      }
    },

    async buildPreview() {
      const root = picusProjectStore.root;
      if (!root || !this.canPreview || previewing) return;
      previewing = true;
      previewError = null;
      const asked = key;
      try {
        preview = await structuralPreview(root, pattern, replacement, scope);
        previewKey = asked;
      } catch (e) {
        preview = null;
        previewError = String(e);
      } finally {
        previewing = false;
      }
    },

    /**
     * Write it. Refuses on a stale preview here as well as on the backend: being
     * told at the last step is correct but late, and the button should not have
     * been pressable.
     */
    async write() {
      const root = picusProjectStore.root;
      if (!root || !preview || writing) return;
      if (previewKey !== key) {
        toastStore.show('The pattern changed since the preview. Compute it again.', 'warning');
        return;
      }
      writing = true;
      try {
        const digests = preview.files.map((f) => ({ path: f.path, digest: f.digest }));
        const done = await structuralApply(root, pattern, replacement, digests, scope);
        toastStore.show(
          done.written.length
            ? `${done.written.length} file${done.written.length === 1 ? '' : 's'} rewritten.`
            : 'Every file was already as the transformation would leave it.',
          'success',
        );
        preview = null;
        // The repository on disk is not the one the matches describe.
        await picusProjectStore.refresh();
        await this.find();
      } catch (e) {
        toastStore.show(String(e), 'error');
      } finally {
        writing = false;
      }
    },

    reset() {
      pattern = '';
      replacement = '';
      scope = {};
      matches = [];
      placeholders = [];
      scanned = 0;
      searched = false;
      searchError = null;
      preview = null;
      previewError = null;
    },
  };
}

export const restructureStore = createRestructureStore();
