/**
 * Avatar cache — provider-resolved (GitHub / GitLab) with initials fallback.
 *
 * For each visible commit author email we ask the backend to look the user
 * up via the active repo's GitProvider:
 *   • GitHub `users.noreply.github.com` emails → `GET /users/:username`.
 *   • Other GitHub emails → `GET /search/users?q=…+in:email` (matches only
 *     when the user has made the email public).
 *   • GitLab emails → `GET /users?search=…` against the host the repo is
 *     hosted on (gitlab.com or self-hosted).
 *
 * If the provider can't resolve the email (no remote, no token, email not
 * exposed) we fall through to a deterministic initials SVG generated
 * client-side. Gravatar is intentionally NOT used: coverage was too low
 * to justify the per-email 404 it produced for every fallback.
 *
 * Usage:
 *   await ensureAvatar(email, name, tabId);   // call for each visible node
 *   const url = avatarUrl(email, name);       // read in SVG <image href>
 *
 * `avatarUrl` reads a reactive $state tick so components automatically
 * re-render once the provider lookup resolves.
 */

import { resolveAvatarForEmail } from '$lib/ipc/avatar';

// ── Initials fallback ─────────────────────────────────────────────────────────

function colorFor(email: string): string {
  let h = 0;
  for (const c of email) h = (Math.imul(31, h) + c.charCodeAt(0)) | 0;
  return `hsl(${Math.abs(h) % 360},46%,36%)`;
}

function initialsOf(name: string): string {
  return (
    name
      .trim()
      .split(/\s+/)
      .map(w => w[0] ?? '')
      .join('')
      .slice(0, 2)
      .toUpperCase() || '?'
  );
}

export function makeInitialsUrl(name: string, email: string): string {
  const color    = colorFor(email);
  const text     = initialsOf(name);
  const s        = 24;
  const fs       = 9;
  const svg = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${s}" height="${s}" viewBox="0 0 ${s} ${s}">`,
    `<circle cx="${s / 2}" cy="${s / 2}" r="${s / 2}" fill="${color}"/>`,
    `<text x="${s / 2}" y="${s / 2 + 3.5}" text-anchor="middle"`,
    ` font-family="system-ui,sans-serif" font-size="${fs}"`,
    ` fill="white" font-weight="600">${text}</text>`,
    `</svg>`,
  ].join('');
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

// ── Cache + reactive state ────────────────────────────────────────────────────

const emailToUrl = new Map<string, string>();
const loading    = new Set<string>();
let _tick = $state(0);

/** Synchronously returns the best available URL for this email.
 *  Falls back to a generated initials avatar until the provider lookup resolves. */
export function avatarUrl(email: string, name: string): string {
  _tick; // reactive dependency
  return emailToUrl.get(email) ?? makeInitialsUrl(name, email);
}

/** Kick off the provider lookup for this email (no-op if already cached
 *  or already in flight). `tabId` is required to know which provider to
 *  ask; without it we just leave the initials avatar in place. */
export async function ensureAvatar(email: string, name: string, tabId: string | null): Promise<void> {
  if (!tabId)                                 return;
  if (emailToUrl.has(email) || loading.has(email)) return;
  loading.add(email);
  try {
    const url = await resolveAvatarForEmail(tabId, email);
    emailToUrl.set(email, url ?? makeInitialsUrl(name, email));
  } finally {
    loading.delete(email);
    _tick++;
  }
}
