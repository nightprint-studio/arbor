<script lang="ts">
  /**
   * KeyValueEditor — the rows of params, headers, form fields and variables, as a table or as bulk text.
   *
   * The table is the everyday view (a checkbox switches a row off without losing it); the text view takes
   * `key: value`, `k=v&k2=v2` or a block of headers copied from a browser, one entry per line, and the
   * backend turns it into rows (`kv_parse`) — the same parser the file format uses, so what you paste is
   * what a diff will show. A last, empty row is always there to type into.
   */
  import { untrack } from 'svelte';
  import { Trash2, Lock, ListChecks } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import SuggestInput, { type SuggestResult } from '$lib/components/shared/ui/SuggestInput.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import { kvFormat, kvParse } from '$lib/ipc/hirundo';
  import type { KeyValue } from '$lib/types/hirundo';
  import { SECRET_MARK } from '$lib/types/hirundo';
  import { blankRow } from '../request-model';

  interface Props {
    rows: KeyValue[];
    onchange: (rows: KeyValue[]) => void;
    /** Start in the bulk-edit text instead of the table (the window's Ctrl+Shift+B drives this). */
    bulk?: boolean;
    /** Called when the header toggle is used, for a caller that keeps the mode itself; without it the table keeps its own. */
    onToggleBulk?: () => void;
    keyPlaceholder?: string;
    valuePlaceholder?: string;
    /** Rows may be marked `@secret` (variables only). */
    allowSecret?: boolean;
    /** The description column (params and variables have one; form fields need not). */
    describe?: boolean;
    /** Completions for the key field and for the value field (given its row's key). Without them the fields are plain. */
    suggestKey?: (text: string, caret: number) => SuggestResult | null;
    suggestValue?: (key: string, text: string, caret: number) => SuggestResult | null;
  }

  let { rows, onchange, bulk: bulkProp = false, onToggleBulk, keyPlaceholder = 'Key', valuePlaceholder = 'Value', allowSecret = false, describe = true, suggestKey, suggestValue }: Props = $props();

  // The mode is the table's own, followed from the caller when the caller drives it.
  let bulkLocal = $state(false);
  $effect(() => { bulkLocal = bulkProp; });
  const bulk = $derived(bulkLocal);
  function toggleBulk() {
    if (onToggleBulk) onToggleBulk(); else bulkLocal = !bulkLocal;
  }

  // Always one empty row at the end to type into; it is not part of the saved rows until it has a key.
  const shown = $derived([...rows, blankRow()]);

  function commit(next: KeyValue[]) {
    onchange(next.filter((r) => r.key !== '' || r.value !== '' || r.description !== ''));
  }

  function patch(i: number, change: Partial<KeyValue>) {
    const next = shown.map((r, at) => (at === i ? { ...r, ...change } : r));
    commit(next);
  }

  function remove(i: number) {
    commit(shown.filter((_, at) => at !== i));
  }

  // ── bulk text ──────────────────────────────────────────────────────────────────────────────────
  let text = $state('');
  let seeded = $state(false);
  /** The text last handed to the parser (or produced by the formatter): typing is only parsed when it differs. */
  let settled = '';

  // Seed the text once per entry into bulk mode; while typing, the text is the source of truth.
  $effect(() => {
    if (bulk && !seeded) {
      seeded = true;
      void kvFormat(untrack(() => rows)).then((t) => { settled = t; text = t; });
    } else if (!bulk) {
      seeded = false;
    }
  });

  $effect(() => {
    const current = text;
    if (!bulk || current === settled) return;
    settled = current;
    void kvParse(current).then(onchange).catch(() => { /* a half-typed line is not an error */ });
  });
</script>

<div class="kv-head">
  <span class="kv-title">{bulk ? 'Bulk edit' : 'Table'}</span>
  <ToggleButton pressed={bulk} icon={ListChecks} title={bulk ? 'Back to the table' : 'Bulk edit — one entry per line, or paste a block'} shortcut="Ctrl+Shift+B" onclick={toggleBulk} />
