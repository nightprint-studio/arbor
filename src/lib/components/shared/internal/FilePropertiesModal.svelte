<script lang="ts">
  /**
   * FilePropertiesModal — what a file or a folder is: kind, size, dates and where it lives, with the system's own
   * Properties sheet one button away.
   *
   * The same facts Sitta's info panel shows, in a dialog any product can open on a path (Bennu's project tree is
   * the first). The entry comes from listing its parent (one call, already the way every file view reads
   * metadata); a folder's size is counted on request, off the UI thread, because walking a big tree is not
   * something to do just because a menu was opened.
   */
  import { Info, Settings2, Copy as CopyIcon } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import PropertyList, { type PropertyRow } from '$lib/components/shared/ui/PropertyList.svelte';
  import { fsReadDir, fsDirSize, fsShowProperties, type FsEntry, type DirSize } from '$lib/ipc/fs';
  import { formatBytes } from '$lib/utils/format';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  let { path, onClose }: { path: string; onClose: () => void } = $props();

  let entry = $state<FsEntry | null>(null);
  let error = $state<string | null>(null);
  let size = $state<DirSize | null>(null);
  let counting = $state(false);

  const parentOf = (p: string) => p.replace(/[\\/]+$/, '').replace(/[\\/][^\\/]*$/, '') || p;
  const nameOf = (p: string) => p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? p;

  $effect(() => {
    const target = path;
    entry = null; error = null; size = null;
    void (async () => {
      try {
        const siblings = await fsReadDir(parentOf(target), true);
        const name = nameOf(target);
        const found = siblings.find((e) => e.path === target || e.name === name) ?? null;
        if (path !== target) return;
        if (!found) error = 'This path is no longer there.';
        else entry = found;
      } catch (e) {
        if (path === target) error = String(e);
      }
    })();
  });

  async function count() {
    if (!entry || counting) return;
    counting = true;
    try { size = await fsDirSize(entry.path); }
    catch (e) { toastStore.show(`${e}`, 'error'); }
    finally { counting = false; }
  }

  const when = (ms: number | null) =>
    ms == null ? '' : new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });

  const rows = $derived<PropertyRow[]>(entry ? [
    { label: 'Name', value: entry.name },
    { label: 'Kind', value: entry.is_dir ? 'Folder' : kindOf(entry.name) },
    { label: 'Size', custom: 'size' },
    { label: 'Modified', value: when(entry.modified), hidden: entry.modified == null },
    { label: 'Created', value: when(entry.created), hidden: entry.created == null },
    { label: 'Location', value: parentOf(entry.path), wrap: true },
  ] : []);

  function kindOf(name: string): string {
    const dot = name.lastIndexOf('.');
    return dot > 0 && dot < name.length - 1 ? `${name.slice(dot + 1).toUpperCase()} file` : 'File';
  }

  async function copyPath() {
    try { await navigator.clipboard.writeText(path); toastStore.show('Path copied', 'info'); }
    catch (e) { toastStore.show(`${e}`, 'error'); }
  }
</script>

<Modal {onClose} width="460px" ariaLabel="Properties">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Info size={14} />
      <span class="modal-title">Properties</span>
    </ModalHeader>
  {/snippet}

  {#if error}
    <Alert variant="error" compact text={error} />
  {:else if !entry}
    <span class="loading"><Spinner size="sm" /> Reading…</span>
  {:else}
    <PropertyList {rows}>
      {#snippet custom(key)}
        {#if key === 'size'}
          {#if !entry!.is_dir}
            {entry!.size != null ? `${formatBytes(entry!.size)} (${entry!.size.toLocaleString('en-US')} bytes)` : '—'}
          {:else if size}
            {formatBytes(size.bytes)} · {size.files.toLocaleString('en-US')} files, {size.dirs.toLocaleString('en-US')} folders
          {:else if counting}
            <Spinner size="xs" /> Counting…
          {:else}
            <Button size="xs" variant="ghost" onclick={count}>Calculate</Button>
          {/if}
        {/if}
      {/snippet}
    </PropertyList>
  {/if}

  {#snippet footer()}
    <Button size="sm" variant="ghost" onclick={copyPath}>
      {#snippet iconStart()}<CopyIcon size={13} />{/snippet}
      Copy path
    </Button>
    <Button size="sm" variant="ghost" onclick={() => fsShowProperties(path).catch((e) => toastStore.show(`${e}`, 'error'))}>
      {#snippet iconStart()}<Settings2 size={13} />{/snippet}
      System properties…
    </Button>
    <Button size="sm" variant="primary" onclick={onClose}>Close</Button>
  {/snippet}
</Modal>

<style>
  .loading { display: inline-flex; align-items: center; gap: 8px; color: var(--text-secondary); font-size: var(--font-size-sm); }
</style>
