<script lang="ts">
  /**
   * One thing the assistant did on the way to its answer: it read a file, searched, checked its work.
   * A single quiet line — these are the receipts, not the content — with the kind of work as a small
   * tinted glyph and what came back at the right edge.
   */
  import { FileText, Search, ShieldCheck, Pencil, Check, Wand2, X } from 'lucide-svelte';
  import type { ToolCall, ToolKind } from './assistant-types';

  let { call }: { call: ToolCall } = $props();

  const ICON: Record<ToolKind, typeof FileText> = {
    read: FileText,
    search: Search,
    edit: Pencil,
    validate: ShieldCheck,
    refactor: Wand2,
  };
  const Icon = $derived(ICON[call.kind]);
</script>

<div class="tool" class:failed={call.ok === false}>
  <span class="glyph k-{call.kind}"><Icon size={11} /></span>
  <span class="label">{call.label}</span>
  <span class="detail">{call.detail}</span>
  {#if call.ok === false}<X size={11} class="bad" />{:else}<Check size={11} class="done" />{/if}
</div>

<style>
  .tool {
    display: flex; align-items: center; gap: 8px;
    padding: 4px 8px; min-width: 0;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    background: color-mix(in srgb, var(--bg-overlay) 55%, transparent);
  }
  .glyph {
    display: flex; align-items: center; justify-content: center; flex-shrink: 0;
    width: 18px; height: 18px; border-radius: 5px;
    color: var(--text-muted); background: var(--bg-overlay);
  }
  .k-read     { color: var(--info);    background: color-mix(in srgb, var(--info) 14%, transparent); }
  .k-search   { color: var(--accent);  background: var(--accent-subtle); }
  .k-edit     { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); }
  .k-validate { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); }
  .k-refactor { color: var(--accent);  background: var(--accent-subtle); }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .detail { flex-shrink: 0; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .tool.failed .detail { color: var(--danger); }
  .tool :global(.bad) { flex-shrink: 0; color: var(--danger); }
  .tool :global(.done) { flex-shrink: 0; color: var(--success); opacity: 0.8; }
</style>
