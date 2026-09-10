<script lang="ts">
  /**
   * A spreadsheet, in an editor tab.
   *
   * ## Why a grid and not a rendering
   *
   * The `.docx` viewer beside this one renders *pages*, because the reason you open a document
   * from a project tree is to check the document. A spreadsheet is the opposite: nobody opens one
   * from a source tree to see its borders and its conditional fills — they open it to read the
   * column mapping, the codes, the translations. So this draws the values, in a grid, with the
   * sheet tabs it came with, and takes no position on how Excel would have painted them.
   *
   * ## Read-only, and it says so
   *
   * Nothing here edits. The file never enters the source cache (see `opensAsPreview`), so there is
   * no buffer for a stray Ctrl+S to write back over the spreadsheet.
   *
   * ## What it will not pretend
   *
   * A truncated sheet says it was truncated, and an unsupported format says which one it is
   * instead of opening an empty grid. Both come from the backend — see `bennu-sheet`.
   */
  import { Table2, ExternalLink, Lock, TriangleAlert } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { baseName } from '$lib/utils/paths';
  import { tooltip } from '$lib/actions/tooltip';
  import { readSheet, type Workbook } from '$lib/ipc/bennu/sheet';

  let { path }: { path: string } = $props();

  let book = $state<Workbook | null>(null);
  let loading = $state(true);
  let error = $state('');
  let active = $state(0);

  // Re-read when the tab changes. The `path` is the dependency and the only one: a spreadsheet in
  // a project tree is not being edited under us, and a viewer that polled would be spending a file
  // read per second on a file nobody is touching.
  $effect(() => {
    const file = path;
    let alive = true;
    loading = true;
    error = '';
    book = null;
    active = 0;
    void readSheet(file)
      .then((w) => {
        if (!alive) return;
        book = w;
      })
      .catch((e) => {
        if (!alive) return;
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (alive) loading = false;
      });
    return () => {
      alive = false;
    };
  });

  const sheet = $derived(book?.sheets[active] ?? null);

  /** `A`, `B`, … `AA` — the header a spreadsheet has, so a cell can be named out loud. */
  function columnName(index: number): string {
    let name = '';
    let n = index;
    do {
      name = String.fromCharCode(65 + (n % 26)) + name;
      n = Math.floor(n / 26) - 1;
    } while (n >= 0);
    return name;
  }
</script>

<div class="sv">
  <header class="sv-head">
    <span class="sv-icon"><Table2 size={13} /></span>
    <span class="sv-name">{baseName(path)}</span>
    {#if book}
      <span class="sv-meta">{book.format}</span>
    {/if}
    <span class="sv-spacer"></span>
    <span class="sv-ro" use:tooltip={'Spreadsheets open read-only — nothing here writes to the file'}>
      <Lock size={11} /> read-only
    </span>
    <button
      type="button"
      class="sv-open"
      onclick={() => void openPath(path).catch(() => {})}
      use:tooltip={'Open in the system spreadsheet application'}
    >
      <ExternalLink size={12} /> Open externally
    </button>
  </header>

  {#if loading}
    <div class="sv-loading"><Spinner size={16} /><span>Reading the workbook…</span></div>
  {:else if error}
    <div class="sv-notice">
      <Alert variant="warning" compact text={error} />
    </div>
  {:else if !sheet}
    <EmptyState message="This workbook has no sheets to show." />
  {:else}
    {#if book && book.sheets.length > 1}
      <!-- The sheet tabs, where a spreadsheet puts them. -->
      <nav class="sv-tabs">
        {#each book.sheets as s, i (s.name + i)}
          <button
            type="button"
            class="sv-tab"
            class:sv-tab-on={i === active}
            onclick={() => (active = i)}
          >{s.name}</button>
        {/each}
      </nav>
    {/if}

    {#if sheet.truncated}
      <div class="sv-notice">
        <Alert variant="info" compact>
          <TriangleAlert size={11} /> This sheet is longer or wider than a viewer shows — what is
          here is the beginning of it, not all of it.
        </Alert>
      </div>
    {/if}

    <div class="sv-grid-wrap">
      <table class="sv-grid">
        <thead>
          <tr>
            <th class="sv-corner"></th>
            {#each Array(sheet.columns) as _, c (c)}
              <th class="sv-col">{columnName(c)}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each sheet.rows as row, r (r)}
            <tr>
              <th class="sv-row">{r + 1}</th>
              {#each row as cell, c (c)}
                <td class="sv-cell sv-{cell.kind}" title={cell.text}>{cell.text}</td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .sv { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-base); }
  .sv-head {
    display: flex; align-items: center; gap: 6px;
    padding: 5px 8px; border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .sv-icon { color: var(--success); display: flex; }
  .sv-name { color: var(--text-primary); font-weight: 600; }
  .sv-meta { color: var(--text-disabled); font-size: var(--font-size-2xs); text-transform: uppercase; }
  .sv-spacer { flex: 1; }
  .sv-ro { display: inline-flex; align-items: center; gap: 3px; color: var(--text-disabled); font-size: var(--font-size-2xs); }
  .sv-open {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 3px 7px; border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    background: transparent; color: var(--text-secondary);
    font-size: var(--font-size-2xs); cursor: pointer;
  }
  .sv-open:hover { color: var(--text-primary); border-color: var(--border-default); }

  .sv-tabs { display: flex; gap: 2px; padding: 4px 6px 0; overflow-x: auto; }
  .sv-tab {
    padding: 3px 10px; border: 1px solid transparent; border-bottom: none;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    background: transparent; color: var(--text-secondary);
    font-size: var(--font-size-2xs); white-space: nowrap; cursor: pointer;
  }
  .sv-tab:hover { color: var(--text-primary); }
  .sv-tab-on {
    background: var(--bg-elevated); border-color: var(--border-subtle);
    color: var(--text-primary); font-weight: 600;
  }

  .sv-notice { padding: 6px 8px 2px; }
  .sv-loading {
    display: flex; align-items: center; gap: 8px;
    padding: 14px 12px; color: var(--text-muted); font-size: var(--font-size-xs);
  }

  /* The grid scrolls in its own box: a spreadsheet is wider than any panel, and the page must
     never scroll sideways with it. */
  .sv-grid-wrap { flex: 1; min-height: 0; overflow: auto; }
  .sv-grid { border-collapse: separate; border-spacing: 0; font-size: var(--font-size-2xs); }
  .sv-grid th, .sv-grid td {
    border-right: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
    padding: 2px 6px; white-space: nowrap;
    max-width: 320px; overflow: hidden; text-overflow: ellipsis;
  }
  /* The headers stay put — a spreadsheet without its column letters is a wall of values. */
  .sv-grid thead th {
    position: sticky; top: 0; z-index: 2;
    background: var(--bg-elevated); color: var(--text-muted);
    font-weight: 600; text-align: center;
  }
  .sv-row, .sv-corner {
    position: sticky; left: 0; z-index: 1;
    background: var(--bg-elevated); color: var(--text-muted);
    font-weight: 500; text-align: right;
  }
  .sv-corner { z-index: 3; }
  .sv-cell { color: var(--text-primary); }
  /* Alignment is information: a column that lines up on the right is numeric, and a value that
     does not belong in it shows itself by sitting the other way. */
  .sv-number, .sv-date { text-align: right; font-variant-numeric: tabular-nums; }
  .sv-bool { text-align: center; color: var(--text-secondary); }
  .sv-error { color: var(--accent-danger, #e5534b); }
  .sv-empty { background: transparent; }
</style>
