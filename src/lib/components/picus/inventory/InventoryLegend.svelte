<script lang="ts">
  /**
   * What the numbers in the coverage matrix mean.
   *
   * Not decoration. The matrix is the one place in Picus where a *colour* carries
   * a verdict, and a reader who has to infer that vocabulary from the cells will
   * infer it wrongly — most often by reading every dash as a problem, which turns
   * a table with four honest gaps into a table that looks broken everywhere.
   *
   * Deliberately spelled out rather than reduced to swatches: "—" and a colour
   * chip explain nothing on their own, and this legend is read once by each person
   * and then never again. Costing a few lines to be read correctly the first time
   * is the trade.
   */
  import { Info } from 'lucide-svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
</script>

<Collapsible chevron>
  {#snippet header()}
    <span class="il-head"><Info size={12} /> How to read this table</span>
  {/snippet}

  <dl class="il">
    <dt><span class="il-n">3</span></dt>
    <dd>
      Statements under that engine and role which <b>change</b> the object — create it,
      alter it, write to it, drop it. More than one is normal and not a warning: a table
      created once and altered by four update scripts reads 5.
    </dd>

    <dt><span class="il-n il-dash">—</span></dt>
    <dd>Nothing there says anything about the object.</dd>

    <dt><span class="il-n il-gap">—</span></dt>
    <dd>
      A <b>gap</b>: one side is silent about something another side installs. This is the
      same judgement the consistency report makes, so what is marked here is what
      <code>CONS001</code> raises — never more. A dash that is not a gap is left plain.
    </dd>

    <dt><span class="il-tag">read only</span></dt>
    <dd>
      Nothing in this repository creates, alters or writes to it — another repository
      installs it and a view here reads it. Its dashes are the boundary of the project,
      so none of them is ever a gap.
    </dd>

    <dt><span class="il-tag il-stray">+2 elsewhere</span></dt>
    <dd>
      Statements in folders no column covers — an ignored folder, or one with no engine.
      Counted rather than rounded away, so a folded matrix cannot look complete when it
      is not.
    </dd>
  </dl>
</Collapsible>

<style>
  .il-head {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 600;
  }
  .il-head :global(svg) { color: var(--text-muted); }

  .il {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 7px 12px;
    padding: 4px 2px 2px;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .il dt { display: flex; justify-content: center; }
  .il dd { color: var(--text-muted); }
  .il dd :global(b) { color: var(--text-secondary); font-weight: 600; }
  .il code { font-family: var(--font-code); font-size: 10.5px; }

  .il-n {
    display: inline-block;
    min-width: 30px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    text-align: center;
    font-family: var(--font-code);
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary);
  }
  .il-dash { color: var(--text-disabled); }
  .il-gap { background: var(--error-subtle); color: var(--error); font-weight: 700; }

  .il-tag {
    display: inline-block;
    padding: 0 5px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 10px;
    color: var(--text-disabled);
    white-space: nowrap;
  }
  .il-stray { border-color: transparent; color: var(--warning); }
</style>
