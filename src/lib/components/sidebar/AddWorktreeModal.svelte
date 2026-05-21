<script lang="ts">
  import { untrack } from 'svelte';
  import { Layers, FolderOpen, ChevronDown } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { addWorktree } from '$lib/ipc/worktree';
  import { repoStore } from '$lib/stores/repo.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    tabId,
    initialBranch,
    onClose,
    onAdded,
  }: {
    tabId: string;
    /** Optional pre-selected branch (local short name OR `origin/foo` remote
     *  ref). Used by the deep-link router to seed the picker for
     *  `arbor://branch/<name>?…&worktree=1` flows. */
    initialBranch?: string;
    onClose: () => void;
    onAdded: () => void;
  } = $props();

  // `branchValue` carries either a local branch name ("feature") or a remote
  // ref ("origin/feature"). The submit logic distinguishes the two.
  let destPath     = $state('');
  let branchValue  = $state(untrack(() => initialBranch ?? ''));
  let createBranch = $state(false);
  let newBranch    = $state('');
  let busy              = $state(false);
  let error             = $state<string | null>(null);
  let showFolderPicker  = $state(false);

  const localBranches  = $derived(repoStore.localBranches);
  const remoteBranches = $derived(repoStore.remoteBranches);

  /** Remote branches WITHOUT a local counterpart — these are the ones we
   *  surface as a separate group, so the user can check them out into a new
   *  worktree as a tracking branch. Remote branches that already have a
   *  matching local branch are redundant and would just confuse the picker. */
  const remoteOnlyBranches = $derived.by(() => {
    const localNames = new Set(localBranches.map(b => b.name));
    return remoteBranches
      .filter(b => {
        // strip the leading "<remote>/" segment to compare with local names
        const slashIdx = b.name.indexOf('/');
        if (slashIdx === -1) return false;
        const tail = b.name.slice(slashIdx + 1);
        // ignore the symbolic HEAD ("origin/HEAD") and already-tracked branches
        return tail !== 'HEAD' && !localNames.has(tail);
      });
  });

  const branchItems = $derived.by((): DropdownItem[] => {
    const out: DropdownItem[] = [];
    if (localBranches.length > 0) {
      out.push({
        kind: 'group',
        id: 'local',
        label: 'Local',
        items: localBranches.map(b => ({
          kind: 'item',
          id: b.name,
          label: b.name,
          active: branchValue === b.name,
          onclick: () => { branchValue = b.name; },
        })),
      });
    }
    if (remoteOnlyBranches.length > 0) {
      out.push({
        kind: 'group',
        id: 'remote',
        label: 'Remote (creates tracking branch)',
        items: remoteOnlyBranches.map(b => ({
          kind: 'item',
          id: b.name,
          label: b.name,
          active: branchValue === b.name,
          onclick: () => { branchValue = b.name; },
        })),
      });
    }
    return out;
  });

  /** True when the selected value is a remote ref like "origin/feature".
   *  Implies we'll create a new local branch tracking it. */
  const isRemoteSelected = $derived(
    !!branchValue && remoteOnlyBranches.some(b => b.name === branchValue),
  );

  /** Tail of a remote ref ("origin/feature" → "feature"). */
  function tailOf(remoteRef: string): string {
    const i = remoteRef.indexOf('/');
    return i === -1 ? remoteRef : remoteRef.slice(i + 1);
  }

  function pickFolder() {
    showFolderPicker = true;
  }

  async function handleSubmit() {
    if (!destPath.trim()) { error = 'Destination path is required.'; return; }
    if (!branchValue.trim() && !createBranch) { error = 'Select a branch or enable "Create new branch".'; return; }
    if (createBranch && !newBranch.trim()) { error = 'New branch name is required.'; return; }

    busy  = true;
    error = null;
    try {
      let branchArg: string;
      let newBranchArg: string | undefined;

      if (createBranch) {
        branchArg    = branchValue;
        newBranchArg = newBranch.trim();
      } else if (isRemoteSelected) {
        branchArg    = branchValue;
        newBranchArg = tailOf(branchValue);
      } else {
        branchArg    = branchValue;
        newBranchArg = undefined;
      }

      await addWorktree(tabId, destPath, branchArg, newBranchArg);
      onAdded();
    } catch (e) {
      error = `${e}`;
    } finally {
      busy = false;
    }
  }
</script>

