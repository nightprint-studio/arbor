/**
 * Lazy per-file metadata for the Files panel. Reading every `.merula` file off
 * disk on project open would be wasteful, so each file's summary is parsed once
 * (on first render of its row) and cached; the active file is re-parsed live as
 * the editor buffer changes.
 *
 * The summary is a cheap text scan — no Tree-sitter, no evaluation — so it stays
 * instant even for a folder full of files. (An exact play-duration would need a
 * full eval per file; we surface the declared `cps` instead, which is free.)
 */
import { SvelteMap } from 'svelte/reactivity';
import { fsReadTextFile } from '$lib/ipc/fs';

export interface MerulaFileMeta {
  /** `meta { title = … }` front-matter title, or null. */
  title: string | null;
  /** `meta { description = … }` front-matter description, or null. */
  description: string | null;
  /** `track("…")` declarations. */
  tracks: number;
  /** `section("…")` blocks. */
  sections: number;
  /** Declared `cps(…)` tempo, or null when the file inherits the default. */
  cps: number | null;
  /** Source size in bytes (UTF-8). */
  bytes: number;
}

const encoder = new TextEncoder();

/** Pull a `key = "…"` string from the file's leading `meta { … }` block. */
function metaField(metaBody: string | null, key: string): string | null {
  if (!metaBody) return null;
  const m = metaBody.match(new RegExp(`\\b${key}\\s*=\\s*"((?:[^"\\\\]|\\\\.)*)"`));
  return m ? m[1].replace(/\\(.)/g, '$1') : null;
}

function parseMeta(src: string): MerulaFileMeta {
  const tracks = (src.match(/\btrack\(\s*"/g) ?? []).length;
  const sections = (src.match(/\bsection\(\s*"/g) ?? []).length;
  const cpsM = src.match(/\bcps\(\s*([0-9]*\.?[0-9]+)/);
  const metaBody = src.match(/\bmeta\s*\{([\s\S]*?)\}/)?.[1] ?? null;
  return {
    title: metaField(metaBody, 'title'),
    description: metaField(metaBody, 'description'),
    tracks,
    sections,
    cps: cpsM ? parseFloat(cpsM[1]) : null,
    bytes: encoder.encode(src).length,
  };
}

function createFileMetaStore() {
  const cache = new SvelteMap<string, MerulaFileMeta>();
  const loading = new Set<string>();

  return {
    /** Cached summary for `path`, or null until it's been parsed. */
    get(path: string): MerulaFileMeta | null { return cache.get(path) ?? null; },

    /** Parse + cache once. Pass an already-loaded `source` to skip the disk read
     *  (the active file's buffer is always in hand). No-op once cached. */
    async ensure(path: string, source?: string) {
      if (cache.has(path) || loading.has(path)) return;
      loading.add(path);
      try {
        const src = source ?? await fsReadTextFile(path);
        cache.set(path, parseMeta(src));
      } catch {
        /* leave uncached so a later open can retry */
      } finally {
        loading.delete(path);
      }
    },

    /** Re-parse a file whose source just changed (active-buffer edit / save). */
    refresh(path: string, source: string) { cache.set(path, parseMeta(source)); },
  };
}

export const fileMetaStore = createFileMetaStore();