</div>
{#if bulk}
  <div class="kv-bulk">
    <p class="kv-hint">One entry per line: <code>key: value</code>, <code>k=v&amp;k2=v2</code>, or paste a block of headers. Start a line with <code>//</code> to switch it off.</p>
    <Textarea bind:value={text} minRows={10} maxRows={26} framed ariaLabel="Bulk edit" />
  </div>
{:else}
  {#if rows.length === 0}
    <p class="kv-empty">Nothing here yet. Type in the empty row, or <Button variant="link" size="xs" onclick={toggleBulk}>paste as bulk</Button> — one entry per line.</p>
  {/if}
  <div class="kv" role="table">
    <div class="kv-row kv-cols" class:no-desc={!describe} role="row" aria-hidden="true">
      <span></span><span>{keyPlaceholder}</span><span>{valuePlaceholder}</span>{#if describe}<span>Description</span>{/if}<span></span>
    </div>
    {#each shown as row, i (i)}
      {@const last = i === shown.length - 1}
      <div class="kv-row" class:off={!row.enabled && !last} class:no-desc={!describe} role="row">
        <span class="kv-check">
          {#if !last}<Checkbox checked={row.enabled} ariaLabel="Enabled" onchange={(v) => patch(i, { enabled: v })} />{/if}
        </span>
        <div class="kv-cell">
          {#if suggestKey}
            <SuggestInput value={row.key} placeholder={keyPlaceholder} quiet suggest={suggestKey} oninput={(v) => patch(i, { key: v })} />
          {:else}
            <Input value={row.key} placeholder={keyPlaceholder} size="sm" quiet oninput={(v) => patch(i, { key: v })} />
          {/if}
        </div>
        <div class="kv-cell">
          {#if allowSecret && row.value === SECRET_MARK}
            <span class="kv-secret"><Lock size={12} /> Secret — kept in the keyring</span>
          {:else if suggestValue}
            <SuggestInput value={row.value} placeholder={valuePlaceholder} quiet suggest={(t, c) => suggestValue(row.key, t, c)} oninput={(v) => patch(i, { value: v })} />
          {:else}
            <Input value={row.value} placeholder={valuePlaceholder} size="sm" quiet oninput={(v) => patch(i, { value: v })} />
          {/if}
        </div>
        {#if describe}
          <div class="kv-cell kv-desc"><Input value={row.description} placeholder="Description" size="sm" quiet oninput={(v) => patch(i, { description: v })} /></div>
        {/if}
        <span class="kv-actions">
          {#if allowSecret && !last}
            <IconButton tooltip={row.value === SECRET_MARK ? 'Stop being a secret' : 'Make it a secret (kept in the keyring, never in a file)'} size={22} active={row.value === SECRET_MARK} onclick={() => patch(i, { value: row.value === SECRET_MARK ? '' : SECRET_MARK })}><Lock size={12} /></IconButton>
          {/if}
          {#if !last}<IconButton tooltip="Remove" size={22} variant="danger" onclick={() => remove(i)}><Trash2 size={12} /></IconButton>{/if}
        </span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .kv-empty { margin: 4px 12px 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .kv-head { display: flex; align-items: center; justify-content: space-between; padding: 8px 14px 2px; }
  .kv-title { font-size: var(--font-size-2xs); font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-muted); }
  /* The rows sit on a rounded, slightly raised card with room at the sides, like the panels of the Git view. */
  .kv {
    display: flex; flex-direction: column; margin: 6px 12px 12px; padding: 4px 8px 6px;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle); border-radius: var(--radius-lg);
  }
  .kv-row {
    display: grid; align-items: center; gap: 8px; padding: 3px 0;
    grid-template-columns: 22px minmax(0, 1fr) minmax(0, 1.4fr) minmax(0, 1fr) 48px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .kv-row.no-desc { grid-template-columns: 22px minmax(0, 1fr) minmax(0, 1.4fr) 48px; }
  .kv-row:last-child { border-bottom: none; }
  .kv-row:not(.kv-cols):hover { background: color-mix(in srgb, var(--text-primary) 4%, transparent); border-radius: var(--radius-sm); }
  .kv-cols { padding: 4px 0 6px; font-size: var(--font-size-2xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
  .kv-cols > span { padding-left: 7px; }
  .kv-row.off .kv-cell { opacity: 0.5; }
  .kv-check { display: grid; place-items: center; }
  .kv-cell { min-width: 0; }
  .kv-actions { display: flex; justify-content: flex-end; gap: 2px; }
  .kv-secret { display: inline-flex; align-items: center; gap: 6px; padding: 0 8px; font-size: var(--font-size-xs); color: var(--warning); }
  .kv-bulk { padding: 8px 12px 12px; display: flex; flex-direction: column; gap: 8px; }
  .kv-hint { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .kv-hint code { font-family: var(--font-mono); }
</style>
