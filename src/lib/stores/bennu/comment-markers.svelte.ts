/**
 * Bennu comment-marker settings — whether `TODO` / `FIXME`… are coloured in comments, which words count,
 * and whether the line gets a gutter dot.
 *
 * The same words feed the TODO tool window, so the editor and the window cannot disagree. Persisted in the
 * per-profile Bennu config (`comment_marker*` keys) by a read-modify-write of the whole config, like the
 * regex store — never `localStorage`.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md · "Store pattern").
 */

import { getBennuConfig, setBennuConfig } from '$lib/ipc/bennu/config';
import { DEFAULT_KEYWORDS, effectiveKeywords, parseKeywords } from '$lib/components/bennu/comment-markers/markers';

function createCommentMarkersStore() {
  let enabled = $state(true);
  let keywords = $state<string[]>([...DEFAULT_KEYWORDS]);
  let caseSensitive = $state(true);
  let gutter = $state(false);

  async function persist(patch: {
    comment_markers?: boolean; comment_marker_keywords?: string[];
    comment_marker_case_sensitive?: boolean; comment_marker_gutter?: boolean;
  }) {
    try {
      const cur = await getBennuConfig();
      await setBennuConfig({ ...cur, ...patch });
    } catch {
      /* BE absent — the choice holds for this session */
    }
  }

  return {
    get enabled() { return enabled; },
    setEnabled(v: boolean) { enabled = v; void persist({ comment_markers: v }); },
    /** The words in force (the built-in list when the configured one is empty). */
    get keywords() { return keywords; },
    /** Take the list as the settings field writes it. An empty or invalid list falls back to the built-in words. */
    setKeywords(text: string) {
      keywords = effectiveKeywords(parseKeywords(text));
      void persist({ comment_marker_keywords: keywords });
    },
    get caseSensitive() { return caseSensitive; },
    setCaseSensitive(v: boolean) { caseSensitive = v; void persist({ comment_marker_case_sensitive: v }); },
    get gutter() { return gutter; },
    setGutter(v: boolean) { gutter = v; void persist({ comment_marker_gutter: v }); },

    async loadConfig() {
      try {
        const cfg = await getBennuConfig();
        enabled = cfg.comment_markers ?? true;
        keywords = effectiveKeywords(cfg.comment_marker_keywords ?? []);
        caseSensitive = cfg.comment_marker_case_sensitive ?? true;
        gutter = cfg.comment_marker_gutter ?? false;
      } catch {
        /* keep defaults — BE absent / not ready */
      }
    },
  };
}

export const bennuCommentMarkersStore = createCommentMarkersStore();
