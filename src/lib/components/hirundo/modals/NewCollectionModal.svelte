<script lang="ts">
  /**
   * NewCollectionModal — a collection (or, with `parent`, a folder inside one) is a folder of requests in the
   * project's `.hirundo/collections`. Say what it is called; the modal shows the folder that will be made and
   * refuses a name that a sibling already has.
   */
  import { onMount } from 'svelte';
  import { FolderPlus, Library } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { Node } from '$lib/types/hirundo';

  interface Props {
    /** The collection or folder it goes in; without it, a new collection. */
    parent?: string;
    onCreate: (name: string) => void;
    onClose: () => void;
  }

  let { parent, onCreate, onClose }: Props = $props();

  let name = $state('');
  let el = $state<HTMLInputElement | undefined>(undefined);
  onMount(() => el?.focus());

  const kind = $derived(parent ? 'folder' : 'collection');
  const slug = $derived(name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, ''));

  /** The names already taken where this one would go. */
  const siblings = $derived.by<string[]>(() => {
    if (!parent) return hirundoStore.tree.map((c) => c.name);
    const walk = (nodes: Node[], at: string): Node[] | null => {
      for (const n of nodes) {
        if (n.kind !== 'folder') continue;
        if (n.path === at) return n.children;
        const hit = walk(n.children, at);
        if (hit) return hit;
      }
      return null;
    };
    const top = hirundoStore.tree.find((c) => c.path === parent);
    return (top ? top.children : walk(hirundoStore.tree.flatMap((c) => c.children), parent) ?? []).filter((n) => n.kind === 'folder').map((n) => n.name);
  });

  const error = $derived(!name.trim() ? '' : siblings.some((s) => s.toLowerCase() === name.trim().toLowerCase()) ? `There is already a ${kind} called “${name.trim()}” here.` : !slug ? 'Use letters or digits in the name.' : '');
  const valid = $derived(!!name.trim() && !error);

  function submit() {
    if (valid) onCreate(name.trim());
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey || document.activeElement === el)) { e.preventDefault(); submit(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="520px" height="360px" ariaLabel={`New ${kind}`}>
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if parent}<FolderPlus size={14} />{:else}<Library size={14} />{/if}
      <span class="modal-title">New {kind}</span>
    </ModalHeader>
  {/snippet}

  <div class="nc">
    <FormSection label="Name" hint={parent ? `A folder groups requests inside ${parent.split('/').pop()}.` : 'A collection groups the requests of one API; it also carries their shared auth, headers and variables.'} first>
      <FormField label={parent ? 'Folder name' : 'Collection name'}>
        <Input value={name} bind:element={el} placeholder={parent ? 'Orders' : 'Bandi'} {error} oninput={(v) => (name = v)} />
      </FormField>
    </FormSection>
    <FormSection label="What will be created">
      {#if slug}
        <p class="nc-path"><code>.hirundo/collections/{parent ? `${parent}/` : ''}{slug}/</code></p>
        <p class="nc-muted">with its own settings file, where auth, headers and variables are kept.</p>
      {:else}
        <p class="nc-muted">The folder that will be made shows here.</p>
      {/if}
    </FormSection>
  </div>

  {#snippet footer()}
    <ModalFooter hint="to create">
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" disabled={!valid} onclick={submit}>Create {kind}</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .nc { display: flex; flex-direction: column; padding: 4px 16px 12px; }
  .nc-path { margin: 0 0 4px; font-size: var(--font-size-sm); word-break: break-all; }
  .nc-path code { font-family: var(--font-mono); }
  .nc-muted { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
