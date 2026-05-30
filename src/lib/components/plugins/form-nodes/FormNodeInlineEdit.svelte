<!--
  FormNodeInlineEdit — click-to-edit single-line field.

  Display mode: shows the current value as a clickable label (or the
  display_placeholder when empty). Activating it swaps in the shared
  <InlineEdit> widget; Enter commits, Esc reverts. There is no blur-commit
  semantics — clicking outside reverts the in-progress draft.
-->
<script lang="ts">
  import { Pencil } from 'lucide-svelte';
  import InlineEdit from '$lib/components/shared/ui/InlineEdit.svelte';

  interface Props {
    value:               string;
    placeholder?:        string;
    size?:               'sm' | 'md';
    maxlength?:          number;
    requireValue?:       boolean;
    readonly?:           boolean;
    displayPlaceholder?: string;
    onCommit:            (v: string) => void;
  }

  let {
    value,
    placeholder,
    size = 'sm',
    maxlength,
    requireValue = true,
    readonly = false,
    displayPlaceholder = '—',
    onCommit,
  }: Props = $props();

  let editing = $state(false);
  let draft   = $state('');

  function start() {
    if (readonly) return;
    draft   = value;
    editing = true;
  }
  function confirm(v: string) {
    editing = false;
    if (v !== value) onCommit(v);
  }
  function cancel() {
    editing = false;
  }
  function onKey(e: KeyboardEvent) {
    if (readonly) return;
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      start();
    }
  }
</script>

{#if editing}
  <InlineEdit
    bind:value={draft}
    {placeholder}
    {size}
    {maxlength}
    {requireValue}
    onconfirm={confirm}
    oncancel={cancel}
  />
{:else}
  <button
    type="button"
    class="pf-inline-edit pf-inline-edit-{size}"
    class:pf-inline-edit-empty={!value}
    class:pf-inline-edit-readonly={readonly}
    disabled={readonly}
    onclick={start}
    onkeydown={onKey}
  >
    <span class="pf-inline-edit-text">{value || displayPlaceholder}</span>
    {#if !readonly}<Pencil class="pf-inline-edit-icon" size={size === 'md' ? 12 : 10} />{/if}
  </button>
{/if}

<style>
  .pf-inline-edit {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    padding: 2px 6px;
    background: transparent;
    border: 1px dashed transparent;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    cursor: text;
    text-align: left;
    transition:
      background var(--transition-fast),
      border-color var(--transition-fast),
      color var(--transition-fast);
  }
  .pf-inline-edit-sm { font-size: var(--font-size-xs); }
  .pf-inline-edit-md { font-size: var(--font-size-sm); }

  .pf-inline-edit:hover:not(:disabled),
  .pf-inline-edit:focus-visible:not(:disabled) {
    background: var(--bg-overlay);
    border-color: var(--border-subtle);
    outline: none;
  }

  .pf-inline-edit-empty .pf-inline-edit-text { color: var(--text-muted); font-style: italic; }
  .pf-inline-edit-readonly  { cursor: default; opacity: 0.85; }

  .pf-inline-edit-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pf-inline-edit :global(.pf-inline-edit-icon) {
    flex-shrink: 0;
    color: var(--text-muted);
    opacity: 0;
    transition: opacity var(--transition-fast);
  }
  .pf-inline-edit:hover:not(:disabled) :global(.pf-inline-edit-icon),
  .pf-inline-edit:focus-visible:not(:disabled) :global(.pf-inline-edit-icon) {
    opacity: 1;
  }
</style>
