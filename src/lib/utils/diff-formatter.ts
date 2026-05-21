import Prism from 'prismjs';
import type { DiffFile } from '../types/git';
import { CUSTOM_HIGHLIGHTERS } from './prism-languages';
// Centralised Prism language registrations (also imported by `highlight.ts`
// for the read-only code blocks in plugin forms / JSON Studio modal). Add
// new grammars in `./prism-shared.ts` so both consumers see them.
import './prism-shared';

const EXT_TO_LANG: Record<string, string> = {
  // TypeScript / JavaScript
  ts: 'typescript', tsx: 'typescript',
  js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript',
  // Rust + RON (no dedicated grammar — Rust is close enough)
  rs: 'rust', ron: 'rust',
  // Python
  py: 'python',
  // JVM
  java: 'java',
  kt: 'kotlin', kts: 'kotlin',
  // Data
  json: 'json',
  toml: 'toml',
  yaml: 'yaml', yml: 'yaml',
  // Styles
  css: 'css',
  scss: 'scss', sass: 'scss',
  // Shell / scripts
  sh: 'bash', bash: 'bash', zsh: 'bash',
  bat: 'batch', cmd: 'batch',
  // Docs
  md: 'markdown', mdx: 'markdown',
  // Markup
  html: 'markup', xml: 'markup', svg: 'markup', htm: 'markup',
  // C family
  c: 'c', h: 'c',
  cpp: 'cpp', cc: 'cpp', cxx: 'cpp', hpp: 'cpp',
  cs: 'csharp',
  // Other languages
  go: 'go',
  sql: 'sql',
  swift: 'swift',
  // Shader languages
  glsl: 'glsl', vert: 'glsl', frag: 'glsl',
  wgsl: 'glsl', // no dedicated Prism grammar — glsl is the closest
  // Svelte
  svelte: 'svelte',
  // Scripting
  lua: 'lua',
  // PowerShell
  ps1: 'powershell', psm1: 'powershell', psd1: 'powershell',
  // Template / server-side markup
  jsp: 'markup', // no dedicated Prism grammar — markup (HTML) is the closest
};

const FILENAME_TO_LANG: Record<string, string> = {
  dockerfile: 'docker',
};

export function getLanguage(path: string): string {
  const filename = path.split('/').pop()?.toLowerCase() ?? '';
  if (FILENAME_TO_LANG[filename]) return FILENAME_TO_LANG[filename];
  const ext = filename.split('.').pop() ?? '';
  return EXT_TO_LANG[ext] ?? 'plain';
}

// Memoize per-line highlight output. Conflict-resolver and diff views
// re-render the same lines repeatedly on every reactive update (checkbox
// toggle, selection change, etc.) — running Prism for every line each time
// is O(N) per render and freezes the app on large files. The cache reduces
// repeated highlights of the same string to a Map lookup.
//
// Key uses a NUL separator so a lang named e.g. "ts" can never collide with
// a code line that happens to start with "s:". We cap the cache size and
// drop a chunk when full (FIFO via Map's insertion order — good enough,
// no LRU bookkeeping needed for this workload).
const _highlightCache = new Map<string, string>();
const HIGHLIGHT_CACHE_MAX = 50_000;

export function highlight(code: string, path: string): string {
  const lang = getLanguage(path);
  const key  = `${lang}\u0000${code}`;
  const hit  = _highlightCache.get(key);
  if (hit !== undefined) return hit;

  let out: string;
  if (lang === 'plain') {
    out = escapeHtml(code);
  } else {
    try {
      const custom = CUSTOM_HIGHLIGHTERS[lang];
      if (custom)                    out = custom(code);
      else if (!Prism.languages[lang]) out = escapeHtml(code);
      else                            out = Prism.highlight(code, Prism.languages[lang], lang);
    } catch {
      out = escapeHtml(code);
    }
  }

  if (_highlightCache.size >= HIGHLIGHT_CACHE_MAX) {
    // Drop oldest 25% in one pass (cheap eviction, no LRU promotion needed).
    const drop = Math.floor(HIGHLIGHT_CACHE_MAX / 4);
    const it   = _highlightCache.keys();
    for (let i = 0; i < drop; i++) {
      const k = it.next().value;
      if (k === undefined) break;
      _highlightCache.delete(k);
    }
  }
  _highlightCache.set(key, out);
  return out;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

export function totalStats(files: DiffFile[]): { additions: number; deletions: number } {
  return files.reduce(
    (acc, f) => ({
      additions: acc.additions + f.stats.additions,
      deletions: acc.deletions + f.stats.deletions,
    }),
    { additions: 0, deletions: 0 }
  );
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function formatTimestamp(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function relativeTime(seconds: number): string {
  const diff = Date.now() / 1000 - seconds;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)}d ago`;
  if (diff < 86400 * 365) return `${Math.floor(diff / 86400 / 30)}mo ago`;
  return `${Math.floor(diff / 86400 / 365)}y ago`;
}