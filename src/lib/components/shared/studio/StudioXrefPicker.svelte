<script lang="ts" generics="T extends XrefEntryShape">
  import type { Snippet } from 'svelte';
  import { FileText as FileTextIcon } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  export interface XrefEntryShape {
    sourcePath?: string;
    fileName?:   string;
    title?:      string;
    defPath:     string[];
    /** `null` is allowed so domain entries that distinguish "not resolved
     *  yet" (null) from "resolved-but-not-open" (undefined) can still
     *  satisfy the shape — the picker only checks truthiness. */
    docId?:      string | null;
  }

  interface PickerState<E> {
    x: number;
    y: number;
    entries: E[];
  }

  interface Props<E extends XrefEntryShape> {
    picker:    PickerState<E> | null;
    portal:    (node: HTMLElement) => { destroy(): void };
    onPick:    (entry: E) => void;
    onDismiss: () => void;
    /** Optional custom icon snippet per entry. Default = lucide `FileText`. */
    icon?:     Snippet<[E]>;
    /** Optional label override; default = `entry.fileName ?? entry.title ?? ''`. */
    entryLabel?: (entry: E) => string;
    ariaLabel?: string;
  }

  let {
    picker,
    portal,
    onPick,
    onDismiss,
    icon,
    entryLabel,
    ariaLabel = 'Cross-reference matches',
  }: Props<T> = $props();

  function labelOf(e: T): string {
    if (entryLabel) return entryLabel(e);
    return e.fileName ?? e.title ?? '';
  }
</script>

{#if picker}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div use:portal>
    <div class="sxp-overlay"
         role="presentation"
         onclick={onDismiss}
         oncontextmenu={(e) => { e.preventDefault(); onDismiss(); }}
    ></div>
    <div class="sxp-popover"
         style:left="{picker.x}px"
         style:top="{picker.y}px"
         role="menu"
         aria-label={ariaLabel}
         onclick={(e) => e.stopPropagation()}
    >
      <div class="sxp-header">{picker.entries.length} matches</div>
      {#each picker.entries as entry, i (i)}
        <button
          type="button"
          class="sxp-item"
          role="menuitem"
          onclick={() => onPick(entry)}
          use:tooltip={entry.sourcePath || labelOf(entry)}
        >
          <span class="sxp-item-icon" aria-hidden="true">
            {#if icon}{@render icon(entry)}{:else}<FileTextIcon size={13} />{/if}
          </span>
          <span class="sxp-item-name">{labelOf(entry)}</span>
          <span class="sxp-item-path">{entry.defPath.join('.')}</span>
          {#if entry.docId}
            <span class="sxp-item-open" use:tooltip={'Already open'}>•</span>
          {:else}
            <span class="sxp-item-chevron" aria-hidden="true">›</span>
          {/if}
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .sxp-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: transparent;
    cursor: default;
  }
  .sxp-popover {
    position: fixed;
    z-index: 61;
    min-width: 220px;
    max-width: 380px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .sxp-header {
    padding: 4px 8px 6px;
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 2px;
  }
  .sxp-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 8px;
    background: transparent;
    color: var(--text-primary);
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    font-size: 12px;
  }
  .sxp-item:hover { background: var(--bg-hover); }
  .sxp-item-icon { display: inline-flex; align-items: center; flex-shrink: 0; }
  .sxp-item-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-ui-sans);
    font-weight: 500;
  }
  .sxp-item-path {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .sxp-item-chevron,
  .sxp-item-open {
    color: var(--accent);
    font-size: 14px;
    line-height: 1;
    margin-left: 2px;
  }
</style>
