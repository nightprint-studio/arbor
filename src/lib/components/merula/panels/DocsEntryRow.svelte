<script lang="ts">
  /**
   * One reference entry in the Docs panel: a collapsible row showing the name +
   * signature, with the summary, parameters and example revealed on expand. Pure
   * presentation over a {@link MerulaDslEntry} — the data comes from the catalogue.
   */
  import { ChevronRight } from 'lucide-svelte';
  import type { MerulaDslEntry } from '$lib/ipc/merula/merula';

  let {
    entry,
    color,
    expanded,
    onToggle,
  }: {
    entry: MerulaDslEntry;
    color: string;
    expanded: boolean;
    onToggle: () => void;
  } = $props();

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onToggle(); }
  }
</script>

<div class="row">
  <button
    type="button"
    class="head"
    class:open={expanded}
    onclick={onToggle}
    onkeydown={onKey}
    aria-expanded={expanded}
  >
    <span class="chev" class:open={expanded}><ChevronRight size={12} /></span>
    <code class="name" style="color: {color}">{entry.name}</code>
    <code class="sig">{entry.signature}</code>
  </button>

  {#if expanded}
    <div class="body">
      {#if entry.summary}<p class="summary">{entry.summary}</p>{/if}

      {#if entry.params.length > 0}
        <dl class="params">
          {#each entry.params as p}
            <dt>{p.optional ? `${p.name}?` : p.name}</dt>
            <dd>{p.default ? `${p.summary} (default ${p.default})` : p.summary}</dd>
          {/each}
        </dl>
      {/if}

      {#if entry.example}
        <pre class="example">{entry.example}</pre>
      {/if}
    </div>
  {/if}
</div>

<style>
  .row { display: flex; flex-direction: column; }

  .head {
    display: flex; align-items: baseline; gap: 7px;
    width: 100%; padding: 3px 8px 3px 4px;
    background: none; border: none; cursor: pointer; text-align: left;
    border-radius: var(--radius-sm, 4px);
  }
  .head:hover { background: var(--bg-hover); }

  .chev { display: inline-flex; color: var(--text-muted); transition: transform 0.12s ease; flex-shrink: 0; align-self: center; }
  .chev.open { transform: rotate(90deg); }

  .name { flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-xs); font-weight: 600; }
  .sig {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .head.open .sig { white-space: normal; }

  .body { padding: 2px 10px 8px 23px; }
  .summary { margin: 2px 0 0; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-secondary); }

  .params { margin: 7px 0 0; display: grid; grid-template-columns: auto 1fr; gap: 2px 9px; }
  .params dt {
    margin: 0; font-family: var(--font-code); font-size: var(--font-size-2xs); font-weight: 600;
    color: var(--grv-syntax-note, #e5c07b);
  }
  .params dd { margin: 0; font-size: var(--font-size-2xs); color: var(--text-secondary); line-height: 1.45; }

  .example {
    margin: 8px 0 0; padding: 6px 8px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-family: var(--font-code); font-size: var(--font-size-xs); line-height: 1.5;
    color: var(--text-primary); white-space: pre-wrap;
  }
</style>
