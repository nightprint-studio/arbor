<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { Check, Search, Plus, Folder, ChevronDown } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { WS_COLOR_COUNT, workspaceColorVar, SCRATCH_ID } from '$lib/types/workspace';
  import type { RepoRegistryEntryWithRoot } from '$lib/types/workspace';
  import { listRegistryWithRoots } from '$lib/ipc/workspace';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** When set, modal operates in edit mode against this workspace id. */
    editWorkspaceId?: string | null;
    onClose: () => void;
  }
  let { editWorkspaceId = null, onClose }: Props = $props();

  const editing = $derived(editWorkspaceId
    ? (workspacesStore.workspaces.find(w => w.id === editWorkspaceId) ?? null)
    : null);

  let name          = $state('');
  let colorIdx      = $state(0);
  let groupId       = $state<string | null>(null);
  let selectedRepos = $state<Set<string>>(new Set());
  let repoFilter    = $state('');
  let nameInput: HTMLInputElement | undefined;
  let saving        = $state(false);

  // Augmented registry — includes `is_worktree` so the picker can offer only
  // "root" repos.  Loaded once on mount; the modal lives briefly so we don't
  // need a reactive subscription.
  let registryWithRoots = $state<RepoRegistryEntryWithRoot[]>([]);

  // Hydrate from the workspace when in edit mode.
  onMount(async () => {
    if (editing) {
      name = editing.name;
      colorIdx = editing.color_idx;
      groupId  = editing.group_id;
      selectedRepos = new Set(editing.repo_ids);
    } else {
      // Pick a sensible default colour by rotating through the palette
      // based on how many workspaces already exist.
      const existing = workspacesStore.workspaces.filter(w => w.id !== SCRATCH_ID).length;
      colorIdx = existing % WS_COLOR_COUNT;
    }
    try { registryWithRoots = await listRegistryWithRoots(); } catch { /* keep empty list */ }
    await tick();
    nameInput?.focus();
    nameInput?.select();
  });

  // Picker shows only root repos.  Linked worktrees belong inside their root
  // and are reachable via the in-tab worktree switcher — listing them here
  // would let the user "add the same project twice" by accident.
  // Edit-mode caveat: if the workspace already has a worktree as a member
  // (legacy data, or the user added one manually before this change), we
  // keep showing it so it can be deselected — otherwise the count would be
  // unreachable.
  const visibleRepos = $derived.by(() => {
    return registryWithRoots.filter(r => !r.is_worktree || selectedRepos.has(r.id));
  });

  const filteredRepos = $derived.by(() => {
    const q = repoFilter.trim().toLowerCase();
    if (!q) return visibleRepos;
    return visibleRepos.filter(r =>
      r.display_name.toLowerCase().includes(q) ||
      r.path.toLowerCase().includes(q) ||
      (r.remote_url?.toLowerCase().includes(q) ?? false)
    );
  });

  const groupItems = $derived<DropdownItem[]>([
    {
      kind: 'item',
      id: '__none__',
      label: 'No group (top level)',
      active: groupId === null,
      onclick: () => { groupId = null; },
    },
    ...workspacesStore.groups.map(g => ({
      kind: 'item' as const,
      id: g.id,
      label: g.name,
      active: groupId === g.id,
      onclick: () => { groupId = g.id; },
    })),
  ]);

  function toggleRepo(id: string) {
    const next = new Set(selectedRepos);
    if (next.has(id)) next.delete(id); else next.add(id);
    selectedRepos = next;
  }

  function selectAllVisible() {
    const next = new Set(selectedRepos);
    for (const r of filteredRepos) next.add(r.id);
    selectedRepos = next;
  }
  function clearSelection() { selectedRepos = new Set(); }

  async function save() {
    const trimmed = name.trim();
    if (!trimmed) { nameInput?.focus(); return; }
    saving = true;
    try {
      if (editing) {
        await workspacesStore.updateWorkspace(editing.id, {
          name: trimmed,
          color_idx: colorIdx,
          group_id: groupId,
          repo_ids: [...selectedRepos],
        });
        uiStore.showToast(`Workspace "${trimmed}" updated`, 'success');
      } else {
        const ws = await workspacesStore.createWorkspace(trimmed, colorIdx, [...selectedRepos], groupId);
        uiStore.showToast(`Workspace "${ws.name}" created`, 'success');
      }
      onClose();
    } catch (e) {
      uiStore.showToast(`Failed: ${e}`, 'error');
    } finally {
      saving = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); void save(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal
  {onClose}
  width="680px"
  height="740px"
  ariaLabel={editing ? `Edit workspace ${editing.name}` : 'Create workspace'}
>
  {#snippet header()}
    <ModalHeader
      title={editing ? 'Edit Workspace' : 'New Workspace'}
      {onClose}
    />
  {/snippet}

  <div class="ws-body">
    <!-- Name + colour -->
    <div class="field">
      <label for="ws-name">Name</label>
      <div class="name-row">
        <Monogram name={name || '?'} color={workspaceColorVar(colorIdx)} size={26} />
        <input
          id="ws-name"
          bind:this={nameInput}
          bind:value={name}
          placeholder="e.g. Client Apps"
          maxlength="40"
        />
      </div>
    </div>

    <div class="field">
      <label for="ws-colour">Colour</label>
      <div class="swatches" id="ws-colour">
        {#each Array.from({ length: WS_COLOR_COUNT }, (_, i) => i) as i}
          <button
            class="swatch"
            class:selected={colorIdx === i}
            style="background: {workspaceColorVar(i)};"
            onclick={() => colorIdx = i}
            use:tooltip={`Colour ${i + 1}`}
            aria-label="Choose colour {i + 1}"
            aria-pressed={colorIdx === i}
          >
            {#if colorIdx === i}<Check size={12} />{/if}
          </button>
        {/each}
      </div>
    </div>

    <!-- Group -->
    {#if workspacesStore.groups.length > 0 || !editing}
      <div class="field">
        <span class="label">Group (optional)</span>
        <div class="group-select-wrap">
          <Dropdown position="fixed" direction="down" matchTriggerWidth items={groupItems}>
            {#snippet trigger({ open, toggle })}
              <button class="group-trigger" onclick={toggle} type="button" aria-expanded={open}>
                <span class="group-trigger-label">
                  {groupId
                    ? (workspacesStore.groups.find(g => g.id === groupId)?.name ?? 'Unknown')
                    : 'No group (top level)'}
                </span>
                <ChevronDown size={12} />
              </button>
            {/snippet}
          </Dropdown>
        </div>
      </div>
    {/if}

    <!-- Repos multi-select -->
    <div class="field repos-field">
      <div class="field-header">
        <label for="ws-repos">Repositories ({selectedRepos.size} selected)</label>
        <div class="field-actions">
          <button class="link-btn" onclick={selectAllVisible} type="button">Select visible</button>
          <button class="link-btn" onclick={clearSelection}   type="button">Clear</button>
        </div>
      </div>

      <div class="repo-search">
        <Search size={13} />
        <input
          bind:value={repoFilter}
          placeholder="Filter by name, path or remote URL…"
        />
      </div>

      <div class="repo-list" id="ws-repos">
        {#if registryWithRoots.length === 0}
          <div class="empty">
            <Folder size={24} />
            <p>No repositories registered yet.</p>
            <span>Open a repository first — it will appear here.</span>
          </div>
        {:else if visibleRepos.length === 0}
          <div class="empty">
            <Folder size={24} />
            <p>No root repositories available.</p>
            <span>The registry only contains linked worktrees — switch to a worktree from inside its tab instead of adding it here.</span>
          </div>
        {:else if filteredRepos.length === 0}
          <div class="empty"><span>No repositories match "{repoFilter}"</span></div>
        {:else}
          {#each filteredRepos as repo (repo.id)}
            <button
              class="repo-row"
              class:selected={selectedRepos.has(repo.id)}
              class:legacy-worktree={repo.is_worktree}
              onclick={() => toggleRepo(repo.id)}
              type="button"
              use:tooltip={repo.is_worktree ? { content: 'Linked worktree', description: 'Legacy member — uncheck to remove from this workspace' } : ''}
            >
              <span class="check" class:on={selectedRepos.has(repo.id)}>
                {#if selectedRepos.has(repo.id)}<Check size={12} />{/if}
              </span>
              <div class="repo-body">
                <span class="repo-name">{repo.display_name}</span>
                <span class="repo-path">{repo.path}</span>
              </div>
              {#if repo.is_worktree}
                <span class="worktree-tag">worktree</span>
              {/if}
            </button>
          {/each}
        {/if}
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
  .ws-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100%;
  }

  /* Each field is a grey card on the bg-base body — raises the fields out
     of the dark body so the content has visual structure instead of
     floating on a flat slab.
     `overflow: hidden` keeps inner content (notably the long repo-list)
     respecting the card's rounded corner + border, instead of bleeding
     past the bottom edge when flex-allocation gets tight. */
  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 12px 14px;
    overflow: hidden;
  }
  .field label,
  .field .label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    font-weight: 600;
  }
  .field-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .field-actions { display: flex; gap: 8px; }
  .link-btn {
    background: transparent;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: 11px;
    padding: 0;
  }
  .link-btn:hover { text-decoration: underline; }

  .name-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  input {
    flex: 1;
    min-width: 0;
    padding: 6px 10px;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
  }
  input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .group-select-wrap { width: 100%; }
  .group-select-wrap :global(.dd-root) { width: 100%; }
  .group-trigger {
    width: 100%;
    box-sizing: border-box;
    padding: 6px 10px;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    text-align: left;
    transition: border-color var(--transition-fast);
  }
  .group-trigger:focus,
  .group-trigger:hover { outline: none; border-color: var(--accent); }
  .group-trigger-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

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
  .swatch:hover  { transform: scale(1.08); }
  .swatch.selected {
    box-shadow: 0 0 0 2px var(--bg-elevated), 0 0 0 4px currentColor;
  }

  .repos-field { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .repo-search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
  }
  .repo-search input {
    flex: 1;
    background: transparent;
    border: none;
    padding: 0;
    color: var(--text-primary);
  }
  .repo-search input:focus { outline: none; }

  .repo-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    margin-top: 8px;
  }
  .repo-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    text-align: left;
    transition: background var(--transition-fast);
  }
  .repo-row:last-child { border-bottom: none; }
  .repo-row:hover      { background: var(--bg-hover); }
  .repo-row.selected   { background: var(--accent-subtle); }
  /* A linked worktree only shows up here when it's already a member (legacy
     data).  Keep it visible so the user can deselect it, but soften it so
     it's clearly not a "real" pickable option. */
  .repo-row.legacy-worktree { font-style: italic; opacity: 0.85; }
  .worktree-tag {
    flex-shrink: 0;
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--accent);
    background: var(--accent-subtle);
    padding: 1px 6px;
    border-radius: var(--radius-md);
  }

  .check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: 1.5px solid var(--border);
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }
  .check.on { background: var(--accent); border-color: var(--accent); color: var(--text-on-accent); }

  .repo-body {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }
  .repo-name {
    font-size: var(--font-size-sm);
    font-weight: 500;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .repo-path {
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-ui);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 30px 16px;
    gap: 8px;
    color: var(--text-muted);
    text-align: center;
    font-size: 12px;
  }

</style>
