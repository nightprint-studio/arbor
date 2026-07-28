<script lang="ts">
  /**
   * CSV import — delimiter sniffing, explicit header → column mapping, and a
   * preview that shows which rows would not survive their column types.
   *
   * The mapping is **explicit on purpose**. A same-name match is proposed
   * automatically because it is right most of the time, but a header that maps
   * to nothing is shown as ignored rather than quietly dropped: importing 300
   * rows and silently losing a column is exactly the class of mistake Picus is
   * supposed to remove.
   *
   * The file is read with the same encoding logic as the scripts — a CSV
   * exported from a legacy system is usually windows-1252 too.
   */
  import { FileSpreadsheet, TriangleAlert, Upload } from 'lucide-svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { parseCsv } from '$lib/utils/picus/csv';

  const parsed = $derived(parseCsv(dmlStore.csvText));

  /** Column choices per header: every table column, plus "ignore". */
  const options = $derived([
    { value: '', label: '— ignore this column —' },
    ...dmlStore.columns.map((c) => ({ value: c.name, label: `${c.name}  (${c.type})` })),
  ]);

  const mappedCount = $derived(parsed.headers.filter((h) => dmlStore.csvMapping[h]).length);
  const invalidRows = $derived(dmlStore.rowIssues.size);
  const validRows = $derived(Math.max(0, dmlStore.importedRows.length - invalidRows));

  /** Preview columns mirror the table's own, so problems land where you expect. */
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

  const delimiterLabel = $derived(
    parsed.delimiter === '\t' ? 'tab' : parsed.delimiter === ';' ? 'semicolon' : parsed.delimiter === ',' ? 'comma' : 'pipe',
  );
</script>

<div class="cg">
  <div class="cg-meta">
    <span use:tooltip={'Sniffed from the header line'}>
      <Badge variant="tone" tone="neutral" size="sm" label={`delimiter: ${delimiterLabel}`} />
    </span>
    <Badge variant="tone" tone="neutral" size="sm" label="first row = header" />
    <Badge variant="tone" tone="neutral" size="sm" label="windows-1252" />
    <span class="cg-spacer"></span>
    <Button
      variant="secondary"
      size="xs"
      onclick={() => toastStore.show('The file picker arrives with the filesystem milestone.', 'info')}
    >
      {#snippet iconStart()}<Upload size={12} />{/snippet}
      Choose a file…
    </Button>
  </div>

  <textarea
    class="cg-text"
    spellcheck="false"
    aria-label="CSV contents"
    value={dmlStore.csvText}
    oninput={(e) => dmlStore.setCsvText(e.currentTarget.value)}
  ></textarea>

  <!-- Mapping: one row per CSV header. Same-name pairs are proposed; the rest
       are the user's call, and an unmapped header says so out loud. -->
  <div class="cg-section">
    <span class="cg-section-title">Column mapping</span>
    <span class="cg-section-meta">{mappedCount} of {parsed.headers.length} headers mapped</span>
  </div>

  <div class="cg-map">
    {#each parsed.headers as header (header)}
      <div class="cg-map-row">
        <span class="cg-header">{header}</span>
        <span class="cg-arrow" aria-hidden="true">→</span>
        <Select
          value={dmlStore.csvMapping[header] ?? ''}
          {options}
          onchange={(v) => dmlStore.setCsvMapping(header, v || null)}
        />
        {#if !dmlStore.csvMapping[header]}
          <span class="cg-ignored">ignored</span>
        {/if}
      </div>
    {/each}
    {#if !parsed.headers.length}
      <p class="cg-empty">Nothing to map yet — paste or load a CSV above.</p>
    {/if}
  </div>

  <div class="cg-section">
    <span class="cg-section-title">Preview</span>
    <span class="cg-section-meta">
      {validRows} row{validRows === 1 ? '' : 's'} ready
      {#if invalidRows}
        <span class="cg-bad"><TriangleAlert size={11} /> {invalidRows} rejected</span>
      {/if}
    </span>
  </div>

  {#if invalidRows}
    <ul class="cg-issues">
      {#each [...dmlStore.rowIssues.entries()].slice(0, 5) as [index, messages] (index)}
        <li><strong>Row {index + 1}</strong> — {messages.join(' · ')}</li>
      {/each}
      {#if dmlStore.rowIssues.size > 5}
        <li class="cg-more">…and {dmlStore.rowIssues.size - 5} more.</li>
      {/if}
    </ul>
  {/if}

  <div class="cg-grid">
    <DataGrid
      columns={previewColumns}
      rows={previewRows}
      rowHeight={22}
      sortable={false}
      ariaLabel="CSV preview"
      emptyMessage="Import the CSV to see the rows it produces."
    />
  </div>

  <div class="cg-actions">
    <span class="cg-hint">
      <FileSpreadsheet size={12} />
      Headers latch onto columns with the same name; everything else is your call.
    </span>
    <span class="cg-spacer"></span>
    <Button variant="primary" size="sm" onclick={() => dmlStore.parseCsvSource()}>
      Import and generate
    </Button>
  </div>
</div>

<style>
  .cg { display: flex; flex-direction: column; gap: 10px; }

  .cg-meta { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .cg-spacer { flex: 1; }

  .cg-text {
    width: 100%;
    min-height: 120px;
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
  .cg-text:focus { border-color: var(--border-focus); }

  .cg-section {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding-top: 4px;
  }
  .cg-section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .cg-section-meta {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-disabled);
  }
  .cg-bad {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--error);
  }

  .cg-map { display: flex; flex-direction: column; gap: 5px; }
  .cg-map-row { display: flex; align-items: center; gap: 9px; }
  .cg-header {
    width: 190px;
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cg-arrow { color: var(--text-disabled); flex-shrink: 0; }
  .cg-ignored { font-size: 11px; color: var(--text-disabled); font-style: italic; }
  .cg-empty { font-size: 11.5px; color: var(--text-disabled); font-style: italic; }

  .cg-issues {
    margin: 0;
    padding: 8px 12px 8px 26px;
    background: var(--error-subtle);
    border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    border-radius: var(--radius-md);
    font-size: 11.5px;
    line-height: 1.6;
    color: var(--text-secondary);
  }
  .cg-more { color: var(--text-disabled); }

  .cg-grid {
    display: flex;
    height: 200px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .cg-actions { display: flex; align-items: center; gap: 10px; }
  .cg-hint {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    color: var(--text-muted);
  }
</style>
