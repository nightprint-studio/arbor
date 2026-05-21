<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { Check, Plus, FolderPlus } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { WS_COLOR_COUNT, workspaceColorVar } from '$lib/types/workspace';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** When set, the form edits that group.  Otherwise creates a new one. */
    editGroupId?: string | null;
    onClose: () => void;
  }
  let { editGroupId = null, onClose }: Props = $props();

  const editing = $derived(editGroupId
    ? (workspacesStore.groups.find(g => g.id === editGroupId) ?? null)
    : null);

  let name     = $state('');
  let colorIdx = $state(0);
  let nameInput: HTMLInputElement | undefined;
  let saving   = $state(false);

  onMount(async () => {
    if (editing) {
      name     = editing.name;
      colorIdx = editing.color_idx;
    } else {
      colorIdx = workspacesStore.groups.length % WS_COLOR_COUNT;
    }
    await tick();
    nameInput?.focus();
    nameInput?.select();
  });

  async function save() {
    const trimmed = name.trim();
    if (!trimmed) { nameInput?.focus(); return; }
    saving = true;
    try {
      if (editing) {
        await workspacesStore.updateGroup(editing.id, { name: trimmed, color_idx: colorIdx });
        uiStore.showToast(`Group "${trimmed}" updated`, 'success');
      } else {
        await workspacesStore.createGroup(trimmed, colorIdx);
        uiStore.showToast(`Group "${trimmed}" created`, 'success');
      }
      onClose();
    } catch (e) {
      uiStore.showToast(`Failed: ${e}`, 'error');
    } finally {
      saving = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); void save(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} ariaLabel={editing ? `Edit group ${editing.name}` : 'New group'}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <FolderPlus size={16} />
      <span class="modal-title">{editing ? 'Edit Group' : 'New Group'}</span>
    </ModalHeader>
  {/snippet}

  <div class="body">
    <div class="field">
      <label for="group-name">Name</label>
      <div class="name-row">
        <Monogram name={name || '?'} color={workspaceColorVar(colorIdx)} size={22} />
        <input
          id="group-name"
          bind:this={nameInput}
          bind:value={name}
          maxlength="40"
          placeholder="e.g. Clients"
          autocomplete="off"
        />
      </div>
    </div>

    <div class="field">
      <span class="label">Colour</span>
      <div class="swatches">
        {#each Array.from({ length: WS_COLOR_COUNT }, (_, i) => i) as i}
          <button
            class="swatch"
            class:selected={colorIdx === i}
            style="background: {workspaceColorVar(i)};"
            onclick={() => colorIdx = i}
            use:tooltip={`Colour ${i + 1}`}
            aria-label="Choose colour {i + 1}"
            aria-pressed={colorIdx === i}
            type="button"
          >
            {#if colorIdx === i}<Check size={12} />{/if}
          </button>
        {/each}
      </div>
    </div>
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose} type="button">Cancel</Button>
    <Button variant="primary" onclick={save} disabled={!name.trim() || saving} loading={saving} type="button">
      {#snippet iconStart()}
        {#if editing}<Check size={13} />{:else}<Plus size={13} />{/if}
      {/snippet}
      {editing ? 'Save' : 'Create'}
    </Button>
  {/snippet}
</Modal>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    font-family: var(--font-ui-sans);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 12px 14px;
  }
  .field label,
  .field .label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    font-weight: 600;
  }

  .name-row { display: flex; align-items: center; gap: 10px; }
  input {
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
  }
  input:focus { outline: none; border-color: var(--accent); }

  .swatches {
    display: grid;
    grid-template-columns: repeat(12, 1fr);
    gap: 6px;
  }
  .swatch {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 26px;
    border: 1px solid color-mix(in srgb, currentColor 30%, transparent);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--ws-color-fg);
    transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  }
  .swatch:hover { transform: scale(1.08); }
  .swatch.selected {
    box-shadow: 0 0 0 2px var(--bg-elevated), 0 0 0 4px currentColor;
  }

</style>
