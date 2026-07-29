<script lang="ts">
  /**
   * Take the matches out of Picus.
   *
   * A structural pattern is a **query over the repository** as much as it is the
   * first half of a rewrite — "every row these scripts install, with its columns
   * and its values" is a question people ask far more often than they ask for a
   * migration. Its answer is a table, and a table you cannot get out of the tool
   * is a table you end up retyping.
   *
   * Three formats, three destinations: CSV for a spreadsheet, JSON for a script,
   * Markdown for a ticket. To the clipboard, or to a file — through Arbor's own
   * picker, never a native dialog, and never without the user naming the file.
   */
  import { Download, Copy, FileJson, FileSpreadsheet, FileText } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { fsWriteTextFile } from '$lib/ipc/fs';
  import {
    exportRows,
    EXPORT_EXTENSION,
    type ExportColumn,
    type ExportFormat,
  } from '$lib/utils/tabular-export';
  import type { FoundMatch } from '$lib/ipc/picus/restructure';

  interface Props {
    matches: FoundMatch[];
    placeholders: string[];
    /** Included as a column only when a template is being composed. */
    showReplacement: boolean;
  }

  let { matches, placeholders, showReplacement }: Props = $props();

  /** Set while the save picker is up; carries which format was asked for. */
  let saving = $state<ExportFormat | null>(null);

  /**
   * The columns, in the order the table shows them.
   *
   * Built here rather than taken from the table component: the two must agree, and
   * the honest way to make them agree is for both to be a function of the
   * placeholders — not for one to read the other's DOM.
   */
  const columns = $derived<ExportColumn<FoundMatch>[]>([
    { key: 'file', value: (m) => m.path },
    { key: 'line', value: (m) => m.line },
    ...placeholders.map((name) => ({
      key: name,
      value: (m: FoundMatch) => m.captures[name] ?? '',
    })),
    { key: 'matched', value: (m) => m.text },
    ...(showReplacement
      ? [{ key: 'becomes', value: (m: FoundMatch) => m.replacement ?? m.problem ?? '' }]
      : []),
  ]);

  function text(format: ExportFormat): string {
    return exportRows(matches, columns, format);
  }

  async function copy(format: ExportFormat) {
    try {
      await navigator.clipboard.writeText(text(format));
      toastStore.show(
        `${matches.length} match${matches.length === 1 ? '' : 'es'} copied as ${format.toUpperCase()}.`,
        'success',
      );
    } catch (e) {
      toastStore.show(`Nothing was copied — ${e}`, 'error');
    }
  }

  async function save(path: string) {
    const format = saving;
    saving = null;
    if (!format) return;
    try {
      await fsWriteTextFile(path, text(format));
      toastStore.show(`${matches.length} written to ${path.split(/[\\/]/).pop()}.`, 'success');
    } catch (e) {
      toastStore.show(`${path} could not be written — ${e}`, 'error');
    }
  }

  const items = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Copy' },
    {
      kind: 'item',
      id: 'copy-csv',
      label: 'As CSV',
      subtitle: 'For a spreadsheet',
      icon: FileSpreadsheet,
      onclick: () => void copy('csv'),
    },
    {
      kind: 'item',
      id: 'copy-json',
      label: 'As JSON',
      subtitle: 'One object per match',
      icon: FileJson,
      onclick: () => void copy('json'),
    },
    {
      kind: 'item',
      id: 'copy-md',
      label: 'As a Markdown table',
      subtitle: 'For a ticket or a message',
      icon: FileText,
      onclick: () => void copy('markdown'),
    },
    { kind: 'separator', label: 'Save to a file' },
    { kind: 'item', id: 'save-csv', label: 'CSV…', icon: FileSpreadsheet, onclick: () => (saving = 'csv') },
    { kind: 'item', id: 'save-json', label: 'JSON…', icon: FileJson, onclick: () => (saving = 'json') },
    { kind: 'item', id: 'save-md', label: 'Markdown…', icon: FileText, onclick: () => (saving = 'markdown') },
  ]);
</script>

<Dropdown {items} position="fixed" width="260px">
  {#snippet trigger({ open, toggle })}
    <Button
      variant="secondary"
      size="xs"
      disabled={!matches.length}
      ariaExpanded={open}
      ariaLabel="Export the matches"
      tooltip={matches.length
        ? 'Take these matches out — a pattern is a query over the repository, and its answer is a table'
        : { content: 'There is nothing to export yet' }}
      onclick={toggle}
    >
      {#snippet iconStart()}<Download size={13} />{/snippet}
      Export
    </Button>
  {/snippet}
</Dropdown>

{#if saving}
  <FileExplorerModal
    mode="save"
    title={`Save the matches as ${saving.toUpperCase()}`}
    initialFilename={`picus-matches.${EXPORT_EXTENSION[saving]}`}
    extensions={[EXPORT_EXTENSION[saving]]}
    onConfirm={(path) => void save(String(path))}
    onClose={() => (saving = null)}
  />
{/if}
