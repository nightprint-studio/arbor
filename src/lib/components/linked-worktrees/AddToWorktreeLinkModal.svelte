<script lang="ts">
  import { Layers, Plus, Folder } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { uiStore }    from '$lib/stores/ui.svelte';
  import { linkedWorktreesStore } from '$lib/stores/linkedWorktrees.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { addWorktreeLinkMember, createWorktreeLink } from '$lib/ipc/linkedWorktree';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';

  const repoId = $derived(uiStore.addToLinkRepoId);
  const repo   = $derived(workspacesStore.registry.find(r => r.id === repoId) ?? null);
  const links  = $derived(linkedWorktreesStore.links);

  // Worktree link this repo is already in (a repo can be in at most one).
  const occupied = $derived(linkedWorktreesStore.linkForRepo(repoId));

  let mode       = $state<'pick' | 'create'>('pick');
  let newName    = $state('');
  let busy       = $state(false);
  let error      = $state<string | null>(null);
  let newNameEl: HTMLInputElement | undefined = $state();
  $effect(() => { if (mode === 'create') newNameEl?.focus(); });

  function close() { uiStore.closeAddToLink(); mode = 'pick'; newName = ''; error = null; }

  async function joinLink(linkId: string) {
    if (!repoId) return;
    busy = true; error = null;
    try {
      await addWorktreeLinkMember(linkId, repoId);
      close();
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }

  async function confirmCreate() {
    if (!repoId || !newName.trim()) { error = 'Name required.'; return; }
    busy = true; error = null;
    try {
      await createWorktreeLink(newName.trim(), [repoId]);
      close();
    } catch (e) { error = `${e}`; }
    finally { busy = false; }
  }
</script>

{#if uiStore.addToLinkModalOpen}
  <Modal onClose={close} ariaLabel="Add to Linked Worktrees">
    {#snippet header()}
      <ModalHeader onClose={close}>
        <span class="modal-icon"><Layers size={14}/></span>
        <span class="modal-title">Link this Worktree</span>
      </ModalHeader>
    {/snippet}

    <div class="body">
      {#if !repo}
        <div class="empty">Repo not found in registry.</div>
      {:else}
        <div class="repo-line">
          <Folder size={11}/>
          <span class="repo-name">{repo.display_name}</span>
          <span class="repo-path">{repo.path}</span>
        </div>

        {#if occupied}
          <div class="info">Already a member of <strong>{occupied.name}</strong>. Remove it from there first.</div>
        {:else}
          <div class="tabs">
            <button class:active={mode === 'pick'} onclick={() => mode = 'pick'}>Existing</button>
            <button class:active={mode === 'create'} onclick={() => mode = 'create'}>Create new</button>
          </div>

          {#if mode === 'pick'}
            {#if links.length === 0}
              <div class="empty">No links yet — switch to "Create new".</div>
            {:else}
              <ul class="link-list">
                {#each links as l}
                  <li>
                    <button class="link-row" onclick={() => joinLink(l.id)} disabled={busy}>
                      <Layers size={11}/>
                      <span class="name">{l.name}</span>
                      <span class="count">{l.members.length} members</span>
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          {:else}
            <div class="create">
              <input class="input" placeholder="Link name…" bind:value={newName} bind:this={newNameEl}
                onkeydown={(e) => { if (e.key === 'Enter') confirmCreate(); }}/>
              <button class="btn-primary" onclick={confirmCreate} disabled={busy}>
                <Plus size={12}/> Create
              </button>
            </div>
          {/if}
        {/if}

        {#if error}<div class="error-msg">{error}</div>{/if}
      {/if}
    </div>

    {#snippet footer()}
      <Button variant="secondary" onclick={close}>Close</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .modal-icon { color: var(--accent); display: flex; align-items: center; }

  .body { display: flex; flex-direction: column; gap: 10px; }

  .repo-line { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-secondary); padding: 6px 8px; background: var(--bg-overlay); border-radius: 5px; }
  .repo-name { font-weight: 500; color: var(--text-primary); }
  .repo-path { font-family: var(--font-code); font-size: 11px; color: var(--text-muted); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; }

  .tabs { display: flex; gap: 4px; }
  .tabs button {
    flex: 1; padding: 6px 10px;
    background: transparent; border: 1px solid var(--border);
    border-radius: 5px; color: var(--text-secondary);
    font-size: 12px; cursor: pointer;
  }
  .tabs button.active { background: var(--accent-subtle); border-color: var(--accent); color: var(--accent); }

  .link-list { list-style: none; margin: 0; padding: 0; max-height: 240px; overflow-y: auto; }
  .link-row { width: 100%; display: flex; align-items: center; gap: 8px; padding: 6px 10px; border: none; background: transparent; font-size: 12px; color: var(--text-secondary); cursor: pointer; text-align: left; border-radius: 5px; }
  .link-row:hover { background: var(--bg-hover); color: var(--text-primary); }
  .link-row .name { flex: 1; }
  .link-row .count { font-size: 11px; color: var(--text-muted); }

  .create { display: flex; gap: 6px; }
  .create .input { flex: 1; }

  .info { padding: 8px 10px; background: var(--bg-overlay); border-radius: 5px; font-size: 11.5px; color: var(--text-muted); }
  .empty { padding: 6px 10px; font-size: 12px; color: var(--text-muted); font-style: italic; }

  .input {
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 5px 8px;
    font-size: 12px;
    color: var(--text-primary);
    outline: none; transition: border-color 0.15s;
    font-family: inherit;
  }
  .input:focus { border-color: var(--accent); }

  .btn-primary {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 5px 12px; border-radius: 5px;
    font-size: 12px; cursor: pointer;
    border: 1px solid transparent; transition: background 0.12s;
    background: var(--accent); color: var(--text-on-accent);
  }
  .btn-primary:hover { opacity: 0.88; }
  .btn-primary:disabled { opacity: 0.5; pointer-events: none; }

  .error-msg {
    padding: 6px 10px;
    background: var(--error-subtle);
    border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    border-radius: 5px;
    font-size: 11.5px;
    color: var(--error);
  }
</style>
