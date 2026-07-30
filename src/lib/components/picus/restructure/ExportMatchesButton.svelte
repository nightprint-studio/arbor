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
   * What is left in this file is the part only Picus can supply: which columns a
   * match has. The menu, the picker, the writing and the messages are
   * {@link ExportButton}'s, shared with the result grid — six commands written
   * twice is six chances for the two to drift.
   */
  import { FileJson, FileSpreadsheet, FileText } from 'lucide-svelte';
  import ExportButton, {
    type Rendition,
  } from '$lib/components/shared/internal/ExportButton.svelte';
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

  function rendition(
    format: ExportFormat,
    label: string,
    subtitle: string,
    icon: Rendition['icon'],
  ): Rendition {
    return {
      id: format,
      label,
      subtitle,
      icon,
      extension: EXPORT_EXTENSION[format],
      text: () => exportRows(matches, columns, format),
    };
  }

  const renditions = $derived<Rendition[]>([
    rendition('csv', 'As CSV', 'For a spreadsheet', FileSpreadsheet),
    rendition('json', 'As JSON', 'One object per match', FileJson),
    rendition('markdown', 'As a Markdown table', 'For a ticket or a message', FileText),
  ]);
</script>

<ExportButton
  {renditions}
  fileName="picus-matches"
  subject={`${matches.length} match${matches.length === 1 ? '' : 'es'}`}
  empty={!matches.length}
  tooltip="Take these matches out — a pattern is a query over the repository, and its answer is a table"
/>
