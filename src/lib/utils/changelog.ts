// CHANGELOG.md is the canonical source — imported as raw text at build time
// so we don't pay runtime fetch cost or risk the file being out of sync
// with the shipped binary. Vite allows imports from the project root.
import changelogRaw from '../../../CHANGELOG.md?raw';

/** Order matters for rendering: groups appear in this sequence. */
export const CHANGELOG_GROUPS = ['Added', 'Changed', 'Fixed', 'Deprecated', 'Removed', 'Security'] as const;
export type ChangelogGroup = typeof CHANGELOG_GROUPS[number];

export interface ChangelogEntry {
  /** Version string, e.g. "0.3.0" or "Unreleased". */
  version: string;
  /** ISO-ish date string from the heading, e.g. "2026-05-24". `null` for Unreleased. */
  date: string | null;
  /** Free-form intro paragraph between the version heading and the first
   *  group heading. Trimmed; empty when absent. */
  intro: string;
  /** Bullet items grouped by category. Each item is the raw markdown text
   *  of one `-` bullet, with hard-wrapped continuation lines re-joined. */
  groups: Partial<Record<ChangelogGroup, string[]>>;
}

const HEADING_VERSION = /^##\s+\[([^\]]+)\](?:\s*[—\-–]\s*(.+))?\s*$/;
const HEADING_GROUP   = /^###\s+(.+?)\s*$/;
const BULLET          = /^[-*]\s+(.+)$/;

/** Parse a CHANGELOG.md string into structured entries. Forgiving: stops
 *  collecting bullets at the next heading; unknown group names are kept
 *  as-is so future categories don't get silently dropped. */
export function parseChangelog(raw: string): ChangelogEntry[] {
  const lines = raw.split(/\r?\n/);
  const entries: ChangelogEntry[] = [];

  let current: ChangelogEntry | null = null;
  let currentGroup: string | null = null;
  let introBuf: string[] = [];
  let bulletBuf: string[] | null = null;

  const flushBullet = () => {
    if (bulletBuf && current && currentGroup) {
      const arr = (current.groups[currentGroup as ChangelogGroup] ??= []);
      arr.push(bulletBuf.join(' ').replace(/\s+/g, ' ').trim());
    }
    bulletBuf = null;
  };

  const flushIntro = () => {
    if (current) current.intro = introBuf.join('\n').trim();
    introBuf = [];
  };

  for (const line of lines) {
    const mVer = HEADING_VERSION.exec(line);
    if (mVer) {
      flushBullet();
      flushIntro();
      current = { version: mVer[1], date: mVer[2]?.trim() ?? null, intro: '', groups: {} };
      currentGroup = null;
      entries.push(current);
      continue;
    }
    if (!current) continue;

    const mGroup = HEADING_GROUP.exec(line);
    if (mGroup) {
      flushBullet();
      flushIntro();
      currentGroup = mGroup[1];
      continue;
    }

    const mBullet = BULLET.exec(line);
    if (mBullet) {
      flushBullet();
      if (currentGroup) {
        bulletBuf = [mBullet[1].trim()];
      }
      continue;
    }

    // Continuation of a bullet (hard-wrapped line) — same indent prefix.
    if (bulletBuf && /^\s+\S/.test(line)) {
      bulletBuf.push(line.trim());
      continue;
    }

    // Intro paragraph (between version heading and first group heading).
    if (currentGroup === null && line.trim()) {
      introBuf.push(line);
    }
  }
  flushBullet();
  flushIntro();

  return entries;
}

let cached: ChangelogEntry[] | null = null;

/** Parsed CHANGELOG, memoized — the raw markdown is bundled so this is
 *  cheap, but the modal opens often enough that re-parsing on every show
 *  would be wasteful. */
export function getChangelog(): ChangelogEntry[] {
  if (!cached) cached = parseChangelog(changelogRaw);
  return cached;
}

/** Look up the entry for a specific version. Falls back to the `Unreleased`
 *  section first (dev builds with the current version not yet cut into a
 *  release), then to the most recent released entry. */
export function findEntry(version: string): ChangelogEntry | null {
  const all = getChangelog();
  return all.find(e => e.version === version)
      ?? all.find(e => e.version === 'Unreleased')
      ?? all.find(e => e.version !== 'Unreleased')
      ?? null;
}

/** All entries between `from` (exclusive) and `to` (inclusive) — used when
 *  the user has skipped one or more versions and we want to show the
 *  accumulated notes in one go. SemVer-ordered descending (newest first).
 *  Unreleased is excluded. */
export function entriesSince(from: string | null, to: string): ChangelogEntry[] {
  const all = getChangelog().filter(e => e.version !== 'Unreleased');
  if (!from) return all.filter(e => cmpSemver(e.version, to) <= 0);
  return all.filter(e => cmpSemver(e.version, from) > 0 && cmpSemver(e.version, to) <= 0);
}

/** Lenient SemVer compare: returns -1 / 0 / 1. Non-numeric segments compare
 *  lexically; missing segments are treated as 0. */
export function cmpSemver(a: string, b: string): number {
  const pa = a.split('.').map(s => parseInt(s, 10));
  const pb = b.split('.').map(s => parseInt(s, 10));
  const n = Math.max(pa.length, pb.length);
  for (let i = 0; i < n; i++) {
    const ai = pa[i] ?? 0, bi = pb[i] ?? 0;
    if (Number.isNaN(ai) || Number.isNaN(bi)) {
      const sa = String(a.split('.')[i] ?? ''), sb = String(b.split('.')[i] ?? '');
      if (sa !== sb) return sa < sb ? -1 : 1;
      continue;
    }
    if (ai !== bi) return ai < bi ? -1 : 1;
  }
  return 0;
}
