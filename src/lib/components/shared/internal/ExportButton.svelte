<script lang="ts" module>
  import type { IconComponent } from '$lib/types/icon';

  /** One thing the data can be turned into. */
  export interface Rendition {
    id: string;
    /** Shown in the menu — "As CSV", "As a Markdown table". */
    label: string;
    /** One line under it, saying what it is for. */
    subtitle?: string;
    icon?: IconComponent;
    /** Extension the save picker offers, without the dot. */
    extension: string;
    /** Produced only when asked for: a large table should not be rendered three
     *  ways every time the menu opens. */
    text: () => string | Promise<string>;
  }
</script>

<script lang="ts">
  /**
   * "Take this table out of Arbor" — the one implementation.
   *
   * Every grid in this application eventually needs the same six commands: three
   * formats, two destinations. Written per panel that is six chances to forget the
   * clipboard's error path, six spellings of the same toast, and — the one that
   * actually happened — a panel whose export button did nothing at all because it
   * was wired to a placeholder.
   *
   * What a caller supplies is the **renditions**: what the data can become, and how
   * to produce each. Everything else — the menu, the picker, the writing, the
   * messages — is here.
   *
   * Saving always goes through Arbor's own picker, never a native dialog, and never
   * without the user naming the file: nothing is written to disk that the user did
   * not just point at.
   */
  import { Download, Loader } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { fsWriteTextFile } from '$lib/ipc/fs';

  interface Props {
    renditions: Rendition[];
    /** Base file name offered by the picker, without an extension. */
    fileName: string;
    /** What is being exported, for the messages: "4 matches", "1,200 rows". */
    subject: string;
    /** Nothing to export — the button says why rather than disappearing. */
    empty?: boolean;
    label?: string;
    /** Explains the button when there IS something to export. */
    tooltip?: string;
    /** Reason it is disabled, when it is. */
    emptyTooltip?: string;
    size?: 'xs' | 'sm';
    variant?: 'secondary' | 'ghost' | 'icon';
  }

  let {
    renditions,
    fileName,
    subject,
    empty = false,
    label = 'Export',
    tooltip = 'Take this out of Arbor — to the clipboard, or to a file',
    emptyTooltip = 'There is nothing to export yet',
    size = 'xs',
    variant = 'secondary',
  }: Props = $props();

  /** Set while the save picker is up; carries which rendition was asked for. */
  let saving = $state<Rendition | null>(null);
  /** True while a rendition is being produced — some of them ask the backend. */
  let working = $state(false);

  /**
   * Produce a rendition, with the failure surfaced.
   *
   * `null` on failure rather than a throw: both callers below have something
   * sensible to do with "it did not work", and neither wants to unwind.
   */
  async function render(rendition: Rendition): Promise<string | null> {
    working = true;
    try {
      return await rendition.text();
    } catch (e) {
      toastStore.show(`${subject} could not be exported — ${e}`, 'error');
      return null;
    } finally {
      working = false;
    }
  }

  async function copy(rendition: Rendition) {
    const text = await render(rendition);
    if (text === null) return;
    try {
      await navigator.clipboard.writeText(text);
      toastStore.show(`${subject} copied as ${rendition.label.replace(/^As /i, '')}.`, 'success');
    } catch (e) {
      toastStore.show(`Nothing was copied — ${e}`, 'error');
    }
  }

  async function save(path: string) {
    const rendition = saving;
    saving = null;
    if (!rendition) return;
    const text = await render(rendition);
    if (text === null) return;
    try {
      await fsWriteTextFile(path, text);
      toastStore.show(`${subject} written to ${path.split(/[\\/]/).pop()}.`, 'success');
    } catch (e) {
      toastStore.show(`${path} could not be written — ${e}`, 'error');
    }
  }

  const items = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Copy' },
    ...renditions.map((r): DropdownItem => ({
      kind: 'item',
      id: `copy-${r.id}`,
      label: r.label,
      subtitle: r.subtitle,
      icon: r.icon,
      onclick: () => void copy(r),
    })),
    { kind: 'separator', label: 'Save to a file' },
    ...renditions.map((r): DropdownItem => ({
      kind: 'item',
      id: `save-${r.id}`,
      label: `${r.label.replace(/^As (a )?/i, '')}…`,
      icon: r.icon,
      onclick: () => (saving = r),
    })),
  ]);
</script>

<Dropdown {items} position="fixed" width="270px">
  {#snippet trigger({ open, toggle })}
    <Button
      {variant}
      {size}
      disabled={empty || working}
      ariaExpanded={open}
      ariaLabel={`${label} — ${subject}`}
      tooltip={empty ? { content: emptyTooltip } : tooltip}
      onclick={toggle}
    >
      {#snippet iconStart()}
        {#if working}<Spinner size={12} />{:else}<Download size={13} />{/if}
      {/snippet}
      {#if variant !== 'icon'}{label}{/if}
    </Button>
  {/snippet}
</Dropdown>

{#if saving}
  <FileExplorerModal
    mode="save"
    title={`Save ${subject} as ${saving.extension.toUpperCase()}`}
    initialFilename={`${fileName}.${saving.extension}`}
    extensions={[saving.extension]}
    onConfirm={(path) => void save(String(path))}
    onClose={() => (saving = null)}
  />
{/if}
