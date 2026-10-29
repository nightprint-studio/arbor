import type { BuildQueueOwner, BuildQueueRow } from '$lib/ipc/bennu/build-queue';

const KIND_WORD = { build: 'build', test: 'test run', check: 'check' } as const;

/** Who a ticket belongs to, as a person would say it. */
export const ownerName = (o: BuildQueueOwner): string => (o.conversation ? (o.title.trim() || 'a conversation') : 'You');

/** `#2 · waiting behind build of <title>` / `#1 · running`. */
export function queueLine(r: BuildQueueRow): string {
  const place = `#${r.position}`;
  if (r.state === 'running') return `${place} · running ${KIND_WORD[r.kind]}`;
  if (r.state === 'riding') return `${place} · will share the ${KIND_WORD[r.kind]} ahead of it`;
  if (!r.behind) return `${place} · next`;
  return `${place} · waiting behind ${KIND_WORD[r.behind.kind]} of ${ownerName(r.behind.owner)}`;
}