{#if showFolderPicker}
  <FilePickerModal
    mode="folder"
    title="Select Destination Folder"
    onConfirm={(path) => { destPath = path; showFolderPicker = false; }}
    onCancel={() => { showFolderPicker = false; }}
  />
{/if}

<Modal {onClose} ariaLabel="Add worktree">
  {#snippet header()}
    <ModalHeader {onClose}>
      <span class="modal-icon"><Layers size={14} /></span>
      <span class="modal-title">Add Workspace</span>
    </ModalHeader>
  {/snippet}

  <div class="body">
    <FormField label="Destination folder" for="wt-dest-path">
      <div class="path-input-row">
        <input
          id="wt-dest-path"
          class="input"
          type="text"
          placeholder="/path/to/new-worktree"
          bind:value={destPath}
        />
        <button class="browse-btn" onclick={pickFolder} use:tooltip={'Browse…'}>
          <FolderOpen size={13} />
        </button>
      </div>
    </FormField>

    <FormField label="Checkout branch">
      <div class="branch-select-wrap">
        <Dropdown
          position="fixed"
          direction="down"
          matchTriggerWidth
          searchable={localBranches.length + remoteOnlyBranches.length > 8}
          searchPlaceholder="Filter branches…"
          items={branchItems}
          emptyMessage="No branches match"
        >
          {#snippet trigger({ open, toggle })}
            <button
              class="input branch-trigger"
              onclick={toggle}
              disabled={createBranch}
              type="button"
              aria-expanded={open}
            >
              <span class="branch-trigger-value" class:branch-trigger-placeholder={!branchValue}>
                {branchValue || '— select a branch —'}
              </span>
              <ChevronDown size={12} />
            </button>
          {/snippet}
        </Dropdown>
      </div>
      {#if isRemoteSelected && !createBranch}
        <span class="field-hint">
          A new local branch <strong>{tailOf(branchValue)}</strong> will be created tracking <strong>{branchValue}</strong>.
        </span>
      {/if}
    </FormField>

    <label class="toggle-row">
      <input type="checkbox" bind:checked={createBranch} />
      <span class="toggle-label">Create a new branch</span>
    </label>

    {#if createBranch}
      <FormField label="New branch name" for="wt-new-branch">
        <input
          id="wt-new-branch"
          class="input"
          type="text"
          placeholder="feature/my-feature"
          bind:value={newBranch}
        />
        {#if branchValue}
          <span class="field-hint">Starting from <strong>{branchValue}</strong></span>
        {/if}
      </FormField>
    {/if}

    {#if error}
      <div class="error-msg">{error}</div>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose}>Cancel</Button>
    <Button variant="primary" onclick={handleSubmit} disabled={busy} loading={busy}>
      Add Workspace
    </Button>
  {/snippet}
</Modal>

<style>
  .modal-icon { color: var(--accent); display: flex; align-items: center; }

  .body {
    display: flex; flex-direction: column; gap: 12px;
  }

  .field-hint { font-size: 11px; color: var(--text-muted); line-height: 1.45; }
  .field-hint strong { color: var(--text-primary); font-weight: 600; }

  .path-input-row { display: flex; gap: 6px; }
  .path-input-row .input { flex: 1; }

  .input {
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 5px 9px;
    font-size: 12.5px;
    color: var(--text-primary);
    outline: none;
    transition: border-color 0.15s;
    font-family: inherit;
    width: 100%;
    box-sizing: border-box;
  }
  .input:focus { border-color: var(--accent); }
  .input:disabled { opacity: 0.5; }

  .branch-select-wrap { width: 100%; }
  .branch-select-wrap :global(.dd-root) { width: 100%; }

  .branch-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    text-align: left;
    cursor: pointer;
    background: var(--bg-input);
  }
  .branch-trigger:disabled { cursor: not-allowed; }
  .branch-trigger:hover:not(:disabled),
  .branch-trigger[aria-expanded='true'] { border-color: var(--accent); }
  .branch-trigger-value {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .branch-trigger-placeholder { color: var(--text-muted); }

  .browse-btn {
    display: flex; align-items: center; justify-content: center;
    padding: 0 9px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 5px;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .browse-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .toggle-row {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    user-select: none;
  }
  .toggle-label { font-size: 12.5px; color: var(--text-secondary); }

  .error-msg {
    padding: 7px 10px;
    background: var(--error-subtle);
    border: 1px solid color-mix(in srgb, var(--error) 25%, transparent);
    border-radius: 5px;
    font-size: 12px;
    color: var(--error);
  }

</style>
