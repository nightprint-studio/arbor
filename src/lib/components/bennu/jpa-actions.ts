/**
 * What each JPA toolbar action opens, keyed by the id the backend contributed.
 *
 * ## Who owns what
 *
 * The **toolbar** is the backend's: which buttons exist on this file, their labels, their
 * grouping and their tooltips all come from `bennu_ext_actions`, because "is this an entity or a
 * repository" is a question about Java that a Svelte file has no business answering. Nothing here
 * decides what to show.
 *
 * This table owns the other half: **what happens when one is chosen** — which dialog, which
 * generator verb, and the presets that make the dialog a focused form rather than a
 * configuration screen.
 *
 * ## Why the presets exist at all
 *
 * The kind is picked *before* the dialog opens, so each dialog has one job and a title that
 * names it. The first version put the choice inside as a tab strip, which meant two thirds of the
 * form was irrelevant at any moment and the title said nothing.
 *
 * That is also why `find one` and `find many` are separate actions rather than one action with a
 * checkbox: they are two different things you set out to write, and the return type follows from
 * which one you picked instead of from a toggle you have to remember to flip.
 */

import type { JpaReturnShape } from '$lib/ipc/bennu/jpa';

/** The `kind` sent to `bennu_jpa_generate`. */
export type JpaGenerateKind =
  | 'repository'
  | 'projection'
  | 'query-method'
  | 'attribute'
  | 'named-query'
  | 'lifecycle'
  | 'modify-method';

/** Which body the dialog renders. */
export type JpaFormKind =
  | 'repository'
  | 'projection'
  | 'query'
  | 'attribute'
  | 'named-query'
  | 'lifecycle'
  | 'modify';

export interface JpaActionSpec {
  /** The backend action id this answers for. */
  id: string;
  /** Dialog title — names the one job. */
  title: string;
  form: JpaFormKind;
  kind: JpaGenerateKind;
  /**
   * Which file the result lands in. Drives the seeding: an action on an entity starts from the
   * entity you were looking at, one on a repository starts from that repository.
   */
  target: 'entity' | 'repository';
  /** Query methods: the verb and the shape it returns. */
  subject: string;
  /** The return shape the action starts from. The form can change it — this is where it opens,
   *  which is the whole reason `find one` and `find many` are separate buttons. */
  returns: JpaReturnShape;
  distinct: boolean;
  /** Modify methods: a delete rather than an update. */
  delete: boolean;
  /** Lifecycle callbacks: which one. */
  event: string;
}

const BASE: Omit<JpaActionSpec, 'id' | 'title' | 'form' | 'kind' | 'target'> = {
  subject: 'find',
  returns: 'optional',
  distinct: false,
  delete: false,
  event: '',
};

const spec = (s: Partial<JpaActionSpec> & Pick<JpaActionSpec, 'id' | 'title' | 'form' | 'kind' | 'target'>): JpaActionSpec =>
  ({ ...BASE, ...s });

const TABLE: JpaActionSpec[] = [
  // On an entity.
  spec({ id: 'jpa.attribute', title: 'Add entity attribute', form: 'attribute', kind: 'attribute', target: 'entity' }),
  spec({ id: 'jpa.lifecycle', title: 'Add lifecycle callback', form: 'lifecycle', kind: 'lifecycle', target: 'entity' }),
  spec({ id: 'jpa.named-query', title: 'Add named query', form: 'named-query', kind: 'named-query', target: 'entity' }),
  spec({ id: 'jpa.repository', title: 'Generate repository', form: 'repository', kind: 'repository', target: 'entity' }),
  spec({ id: 'jpa.projection', title: 'Create projection', form: 'projection', kind: 'projection', target: 'entity' }),

  // On a repository — read.
  spec({ id: 'jpa.query.single', title: 'Create find instance method', form: 'query', kind: 'query-method', target: 'repository' }),
  spec({ id: 'jpa.query.list', title: 'Create find collection method', form: 'query', kind: 'query-method', target: 'repository', returns: 'list' }),
  spec({ id: 'jpa.query.page', title: 'Create paged query', form: 'query', kind: 'query-method', target: 'repository', returns: 'page' }),
  spec({ id: 'jpa.query.count', title: 'Create count method', form: 'query', kind: 'query-method', target: 'repository', subject: 'count' }),
  spec({ id: 'jpa.query.exists', title: 'Create exists method', form: 'query', kind: 'query-method', target: 'repository', subject: 'exists' }),

  // On a repository — write.
  spec({ id: 'jpa.modify.update', title: 'Create update method', form: 'modify', kind: 'modify-method', target: 'repository' }),
  spec({ id: 'jpa.modify.delete', title: 'Create delete method', form: 'modify', kind: 'modify-method', target: 'repository', delete: true }),
];

/**
 * The dialog for an action id, or `null` when the id is one this frontend does not know.
 *
 * `null` rather than a fallback: an extension that grows an action a build of the UI has never
 * heard of should do nothing visible, not open the wrong dialog and generate something the user
 * did not ask for.
 *
 * The lifecycle ids are resolved by prefix, because their suffix is the event itself — the
 * backend owns the list of the seven callbacks and adding an eighth (there will not be one, but
 * that is not the point) costs nothing here.
 */
export function jpaActionSpec(id: string): JpaActionSpec | null {
  const event = id.startsWith('jpa.lifecycle.') ? id.slice('jpa.lifecycle.'.length) : '';
  if (event) {
    return spec({
      id,
      title: `Add @${event} callback`,
      form: 'lifecycle',
      kind: 'lifecycle',
      target: 'entity',
      event,
    });
  }
  return TABLE.find((a) => a.id === id) ?? null;
}

/**
 * Every generation reachable by name from the Command Palette.
 *
 * The toolbar's list is per-file and comes from the backend; this one is per-project and cannot,
 * so it is the whole table. The seven lifecycle callbacks are one entry rather than seven: the
 * dialog asks which event, which keeps the palette readable and keeps the list of callbacks
 * where it belongs — in the backend, not in a second copy here.
 */
export const JPA_PALETTE_ACTIONS: JpaActionSpec[] = TABLE;
