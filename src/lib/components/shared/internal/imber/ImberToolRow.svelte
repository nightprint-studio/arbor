<script lang="ts">
  /**
   * One thing the assistant did on the way to its answer: it read a file, searched, checked its work.
   * A single quiet line — these are the receipts, not the content. The glyph says the kind of work, and turns
   * red when the call failed or was refused; the time it took stays at the right edge whatever the line holds,
   * and what came back is cut to fit, whole in the tooltip.
   */
  import { FileText, Search, ShieldCheck, Pencil, Wand2 } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { formatDuration } from './imber-format';
  import type { ToolCall, ToolKind } from './imber-types';

  let { call }: { call: ToolCall } = $props();

  const ICON: Record<ToolKind, typeof FileText> = {
    read: FileText,
    search: Search,
    edit: Pencil,
    validate: ShieldCheck,
    refactor: Wand2,
  };
  const Icon = $derived(ICON[call.kind]);
  const failed = $derived(call.ok === false);
  const tip = $derived({
    content: call.label,
    description: [
      failed ? `Failed: ${call.detail}` : call.detail,
      call.ms !== undefined ? `Ran in ${formatDuration(call.ms)}` : '',
      call.waitMs ? `Waited ${formatDuration(call.waitMs)} for your answer first` : '',
    ].filter(Boolean).join('\n'),
    maxWidth: 520,
  });
</script>

<div class="tool" class:failed use:tooltip={tip}>
  <span class="glyph k-{call.kind}" aria-label={failed ? 'failed' : undefined}><Icon size={11} /></span>
  <span class="label">{call.label}</span>
  <span class="detail">{call.detail}</span>
  {#if call.waitMs}<span class="wait">waited {formatDuration(call.waitMs)}</span>{/if}
  {#if call.ms !== undefined}<span class="time">{formatDuration(call.ms)}</span>{/if}
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
  /* A failure wins over the kind: the one thing a glance down the list must catch. */
  .tool.failed .glyph { color: var(--danger); background: color-mix(in srgb, var(--danger) 16%, transparent); }
  /* The label keeps its words; what came back gives way first, and the time never does. */
  .label { flex: 0 1 auto; min-width: 40px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .detail {
    flex: 1 1 0; min-width: 0; text-align: right;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    color: var(--text-muted); font-variant-numeric: tabular-nums;
  }
  .tool.failed .detail { color: var(--danger); }
  .wait { flex-shrink: 0; color: var(--warning); font-variant-numeric: tabular-nums; }
  .time { flex-shrink: 0; min-width: 44px; text-align: right; color: var(--text-secondary); font-variant-numeric: tabular-nums; }
</style>
