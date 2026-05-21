<!--
  StudioFooterCenter — center cluster for <StudioModal>'s footer slot.

  Render order:
    1. Undo
    2. Redo
    3. divider
    4. Indent dropdown (configurable options + label callback)
    5. Format button
    6. wrapper-supplied `extras` snippet
       · RON  → Convert RON ↔ JSON dropdown
       · YAML → Convert YAML ↔ .properties dropdown

  Drops into `<StudioModal>`'s `footerCenter` snippet.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Undo2, Redo2, Indent, Wand2, Check } from 'lucide-svelte';
  import Dropdown from '../ui/Dropdown.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { indentLabel as defaultIndentLabel } from './helpers';
  import {
    DEFAULT_INDENT_OPTIONS,
    type IndentChoice, type StudioFooterDoc,
  } from './studio-footer-types';

  interface Props {
    doc:             StudioFooterDoc;
    actionBusy:      boolean;
    indentUnit:      string;
    indentOptions?:  IndentChoice[];
    indentTooltip?:  string;
    formatTooltip?:  string;
    /** Optional explicit override; defaults to `actionBusy || parseError`. */
    formatDisabled?: boolean;

    onUndo:          () => void | Promise<void>;
    onRedo:          () => void | Promise<void>;
    onSetIndent:     (unit: string) => void | Promise<void>;
    onFormat:        () => void | Promise<void>;

    extras?:         Snippet<[]>;
  }

  let {
    doc,
    actionBusy,
    indentUnit,
    indentOptions   = DEFAULT_INDENT_OPTIONS,
    indentTooltip   = 'Indent — applied to Format and tree edits',
    formatTooltip   = 'Format — re-emit canonical form',
    formatDisabled,
    onUndo,
    onRedo,
    onSetIndent,
    onFormat,
    extras,
  }: Props = $props();

  const formatBtnDisabled = $derived(
    formatDisabled ?? (actionBusy || !!doc.parseError),
  );

  const indentItems = $derived(indentOptions.map((opt) => ({
    kind:    'item' as const,
    id:      opt.id,
    label:   opt.label,
    onclick: () => void onSetIndent(opt.unit),
    icon:    indentUnit === opt.unit ? Check : undefined,
  })));
</script>

<button type="button" class="sf-btn"
  onclick={() => void onUndo()}
  disabled={!doc.canUndo}
  use:tooltip={{ content: 'Undo', shortcut: 'Ctrl+Z' }}
  aria-label="Undo"
><Undo2 size={15} /></button>

<button type="button" class="sf-btn"
  onclick={() => void onRedo()}
  disabled={!doc.canRedo}
  use:tooltip={{ content: 'Redo', shortcut: 'Ctrl+Shift+Z' }}
  aria-label="Redo"
><Redo2 size={15} /></button>

<span class="sf-sep" aria-hidden="true"></span>

<Dropdown items={indentItems} position="fixed" direction="up">
  {#snippet trigger({ toggle })}
    <button class="sf-btn" onclick={toggle} use:tooltip={indentTooltip}>
      <Indent size={15} /> <span>{defaultIndentLabel(indentUnit)}</span>
    </button>
  {/snippet}
</Dropdown>

<button type="button" class="sf-btn"
  onclick={() => void onFormat()}
  disabled={formatBtnDisabled}
  use:tooltip={formatTooltip}
>
  <Wand2 size={15} /> <span>Format</span>
</button>

{@render extras?.()}

<style>
  .sf-btn {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 8px; height: 22px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    color: var(--text-primary);
    border-radius: 4px;
    font-size: 11px;
    cursor: pointer;
  }
  .sf-btn:hover:not(:disabled) { background: var(--bg-hover); }
  .sf-btn:disabled { color: var(--text-disabled); cursor: not-allowed; }

  .sf-sep {
    width: 1px;
    height: 18px;
    background: var(--border-subtle);
    margin: 0 4px;
  }
</style>
