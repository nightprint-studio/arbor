<!--
  StudioToolsSidebar — sidecar pane (Inspector / Schema / Bindings /
  Query siblings) collecting document transform tools. Today: Format
  button + Indent picker, plus a wrapper-supplied `extras` snippet for
  format-specific tools (RON Convert ↔ JSON, YAML Convert ↔ .properties,
  …). Designed to grow — drop more rows below as new actions land.

  Rendered by <StudioModal>'s `toolsSidecar` snippet when the wrapper's
  Tools right-rail button toggles `rightPane === 'tools'`.

  Chrome matches <StudioInspectorPanel> (sip-pane / sip-head / etc.) so
  the panel reads as one of the right-side panes rather than a one-off.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Wrench, Wand2, Indent, Check } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Dropdown from '../ui/Dropdown.svelte';
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

    onSetIndent:     (unit: string) => void | Promise<void>;
    onFormat:        () => void | Promise<void>;

    /** Format-specific rows (Convert dropdown …). Rendered below the
     *  built-in Format / Indent block, separated by a divider. */
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

<div class="sts-pane">
  <div class="sts-head">
    <Wrench size={13} />
    <span class="sts-title">Tools</span>
    <span class="sts-spacer"></span>
  </div>

  <div class="sts-body">
    <!-- Format row — runs the wrapper's canonical reformat. -->
    <div class="sts-row">
      <div class="sts-row-label">Format</div>
      <button type="button"
        class="sts-btn sts-btn-primary"
        onclick={() => void onFormat()}
        disabled={formatBtnDisabled}
        use:tooltip={formatTooltip}
      >
        <Wand2 size={13} />
        <span>Run Format</span>
      </button>
    </div>

    <!-- Indent picker — drives both Format and structural edits. -->
    <div class="sts-row">
      <div class="sts-row-label">Indent</div>
      <Dropdown items={indentItems} position="fixed" direction="down">
        {#snippet trigger({ toggle })}
          <button type="button" class="sts-btn" onclick={toggle}
            use:tooltip={indentTooltip}>
            <Indent size={13} />
            <span>{defaultIndentLabel(indentUnit)}</span>
          </button>
        {/snippet}
      </Dropdown>
    </div>

    {#if extras}
      <div class="sts-divider" role="separator"></div>
      {@render extras()}
    {/if}
  </div>
</div>

<style>
  .sts-pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }
  .sts-head {
    display: flex; align-items: center; gap: 6px;
    padding: 0 8px 0 12px;
    height: 34px;
    min-height: 34px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .sts-head > :global(svg:first-child) {
    color: var(--accent);
    flex-shrink: 0;
  }
  .sts-title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
    text-transform: uppercase;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sts-spacer { flex: 1; }

  .sts-body {
    padding: 10px 12px;
    overflow: auto;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* `:global` because wrapper-supplied `extras` rows live in the
     wrapper's CSS scope, not ours. Each wrapper rendering its own
     `.sts-row` markup inside the extras snippet picks up these
     grid + label rules without re-declaring them. */
  :global(.sts-body .sts-row) {
    display: grid;
    grid-template-columns: 64px 1fr;
    align-items: center;
    gap: 8px;
  }
  :global(.sts-body .sts-row-label) {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  /* Shared button base — used by Indent dropdown trigger AND the
     extras rendered by wrappers (Convert dropdown …). The `:global`
     wrapper is needed because the extras snippet emits markup in the
     wrapper's CSS scope, not ours. */
  :global(.sts-body .sts-btn) {
    display: inline-flex; align-items: center; gap: 6px;
    width: 100%; min-width: 0;
    padding: 5px 10px; height: 28px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    color: var(--text-primary);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  :global(.sts-body .sts-btn:hover:not(:disabled)) {
    background: var(--bg-hover);
    border-color: var(--accent);
  }
  :global(.sts-body .sts-btn:disabled) {
    color: var(--text-disabled);
    cursor: not-allowed;
  }
  :global(.sts-body .sts-btn-primary) {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, var(--border-subtle));
  }

  .sts-divider {
    height: 1px;
    background: var(--border-subtle);
    margin: 4px 0;
  }
</style>
