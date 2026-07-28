<script lang="ts">
  /**
   * The engine badge — in one place, for all four answers.
   *
   * The engine is a property of the folder, so it is shown constantly: on tree
   * rows, tabs, generation targets, findings. Centralising it means the colour
   * and the wording are decided once; the colours themselves come from the
   * theme's workspace ramp, never from a hex literal.
   *
   * ## Four states, and none of them may read as another
   *
   * * **A dialect** — Oracle, PostgreSQL. Coloured with its identity colour, and
   *   everything happens here.
   * * **Portable** — `both`, in the accent colour. Not a dialect and not the
   *   absence of one: these scripts run on *every* engine, count for every
   *   engine, and are generated into with what all of them accept.
   * * **Not supported** — SQL Server, DB2. Named, quiet, neutral, italic. An
   *   *answer*: Picus leaves the folder alone and stops asking about it.
   * * **No engine** — nobody has said. The muted `?`, and the only one of the
   *   four that is a question the user is expected to act on.
   *
   * The colour tiers carry the difference on their own: identity colour for a
   * dialect, accent for portable, muted-italic for unsupported, disabled for
   * unknown. That is as far as one chip goes — a fifth state would need a
   * different affordance rather than a fifth shade, and this note exists to say
   * so before somebody tries.
   */
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import {
    DIALECTS,
    FOREIGN_ENGINES,
    isDialect,
    isForeignEngine,
    isGenericEngine,
    type FolderEngine,
  } from '$lib/types/picus';

  interface Props {
    /**
     * `null` is a real answer, not a missing prop: a folder whose engine nobody
     * could identify has none, and nothing is generated into it until somebody
     * says what it is. Rendering that state is the point — it is exactly the
     * folder the user has to classify — and assuming an engine here is how this
     * component used to throw on the folders that most needed attention.
     */
    engine: FolderEngine | null | undefined;
    size?: 'sm' | 'md';
    /** Use the short code (`ORA` / `PG` / `both`) — for very tight rows. */
    terse?: boolean;
    /**
     * The value came from an ancestor rather than from this folder. Renders
     * quieter, so "set here" and "inherited" are never mistaken for each other.
     */
    inherited?: boolean;
    /** Project-relative path of the folder that declared it — named in the tooltip. */
    from?: string;
  }

  let { engine, size = 'sm', terse = false, inherited = false, from = '' }: Props = $props();

  const dialect = $derived(isDialect(engine) ? engine : null);
  const generic = $derived(isGenericEngine(engine));
  const foreign = $derived(isForeignEngine(engine) ? engine : null);
  const info = $derived(dialect ? DIALECTS[dialect] : null);

  const label = $derived.by(() => {
    if (dialect) return terse ? (dialect === 'oracle' ? 'ORA' : 'PG') : DIALECTS[dialect].short;
    if (generic) return terse ? 'both' : 'portable · both engines';
    if (foreign) return FOREIGN_ENGINES[foreign];
    return terse ? '?' : 'no engine';
  });

  /** Where the answer came from — the same clause in every tooltip below. */
  const source = $derived(
    !inherited
      ? 'declared on this folder'
      : from
        ? `inherited from ${from}`
        : 'inherited from a folder above',
  );

  const hint = $derived.by(() => {
    if (dialect) return `${DIALECTS[dialect].label} — ${source}`;
    if (generic) {
      return (
        `Portable SQL — ${source}. These scripts are written to run on Oracle and on ` +
        'PostgreSQL, so they count as present for both: neither engine is ever reported as ' +
        'missing what they contain. Anything belonging to only one engine is a finding here, ' +
        'and what is generated into them is restricted to what both accept.'
      );
    }
    if (foreign) {
      return (
        `${FOREIGN_ENGINES[foreign]} — not supported (${source}). Picus reads and generates ` +
        'Oracle and PostgreSQL; these scripts are listed and left alone. They are not parsed, ' +
        'not compared against any other folder, and nothing is ever written into them.'
      );
    }
    return 'Nothing says which engine this folder is written in, and nothing is generated into it until something does. Classify it, or a folder above it.';
  });

  /** Solid when declared here; outline when inherited. Both stay on theme tokens. */
  const tint = $derived(info ? `var(${info.colorVar})` : generic ? 'var(--accent)' : null);
  const bg = $derived(
    tint && !inherited ? `color-mix(in srgb, ${tint} 16%, transparent)` : undefined,
  );
  const border = $derived(
    tint
      ? `color-mix(in srgb, ${tint} ${inherited ? 26 : 38}%, transparent)`
      : 'var(--border-subtle)',
  );

  /**
   * Unsupported reads as **stated**, not as missing: `--text-muted` rather than
   * the `--text-disabled` of "no engine", so the two are told apart at a glance
   * on a row of eleven.
   */
  const color = $derived(tint ?? (foreign ? 'var(--text-muted)' : 'var(--text-disabled)'));
</script>

<span
  class="pdc"
  class:pdc-inherited={inherited && !!tint}
  class:pdc-foreign={!!foreign}
  use:tooltip={hint}
>
  <Badge variant="chip" {size} {color} {bg} {border} {label} />
</span>

<style>
  .pdc { display: inline-flex; }
  /* Inherited reads as an echo of the declaration above it, never as a peer. */
  .pdc-inherited { opacity: 0.66; }
  /* Somebody else's territory: present, legible, and visibly not participating. */
  .pdc-foreign { font-style: italic; }
</style>
