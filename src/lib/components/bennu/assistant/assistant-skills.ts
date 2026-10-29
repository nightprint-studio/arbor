/**
 * The `/skill` shortcut of the composer, and the filter of the skills manager.
 *
 * Typing `/` at the start of a message offers the skills the user enabled; choosing one writes `/name `
 * into the text, and the backend reads a message that begins with `/name` as a request for that skill.
 * A slash further into the sentence (a path, a fraction) is just text, so only the leading run of
 * `/name` words counts. Pure, so it is tested.
 */

import type { SkillInfo } from '$lib/ipc/bennu/assistant';

/** The part of a skill the suggestions and the filter need. */
export type SkillLike = Pick<SkillInfo, 'name' | 'description'>;

const WORD = /^[A-Za-z0-9_-]*$/;

/**
 * What is being typed after a leading `/`: the characters of the last word, and where its slash is.
 * `null` when the cursor (the end of `draft`) is not in a leading `/word`.
 */
export function slashQuery(draft: string): { query: string; start: number } | null {
  const lead = draft.length - draft.trimStart().length;
  const words = draft.slice(lead).split(/(\s+)/);
  let offset = lead;
  for (let i = 0; i < words.length; i++) {
    const w = words[i];
    const last = i === words.length - 1;
    if (/^\s+$/.test(w) || w === '') { offset += w.length; continue; }
    if (!w.startsWith('/') || !WORD.test(w.slice(1))) return null;
    if (last) return { query: w.slice(1), start: offset };
    offset += w.length;
  }
  return null;
}

/** The skills whose name starts with `query` first, then those that contain it, then by description. */
export function matchSkills<T extends SkillLike>(skills: readonly T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...skills];
  const rank = (s: T): number => {
    const name = s.name.toLowerCase();
    if (name.startsWith(q)) return 0;
    if (name.includes(q)) return 1;
    return s.description.toLowerCase().includes(q) ? 2 : 3;
  };
  return skills.filter((s) => rank(s) < 3).sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name));
}

/** `draft` with the word being typed replaced by `/name `. */
export function applySlash(draft: string, name: string): string {
  const at = slashQuery(draft);
  const head = at ? draft.slice(0, at.start) : draft;
  return `${head}/${name} `;
}

/** `12 KB`, `900 B` — the size of a SKILL.md. */
export function formatSkillSize(bytes: number): string {
  return bytes >= 1024 ? `${(bytes / 1024).toFixed(bytes >= 10 * 1024 ? 0 : 1)} KB` : `${bytes} B`;
}

/** The badge text for where a skill came from: `Project · .claude`, `User · bennu`. */
export function sourceLabel(s: Pick<SkillInfo, 'scope' | 'source'>): string {
  return `${s.scope === 'project' ? 'Project' : 'User'} · ${s.source}`;
}
