<script lang="ts">
  /**
   * BennuReinterpretModal — read the open file as a different encoding.
   *
   * Changes how the file is READ, never what is on disk: the right move when accents show up as
   * `Ã©` or `�` because the file was decoded with the wrong rule. To REWRITE the bytes in another
   * encoding use "Convert encoding…" instead.
   *
   * Keyboard-first: the picker is focused on open; Enter / Ctrl+Enter apply; Esc cancels.
   */
  import { untrack } from 'svelte';
  import { FileSearch } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { BENNU_ENCODINGS, canonicalEncoding } from './encodings';
  import { fileEncodingOverride, reinterpretFile } from './reinterpret';

  let { onClose }: { onClose: () => void } = $props();

  const AUTO = '__auto__';
  const path = $derived(projectStore.activeFilePath);
  const name = $derived(path?.split('/').pop() ?? '');
  const current = $derived(projectStore.activeEncoding ?? 'UTF-8');
  const pinned = $derived(path ? fileEncodingOverride(path) : undefined);

  // Starts on what the file is read as now, so Enter without touching anything changes nothing.
  let choice = $state(untrack(() => (pinned ? (canonicalEncoding(pinned) ?? AUTO) : AUTO)));
  let busy = $state(false);

  // The pin lives in the config; make sure it has been read before the picker shows it.
  void bennuConfigStore.load();

  const options = $derived([
    { value: AUTO, label: `As the project reads it (${projectStore.project?.source_encoding ?? 'UTF-8'})` },
    ...BENNU_ENCODINGS.map((e) => ({ value: e.value, label: e.label })),
  ]);

  async function apply() {
    if (!path || busy) return;
    busy = true;
    try {
      await bennuConfigStore.load();
      await reinterpretFile(path, choice === AUTO ? null : choice);
      toastStore.show(
        choice === AUTO ? `${name} is read as the project reads it` : `${name} is now read as ${choice}`,
        'success',
      );
      onClose();
    } catch (e) {
      toastStore.show(`Could not reinterpret ${name}: ${e}`, 'error');
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.defaultPrevented) {
      e.preventDefault();
      void apply();
    }
  }
</script>

<Modal {onClose} width="520px" height="360px" ariaLabel="Reinterpret file encoding">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FileSearch size={14} />
      <span class="modal-title">Reinterpret as…</span>
    </ModalHeader>
  {/snippet}

  <div class="body" onkeydown={onKey} role="presentation">
    {#if path}
      <p class="lead">
        <code>{name}</code> is read as <strong>{current}</strong>{pinned ? ' (pinned for this file)' : ''}.
      </p>
      <FormField label="Read it as">
        <Select bind:value={choice} {options} fill searchable ariaLabel="Encoding to read the file with" />
      </FormField>
      <Alert variant="info" compact
        text="Only how the file is read changes — the file on disk is not touched. Use “Convert encoding…” to rewrite it." />
    {:else}
      <Alert variant="warning" text="Open a file first." />
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end" hint="to apply">
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" onclick={apply} disabled={!path} loading={busy}>Reinterpret</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .lead {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
  }
  .lead code {
    font-family: var(--font-code);
    color: var(--text-primary);
  }
</style>
