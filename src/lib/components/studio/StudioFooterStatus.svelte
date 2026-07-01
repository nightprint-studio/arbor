<!--
  StudioFooterStatus — left-aligned footer cluster for <StudioModal>.

  Render order:
    1. parse-error / dirty / saved pill
    2. wrapper-supplied `extras` snippet (RON: schema + refs pills)
    3. selected-tree-path pill (suppressed when `selectedPath === null`,
       overridable via the `selectedPathSlot` snippet — Properties uses
       it for the `$value` self-marker)
    4. encoding pill (UTF-8 / windows-1252 / BOM)

  Drops into `<StudioModal>`'s `footerStatusLeft` snippet — the shell
  wraps everything inside a flex row so this component just emits
  inline pill nodes.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { AlertCircle, Check } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { StudioFooterDoc } from './studio-footer-types';

  interface Props {
    doc:              StudioFooterDoc;
    /** Strategy for the parse-error pill text.
     *  · `"short"`     → "parse error" literal
     *                    (RON / JSON / TOML / YAML).
     *  · `"truncated"` → first `;`-separated segment of `parseError`
     *                    (Properties — more actionable inside the pill). */
    errorPillStrategy?: 'short' | 'truncated';
    /** Selected tree-path pill data. `null` ⇒ suppressed. */
    selectedPath?:    string[] | null;
    /** Optional override snippet for the selected-path pill. When set,
     *  fully replaces the default `$.a.b.c` rendering. */
    selectedPathSlot?: Snippet<[{ path: string[] }]>;
    /** Rendered after the standard status pill and before the
     *  selected-path / encoding pills. RON uses it for the schema +
     *  refs index pills. */
    extras?:          Snippet<[]>;
  }

  let {
    doc,
    errorPillStrategy = 'short',
    selectedPath      = null,
    selectedPathSlot,
    extras,
  }: Props = $props();

  const encDefault = $derived(
    !!doc.encoding && doc.encoding.label === 'UTF-8' && !doc.encoding.had_bom,
  );
</script>

{#if doc.parseError}
  <span class="sf-pill sf-pill-warn" use:tooltip={doc.parseError}>
    <AlertCircle size={11} />
    {#if errorPillStrategy === 'truncated'}
      {doc.parseError.split(';')[0]}
    {:else}
      parse error
    {/if}
  </span>
{:else if doc.dirty}
  <span class="sf-pill sf-pill-dirty">
    <span class="sf-dot"></span> modified
  </span>
{:else}
  <span class="sf-pill sf-pill-ok">
    <Check size={11} /> saved
  </span>
{/if}

{@render extras?.()}

{#if selectedPath !== null}
  {#if selectedPathSlot}
    {@render selectedPathSlot({ path: selectedPath })}
  {:else}
    <span class="sf-path" use:tooltip={'Selected node path'}>
      {selectedPath.length === 0 ? '$' : '$.' + selectedPath.join('.')}
    </span>
  {/if}
{/if}

{#if doc.docId && doc.encoding}
  {@const enc = doc.encoding}
  <span class="sf-pill sf-pill-encoding"
        class:sf-pill-encoding-default={encDefault}
        use:tooltip={enc.had_bom
          ? `Encoding: ${enc.label} (with BOM). Save round-trips the original encoding.`
          : `Encoding: ${enc.label}. Save round-trips the original encoding.`}>
    {enc.label}{enc.had_bom ? ' · BOM' : ''}
  </span>
{/if}

<style>
  .sf-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 10px;
    line-height: 1.4;
    white-space: nowrap;
  }
  .sf-pill-warn {
    background: color-mix(in srgb, var(--warning, #d19a66) 18%, transparent);
    color: var(--warning, #d19a66);
    max-width: 360px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sf-pill-dirty    { background: var(--bg-overlay); color: var(--accent); }
  .sf-pill-ok       { background: var(--bg-overlay); color: var(--text-secondary); }
  .sf-pill-encoding { background: var(--bg-overlay); color: var(--text-secondary); font-family: var(--font-code); }
  .sf-pill-encoding-default { color: var(--text-muted); }

  .sf-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    display: inline-block;
  }

  .sf-path {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 2px 6px;
    border-radius: 999px;
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
