/**
 * The words on the confirmation that ends somebody else's session.
 *
 * Kept out of the panel because it is the part that has to be *written*, not the
 * part that has to be laid out — and because the two verbs differ only here. Stop
 * and cancel look identical on screen; what separates them is the sentence about
 * what happens next, and that sentence is the whole decision.
 *
 * Never "are you sure?". A confirmation that does not say what it will do is one
 * that gets agreed to without being read, and the thing this one does is roll back
 * a transaction belonging to someone who is not in the room.
 */

import type { StopKind } from '$lib/ipc/picus/activity';
import type { ActivityRow } from '$lib/stores/picus/activity.svelte';

export interface StopConfirmation {
  title: string;
  message: string;
  detail: string;
}

/** `pid 4412 (marta · psql)` — as much of an identity as the row actually has. */
function describe(row: ActivityRow): string {
  const parts = [row.session.user, row.session.application].filter(Boolean);
  return parts.length ? `pid ${row.session.pid} (${parts.join(' · ')})` : `pid ${row.session.pid}`;
}

/** What the dialog says, for one row and one verb. */
export function stopConfirmation(row: ActivityRow, kind: StopKind): StopConfirmation {
  // Said last, and said plainly: killing our own connection is legitimate — it is
  // how you get out of a wedged session — but it must not happen by accident.
  const mine = row.session.isSelf
    ? '\n\nThis is Picus’s own session: the connection this window is using.'
    : '';

  if (kind === 'cancel') {
    return {
      title: 'Cancel this statement?',
      message: `Ask ${describe(row)} to stop the statement it is running.`,
      detail:
        'The connection stays open and its transaction is not rolled back — the session '
        + 'carries on from where it was. A statement inside an uninterruptible wait may '
        + 'ignore the request.' + mine,
    };
  }
  return {
    title: 'Terminate this session?',
    message: `Close the connection of ${describe(row)}.`,
    detail:
      'Its open transaction is ROLLED BACK and everything it has not committed is lost. '
      + 'Whoever is on the other end sees their connection drop. Cancel first unless the '
      + 'session is idle and holding a lock — that is the case terminate is for.' + mine,
  };
}
