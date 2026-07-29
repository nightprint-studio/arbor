<script lang="ts">
  /**
   * What the pattern caught, as a grid — and it is a grid on purpose.
   *
   * One row per match, one **column per placeholder**. Which means a pattern with
   * no replacement is already a query over the repository: `INSERT INTO
   * LOCALSTRINGS ($cols...$) VALUES ($vals...$)` run over four hundred scripts
   * gives back every row those scripts install, with its columns and its values in
   * their own columns.
   *
   * ## Why `DataGrid` and not a table of its own
   *
   * A real repository answers this with ten thousand matches. A `<table>` with ten
   * thousand rows in the DOM is a frozen window, and the first version of this was
   * exactly that. `DataGrid` is the same widget the query results use — windowed,
   * sortable, filterable per column — which is right twice over: it survives the
   * row count, and the same kind of answer looks the same everywhere in Picus.
   */
  import DataGrid, { type DataGridColumn, type DataGridValue } from '$lib/components/shared/ui/DataGrid.svelte';
  import type { FoundMatch } from '$lib/ipc/picus/restructure';

  interface Props {
    matches: FoundMatch[];
    placeholders: string[];
    /** Shown when a replacement is being composed. */
    showReplacement: boolean;
    onOpen: (match: FoundMatch) => void;
  }

  let { matches, placeholders, showReplacement, onOpen }: Props = $props();

  const columns = $derived<DataGridColumn[]>([
    { id: 'file', label: 'File', width: 220 },
    { id: 'line', label: 'Line', type: 'number', width: 64 },
    ...placeholders.map((name) => ({ id: `c:${name}`, label: `$${name}$`, width: 240 })),
    { id: 'matched', label: 'Matched', width: 320 },
    ...(showReplacement ? [{ id: 'becomes', label: 'Becomes', width: 320 }] : []),
  ]);

  const rows = $derived<DataGridValue[][]>(
    matches.map((m) => [
      // The name, not the path: the column would otherwise be a column of one
      // repeated prefix. The full path is the row's tooltip, in the view above.
      m.path.split('/').pop() ?? m.path,
      m.line,
      ...placeholders.map((name) => m.captures[name] ?? ''),
      m.text,
      ...(showReplacement ? [m.problem ? `⚠ ${m.problem}` : (m.replacement ?? '')] : []),
    ]),
  );
</script>

<DataGrid
  {columns}
  {rows}
  sortable
  filterable
  resizable
  showRowNumbers={false}
  ariaLabel="Structural matches"
  emptyMessage="Nothing matched. A pattern is structural — the formatting does not have to agree, but the shape does."
  onActivate={(index) => {
    const match = matches[index];
    if (match) onOpen(match);
  }}
/>
