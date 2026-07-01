<script lang="ts">
  /**
   * Profile manager — create / clone / rename / delete / switch the isolated
   * environments under `arbor/profiles/`. Opened from the title-bar settings
   * (gear) menu. Switching reloads the window with the chosen profile's
   * settings, plugins and repos. See docs/profiles-and-product-config.md.
   */
  import { Plus, Trash2, Pencil, Copy, Check, X as XIcon } from 'lucide-svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ConfirmModal from './ConfirmModal.svelte';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';
  import { profileStore } from '$lib/stores/profiles.svelte';

  interface Props { onClose: () => void; }
  let { onClose }: Props = $props();

  const DEFAULT = 'default';

  let newName    = $state('');
  let creating   = $state(false);
  let error      = $state<string | null>(null);

  // A single inline-edit row drives both rename and clone: `editing` holds the
  // source profile name, `editMode` picks the verb. Keeping one row (instead of
  // two ad-hoc inputs) means the keyboard flow — type name → Enter to commit,
  // Esc to cancel — is identical for both actions.
  type EditMode = 'rename' | 'clone';
  let editing    = $state<string | null>(null);
  let editMode   = $state<EditMode>('rename');
  let draft      = $state('');
  let busy       = $state(false);

  let pendingDelete = $state<string | null>(null);

  const nameTaken = (name: string) =>
    profileStore.list.some((p) => p.toLowerCase() === name.toLowerCase());

  async function doCreate() {
    const name = newName.trim();
    if (!name || creating) return;
    if (nameTaken(name)) { error = `A profile named “${name}” already exists.`; return; }
    creating = true; error = null;
    try {
      await profileStore.create(name);
      newName = '';
    } catch (e) {
      error = String(e);
    } finally {
      creating = false;
    }
  }

  function startRename(name: string) {
    editing = name; editMode = 'rename'; draft = name; error = null;
  }
  function startClone(name: string) {
    editing = name; editMode = 'clone'; draft = `${name} copy`; error = null;
  }
  function cancelEdit() { editing = null; draft = ''; error = null; }

  async function commitEdit() {
    const src  = editing;
    const next = draft.trim();
    if (!src) return;
    if (editMode === 'rename') {
      if (!next || next === src) { cancelEdit(); return; }
    } else if (!next) {
      return;
    }
    if (next !== src && nameTaken(next)) {
      error = `A profile named “${next}” already exists.`;
      return;
    }
    busy = true; error = null;
    try {
      if (editMode === 'rename') await profileStore.rename(src, next);
      else                       await profileStore.clone(src, next);
      cancelEdit();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function confirmDelete() {
    if (!pendingDelete) return;
    busy = true; error = null;
    try {
      await profileStore.remove(pendingDelete);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
      pendingDelete = null;
    }
  }

  async function doSwitch(name: string) {
    error = null;
    try {
      await profileStore.switchTo(name);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<Modal {onClose} width="520px" ariaLabel="Profiles">
  {#snippet header()}
    <ModalHeader title="Profiles" {onClose} />
  {/snippet}

  <div class="pm-body">
    <ul class="pm-list">
      {#each profileStore.list as name (name)}
        <li class="pm-row" class:active={name === profileStore.active}>
          {#if editing === name}
            <Input
              bind:value={draft}
              ariaLabel={editMode === 'clone' ? 'Clone profile name' : 'New profile name'}
              onkeydown={(e) => {
                if (e.key === 'Enter') commitEdit();
                else if (e.key === 'Escape') cancelEdit();
              }}
            />
            <div class="pm-actions">
              <Button
                size="sm"
                variant="primary"
                onclick={commitEdit}
                disabled={busy || !draft.trim()}
                title={editMode === 'clone' ? 'Create copy' : 'Save'}
              >
                <Check size={14} />
              </Button>
              <Button size="sm" variant="secondary" onclick={cancelEdit} title="Cancel">
                <XIcon size={14} />
              </Button>
            </div>
          {:else}
            <span class="pm-name">{name}</span>
            {#if name === profileStore.active}<span class="pm-badge">active</span>{/if}
            <div class="pm-actions">
              {#if name !== profileStore.active}
                <Button size="sm" variant="secondary" onclick={() => doSwitch(name)} disabled={profileStore.switching}>
                  Switch
                </Button>
              {/if}
              <Button size="sm" variant="ghost" onclick={() => startClone(name)} title="Duplicate">
                <Copy size={14} />
              </Button>
              {#if name !== DEFAULT}
                <Button size="sm" variant="ghost" onclick={() => startRename(name)} title="Rename">
                  <Pencil size={14} />
                </Button>
              {/if}
              {#if name !== profileStore.active && profileStore.list.length > 1}
                <Button size="sm" variant="ghost" onclick={() => { pendingDelete = name; }} title="Delete">
                  <Trash2 size={14} />
                </Button>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>

    <div class="pm-create">
      <Input
        bind:value={newName}
        placeholder="New profile name…"
        ariaLabel="New profile name"
        onkeydown={(e) => { if (e.key === 'Enter') doCreate(); }}
      />
      <Button variant="primary" onclick={doCreate} disabled={creating || !newName.trim()} loading={creating}>
        {#snippet iconStart()}<Plus size={14} />{/snippet}
        Create
      </Button>
    </div>

    {#if error}<p class="pm-error">{error}</p>{/if}
    <p class="pm-hint">
      Switching a profile reloads the window with that profile's settings,
      plugins and repos.
    </p>
  </div>
</Modal>

{#if pendingDelete}
  <ConfirmModal
    title="Delete profile"
    message={`Delete the profile “${pendingDelete}”?`}
    detail="Its settings, plugins and repo list are permanently removed."
    variant="danger"
    confirmLabel="Delete"
    busy={busy}
    onConfirm={confirmDelete}
    onCancel={() => { pendingDelete = null; }}
  />
{/if}

<style>
  .pm-body { display: flex; flex-direction: column; gap: 12px; font-family: var(--font-ui-sans); }

  .pm-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }

  .pm-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius-md);
    background: var(--bg-base);
    border: 1px solid transparent;
  }
  .pm-row.active { border-color: var(--accent-subtle); }

  .pm-name { font-size: var(--font-size-sm); color: var(--text-primary); }

  .pm-badge {
    font-size: var(--font-size-xs);
    color: var(--accent);
    background: var(--accent-subtle);
    padding: 1px 6px;
    border-radius: 999px;
  }

  .pm-actions { display: flex; align-items: center; gap: 4px; margin-left: auto; }

  .pm-create { display: flex; align-items: center; gap: 8px; }

  .pm-error {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--error);
    line-height: 1.5;
  }
  .pm-hint {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
</style>
