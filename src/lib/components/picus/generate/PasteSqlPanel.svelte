<script lang="ts">
  /**
   * Paste-SQL source — "I already have the INSERT, rewrite it for the other
   * branch".
   *
   * The statements are **re-read**, not string-substituted: table, columns and
   * values are extracted and fed into the same intermediate model the guided
   * form produces, so the output for every target is generated from scratch in
   * that target's dialect and shape.
   *
   * Anything unreadable is reported rather than guessed. A half-understood
   * INSERT silently turned into three files is worse than an error message.
   */
  import { ClipboardPaste, TriangleAlert, FileUp } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';

  const previewColumns = $derived<DataGridColumn[]>(
    dmlStore.columns.map((c) => ({
      id: c.name,
      label: c.name,
      hint: c.type,
      type: /NUMBER|INT|NUMERIC|DECIMAL/i.test(c.type) ? 'number' : 'text',
      width: 170,
    })),
  );

  const previewRows = $derived(
    dmlStore.importedRows.map((r) => dmlStore.columns.map((c) => (r[c.name] ?? '') || null)),
  );
</script>

<div class="ps">
  <textarea
    class="ps-text"
    spellcheck="false"
    aria-label="SQL statements to re-read"
    value={dmlStore.pasteText}
    oninput={(e) => dmlStore.setPasteText(e.currentTarget.value)}
  ></textarea>

  {#if dmlStore.pasteErrors.length}
    <div class="ps-errors" role="alert">
      <span class="ps-errors-head"><TriangleAlert size={13} /> Could not read everything</span>
      <ul>
        {#each dmlStore.pasteErrors as err, i (i)}<li>{err}</li>{/each}
      </ul>
    </div>
  {/if}

  {#if dmlStore.importedRows.length}
    <div class="ps-section">
      <span class="ps-section-title">Rows read</span>
      <Badge variant="count" label={String(dmlStore.importedRows.length)} />
    </div>
    <div class="ps-grid">
      <DataGrid
        columns={previewColumns}
        rows={previewRows}
        rowHeight={22}
        sortable={false}
        ariaLabel="Statements read from the pasted SQL"
      />
    </div>
  {/if}

  <div class="ps-actions">
    <span class="ps-hint">
      <ClipboardPaste size={12} />
      Table, columns and values are extracted and re-emitted per destination — the
      pasted text itself is never copied across.
    </span>
    <span class="ps-spacer"></span>
    <Button
      variant="secondary"
      size="sm"
      onclick={() => toastStore.show('Loading from a file arrives with the filesystem milestone.', 'info')}
    >
      {#snippet iconStart()}<FileUp size={12} />{/snippet}
      Load from a file
    </Button>
    <Button variant="primary" size="sm" onclick={() => dmlStore.parsePaste()}>
      Read and generate
    </Button>
  </div>
</div>

<style>
  .ps { display: flex; flex-direction: column; gap: 10px; }

  .ps-text {
    width: 100%;
    min-height: 150px;
    resize: vertical;
    padding: 10px 12px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 11.5px;
    line-height: 1.6;
    white-space: pre;
    outline: none;
  }
  .ps-text:focus { border-color: var(--border-focus); }

  .ps-errors {
    padding: 9px 12px;
    background: var(--error-subtle);
    border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    border-radius: var(--radius-md);
    font-size: 11.5px;
    line-height: 1.6;
  }
  .ps-errors-head {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--error);
    font-weight: 600;
  }
  .ps-errors ul { margin: 4px 0 0; padding-left: 18px; color: var(--text-secondary); }

  .ps-section { display: flex; align-items: center; gap: 8px; }
  .ps-section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .ps-grid {
    display: flex;
    height: 180px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .ps-actions { display: flex; align-items: center; gap: 8px; }
  .ps-hint {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 460px;
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--text-muted);
  }
  .ps-spacer { flex: 1; }
</style>
