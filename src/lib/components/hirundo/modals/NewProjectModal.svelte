<script lang="ts">
  /**
   * NewProjectModal — a Hirundo project is a folder with a `.hirundo/` folder inside it: the collections,
   * environments and tests of the requests you make for that code, kept as readable files next to it (so they can
   * be committed and reviewed). Usually the project is the root of a repository.
   *
   * Pick where it lives, and optionally give it a name: with a name, a new folder of that name is made inside the
   * location; without one, the location itself becomes the project. It opens at once and joins the active workspace.
   */
  import { onMount } from 'svelte';
  import { FolderPlus, FolderSearch } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let location = $state('');
  let name = $state('');
  let browsing = $state(false);
  let creating = $state(false);
  let error = $state('');
  let nameEl = $state<HTMLInputElement | undefined>(undefined);

  onMount(() => nameEl?.focus());

  const sep = $derived(location.includes('\\') ? '\\' : '/');
  const target = $derived(!location ? '' : name.trim() ? `${location.replace(/[\\/]+$/, '')}${sep}${name.trim()}` : location);
  const nameError = $derived(/[\\/]/.test(name) ? 'A name cannot contain / or \\ — pick the folder with Browse instead.' : '');
  const valid = $derived(!!location.trim() && !nameError);

  async function create() {
    if (!valid || creating) return;
    creating = true;
    error = '';
    const path = await hirundoStore.createProject(location, name);
    creating = false;
    if (path) onClose();
    else error = 'The project could not be created. Check that the location exists and can be written to.';
  }

  function onKey(e: KeyboardEvent) {
    if (browsing) return;
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void create(); }
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'b') { e.preventDefault(); browsing = true; }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="600px" height="500px" ariaLabel="New project">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FolderPlus size={14} />
      <span class="modal-title">New project</span>
    </ModalHeader>
  {/snippet}

  <div class="np">
    <FormSection label="Where" hint="The folder it lives in — usually the root of the repository you are testing." first>
      <div class="np-row">
        <div class="np-grow"><Input value={location} placeholder="Choose a folder…" readonly ariaLabel="Location" /></div>
        <Button variant="outline" tooltip={{ content: 'Choose a folder', shortcut: 'Ctrl+B' }} onclick={() => (browsing = true)}>{#snippet iconStart()}<FolderSearch size={13} />{/snippet}Browse…</Button>
      </div>
    </FormSection>
    <FormSection label="Name" hint="Optional. With a name, a new folder of that name is made inside the location; without one, the location itself is the project.">
      <FormField label="Folder name"><Input value={name} bind:element={nameEl} placeholder="my-api-requests" error={nameError} oninput={(v) => (name = v)} /></FormField>
    </FormSection>
    <FormSection label="What will be created">
      {#if target}
        <pre class="np-tree">{target}
└─ .hirundo/
   └─ collections/   (environments and local values appear as you add them)</pre>
      {:else}
        <p class="np-muted">Choose a folder and the project that will be made shows here.</p>
      {/if}
    </FormSection>
    {#if error}<Alert variant="error" compact text={error} />{/if}
  </div>

  {#snippet footer()}
    <ModalFooter hint="to create">
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" loading={creating} disabled={!valid} onclick={() => void create()}>Create project</Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if browsing}
  <FileExplorerModal mode="folder" title="Where should the project live?" onConfirm={(p: string) => { browsing = false; location = p; }} onCancel={() => (browsing = false)} onClose={() => (browsing = false)} />
{/if}

<style>
  .np { display: flex; flex-direction: column; gap: 4px; padding: 4px 16px 12px; }
  .np-row { display: flex; gap: 8px; align-items: center; }
  .np-grow { flex: 1; min-width: 0; }
  .np-tree { margin: 0; padding: 8px 10px; font: var(--font-size-xs) var(--font-mono); color: var(--text-secondary); background: var(--bg-elevated); border-radius: var(--radius-md); white-space: pre-wrap; word-break: break-all; }
  .np-muted { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
