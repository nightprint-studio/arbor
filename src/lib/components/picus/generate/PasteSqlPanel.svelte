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
   *
   * The text is edited in the same `CodeEditor` the query tabs use, bound to the
   * active connection: highlighting, and completion over that database's tables
   * and columns. It was a bare `<textarea>`, which meant the one place in the
   * product where you *type SQL by hand into a form* was the one place with no
   * help at all — and pasting a statement you then have to correct is the normal
   * case, not the exception.
   */
  import { ClipboardPaste, TriangleAlert, FileUp } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { sqlLanguage } from '../picus-sql-language';

  /**
   * Which database completion should offer.
   *
   * The tab's connection when there is one, the sidebar's selection otherwise —
   * the generator is not a query tab, so it often has neither, and then the
   * descriptor is the portable superset with no completion source. Highlighting
   * still works, which is the half that never needs a server.
   */
  const conn = $derived(picusTabsStore.activeConnection ?? connectionsStore.active);
  const language = $derived(sqlLanguage(conn?.dialect, conn?.id));

  /** Read the paste without reaching for the mouse — the same key that runs a query. */
  const keys = [
    {
      key: 'Mod-Enter',
      preventDefault: true,
      run: () => { dmlStore.parsePaste(); return true; },
    },
  ];

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
  <!-- Keyed on the descriptor, as in the query tabs: the extension set is built
       once at mount, so rebinding to another database has to rebuild it or
       completion keeps offering the previous connection's tables. -->
  <div class="ps-editor">
    {#key language}
      <CodeEditor
        value={dmlStore.pasteText}
        {language}
        keyBindings={keys}
        oninput={(v) => dmlStore.setPasteText(v)}
      />
    {/key}
  </div>

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
      The table, the columns and the values are read out of the statements and re-emitted
      per destination — the pasted text itself is never copied across, and there is no
      table to pick.
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
    <Button
      variant="primary"
      size="sm"
      tooltip={{ content: 'Read the statements into rows', shortcut: 'Ctrl+Enter' }}
      onclick={() => dmlStore.parsePaste()}
    >
      Read and generate
    </Button>
  </div>
</div>

<style>
  .ps { display: flex; flex-direction: column; gap: 10px; }

  /* A fixed box rather than a resizable one: the editor manages its own scroll,
     and the grid of read rows below it is the thing that grows. */
  .ps-editor {
    display: flex;
    height: 190px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .ps-editor > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  .ps-errors {
    padding: 9px 12px;
    background: var(--error-subtle);
    border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    border-radius: var(--radius-md);
    font-size: var(--font-size-xs);
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
    font-size: var(--font-size-2xs);
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
    font-size: var(--font-size-xs);
    line-height: 1.4;
    color: var(--text-muted);
  }
  .ps-spacer { flex: 1; }
</style>
