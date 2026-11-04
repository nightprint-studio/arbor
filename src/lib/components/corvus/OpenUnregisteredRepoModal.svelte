<script lang="ts">
  /**
   * "This repository is not registered. Add it?" — shown when something else in the suite (Bennu, the File
   * Explorer, a link) sends Corvus to a repository it does not know. Nothing is added to a workspace
   * silently: the user picks a workspace (the active one by default) or just opens the repository without
   * adding it anywhere. Esc cancels; Ctrl+Enter confirms.
   */
  import { FolderPlus } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { openRepoPrompt } from '$lib/stores/corvus/openRepoPrompt.svelte';
  import { workspacesStore } from '$lib/stores/corvus/workspaces.svelte';
  import { choiceFromValue, choiceValue, defaultChoice, ONLY_VALUE } from '$lib/utils/repo-target';

  let { path, name }: { path: string; name: string } = $props();

  let value = $state(choiceValue(defaultChoice(workspacesStore.activeId, workspacesStore.workspaces)));

  const options = $derived([
    ...workspacesStore.workspaces.map((w) => ({ value: w.id, label: `Add to “${w.name}”${w.id === workspacesStore.activeId ? ' (active)' : ''}` })),
    { value: ONLY_VALUE, label: 'Just open it — don’t add it to a workspace' },
  ]);

  const confirm = () => openRepoPrompt.answer(choiceFromValue(value));
  const cancel = () => openRepoPrompt.answer(null);

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); confirm(); }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Modal onClose={cancel} width="520px" height="auto" ariaLabel="Repository not registered">
  {#snippet header()}
    <ModalHeader onClose={cancel}>
      <FolderPlus size={14} class="header-icon" />
      <span class="modal-title">Repository not registered</span>
    </ModalHeader>
  {/snippet}

  <p class="lead"><strong>{name}</strong> is not one of Corvus’s repositories. Add it?</p>
  <p class="path" title={path}>{path}</p>
  <FormField label="Where">
    <Select {value} {options} fill ariaLabel="Workspace to add the repository to" onchange={(v) => (value = String(v))} />
  </FormField>

  {#snippet footer()}
    <ModalFooter hint="Ctrl+Enter to confirm · Esc to cancel">
      <Button variant="ghost" onclick={cancel}>Cancel</Button>
      <Button variant="primary" onclick={confirm}>{value === ONLY_VALUE ? 'Open' : 'Add & open'}</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .lead { margin: 0 0 4px; font-size: var(--font-size-sm); }
  .path { margin: 0 0 12px; font: var(--font-size-xs) var(--font-code); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
