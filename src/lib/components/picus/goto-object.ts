/**
 * Ctrl+click on a name in SQL — "show me that".
 *
 * The one navigation an IDE user reaches for without thinking, and the one Picus
 * can answer better than any editor: the word under the caret is very often a table
 * this connection actually has, and its structure is one tab away.
 *
 * ## Why it is a lookup and not a parse
 *
 * The obvious implementation reads the statement, finds the `FROM` clause and
 * resolves the identifier properly. This one takes the word and asks the schema
 * whether it names an object. That is a deliberate trade:
 *
 *  * it works on a **half-typed** statement, which is most of the time a caret is in
 *    one, and on a fragment pasted from a log;
 *  * the schema is the authority on what exists, so a false positive would require
 *    the word to be the name of a real object — in which case showing it is not
 *    wrong, just unasked-for;
 *  * a miss is silent and costs nothing.
 *
 * Aliases are followed one step. `o.CODICE` puts the caret on `CODICE`, not on
 * `ORDINI`, so the qualifier is tried too when the word itself resolves to nothing.
 */

import { schemaStore } from '$lib/stores/picus/schema.svelte';
import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';

/** Object kinds a tab can show, in the order the schema is searched. */
type Found = { name: string; kind: 'table' | 'view' | 'sequence' | 'trigger' };

/** Does the schema know this name, and as what? */
function lookup(word: string): Found | null {
  const relation = schemaStore.relation(word);
  if (relation) return { name: relation.name, kind: relation.kind === 'view' ? 'view' : 'table' };
  const sequence = schemaStore.sequence(word);
  if (sequence) return { name: sequence.name, kind: 'sequence' };
  const trigger = schemaStore.trigger(word);
  if (trigger) return { name: trigger.name, kind: 'trigger' };
  return null;
}

/**
 * Open the schema object `word` names, if it names one.
 *
 * Returns what was opened, or `null` — the caller decides whether a miss deserves
 * a message. In an editor it does not: Ctrl+click lands on ordinary words all the
 * time, and a toast for each would make the gesture unusable.
 */
export function openObjectNamed(word: string, connectionId: string | undefined): Found | null {
  const trimmed = word.trim().replace(/^["']|["']$/g, '');
  if (!trimmed) return null;

  // `schema.table` and `alias.column` look the same from here. Both halves are
  // tried, the last first: in `ordini.codice` the interesting name is `ordini`,
  // and in `public.ordini` it is `ordini`.
  const parts = trimmed.split('.').filter(Boolean);
  const candidates = parts.length > 1 ? [parts[parts.length - 1], parts[0]] : [trimmed];

  for (const candidate of candidates) {
    const found = lookup(candidate);
    if (!found) continue;
    picusTabsStore.openObject(found.name, found.kind, connectionId);
    return found;
  }
  return null;
}
