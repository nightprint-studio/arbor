<script lang="ts">
  /**
   * DeepLinkConfirmModal — gate for `arbor://` actions that require a clone.
   *
   * The deep-link dispatcher opens this when the URL targets a repo that
   * isn't in the registry (or whose path is gone from disk).  The user must
   * explicitly accept the clone — pressing Cancel aborts the whole action,
   * matching the user's stated rule "se non accetta non gli apri nulla".
   *
   * On success the modal performs the clone, registers the result in the
   * active workspace and emits `onConfirmed(repoId, path)` so the caller
   * can chain the actual action (jump to commit, open MR, …).
   */
  import { untrack } from 'svelte';
  import { Link2, Download, FolderOpen } from 'lucide-svelte';
  import Alert from './ui/Alert.svelte';
  import Button from './ui/Button.svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import FormField from './ui/FormField.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import UrlBlock from './ui/UrlBlock.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { cloneRepo, openRepo, closeRepo } from '$lib/ipc/graph';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { defaultRepoNameFromUrl } from '$lib/utils/git-url';

  let {
    url,
    actionDescription,
    reason,
    onClose,
    onConfirmed,
  }: {
    /** The git remote URL we're about to clone. */
    url: string;
    /** Human-readable description of what will happen *after* the clone,
     *  e.g. "Open commit abc1234".  Shown in the body so the user knows
     *  exactly what they're consenting to. */
    actionDescription: string;
    /** Why we're asking — drives the explanatory line under the action. */
    reason: 'unknown' | 'missing_on_disk';
    onClose:     () => void;
    onConfirmed: (repoId: string, path: string) => void;
  } = $props();

  // ── State ─────────────────────────────────────────────────────────────
  let baseFolder = $state('');
  let folderName = $state(untrack(() => defaultRepoNameFromUrl(url)));
  let cloning    = $state(false);
  let error      = $state('');
  let showPicker = $state(false);

  const baseTrimmed = $derived(baseFolder.trim());
  const nameTrimmed = $derived(folderName.trim());

  function joinPath(base: string, name: string): string {
    if (!base) return name;
    if (!name) return base;
    const sep = base.includes('\\') ? '\\' : '/';
    return base.replace(/[\\/]+$/, '') + sep + name;
  }

  const fullPath = $derived(joinPath(baseTrimmed, nameTrimmed));
  const canClone = $derived(
    baseTrimmed !== '' && nameTrimmed !== '' && !cloning,
  );

  const reasonText = $derived(
    reason === 'missing_on_disk'
      ? 'The local copy of this repository is missing — re-cloning will get you back to the requested action.'
      : 'This repository isn\'t in your library yet.',
  );

  // ── Actions ───────────────────────────────────────────────────────────
  async function handleClone() {
    if (!canClone) return;
    cloning = true;
    error   = '';
    try {
      const tempTabId = crypto.randomUUID();
      const cloned    = await cloneRepo(
        { url, dest_path: fullPath, branch: undefined, shallow: false, recurse_submodules: false },
        tempTabId,
      );
      try { await closeRepo(tempTabId); } catch { /* best-effort cleanup */ }

      const repoId = await workspacesStore.ensureRepoRegistered(cloned.path);
      const info   = await openRepo(cloned.path, repoId);
      tabsStore.addTab(info);
      uiStore.addRecentRepo(info.path);
      uiStore.showToast(`Cloned ${info.name}`, 'success');
      onConfirmed(repoId, info.path);
    } catch (err) {
      error   = String(err);
      cloning = false;
    }
  }

  function onPickerConfirm(path: string) {
    baseFolder = path.replace(/[\\/]+$/, '');
    showPicker = false;
  }

  function onKeydown(e: KeyboardEvent) {
    if (showPicker) return;
    if (e.key === 'Enter' && canClone) handleClone();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if showPicker}
  <FilePickerModal
    mode="folder"
    title="Choose destination folder"
    initialPath={baseTrimmed || undefined}
    onConfirm={onPickerConfirm}
    onCancel={() => showPicker = false}
  />
{:else}
  <Modal {onClose} width="500px" ariaLabel="Confirm Deep Link">
    {#snippet header()}
      <ModalHeader {onClose}>
        <Link2 size={15} strokeWidth={2} />
        <span class="modal-title">Open Deep Link</span>
      </ModalHeader>
    {/snippet}

    <div class="dl-body">
      <Alert variant="info" title={actionDescription} text={reasonText} />

      <UrlBlock label="Repository URL" value={url} />

      <FormField label="Destination folder" for="dl-base">
        <div class="input-with-action">
          <input
            id="dl-base"
            type="text"
            class="input"
            placeholder="Parent folder (e.g. C:\Sviluppo)"
            bind:value={baseFolder}
            spellcheck="false"
            autocomplete="off"
          />
          <button
            class="input-action-btn"
            onclick={() => showPicker = true}
            tabindex={-1}
            use:tooltip={'Browse…'}
            aria-label="Browse for folder"
          >
            <FolderOpen size={14} strokeWidth={1.8} />
          </button>
        </div>
      </FormField>

      <FormField label="Folder name" for="dl-name">
        <input
          id="dl-name"
          type="text"
          class="input"
          bind:value={folderName}
          spellcheck="false"
          autocomplete="off"
        />
        {#if fullPath}
          <div class="path-preview" use:tooltip={fullPath}>
            Will clone into <code>{fullPath}</code>
          </div>
        {/if}
      </FormField>

      {#if error}
        <Alert variant="error" title="Clone failed">{error}</Alert>
      {/if}
    </div>

    {#snippet footer()}
      <Button variant="secondary" onclick={onClose} disabled={cloning}>Cancel</Button>
      <Button
        variant="primary"
        onclick={handleClone}
        disabled={!canClone}
        loading={cloning}
      >
        {#snippet iconStart()}
          <Download size={13} />
        {/snippet}
        {cloning ? 'Cloning…' : 'Clone & Continue'}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .dl-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .dl-body .input {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    padding: 6px 10px;
    width: 100%;
    outline: none;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .dl-body .input:focus {
    border-color: var(--border-focus);
    box-shadow: 0 0 0 2px rgba(61,127,255,0.15);
  }

  .input-with-action { position: relative; display: flex; align-items: center; }
  .input-with-action .input { padding-right: 32px; }

  .input-action-btn {
    position: absolute;
    right: 1px; top: 1px; bottom: 1px;
    width: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-left: 1px solid transparent;
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .input-action-btn:hover {
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border-color: var(--border);
  }

  .path-preview {
    margin-top: 4px;
    color: var(--text-muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path-preview code {
    font-family: var(--font-code);
    color: var(--text-secondary);
  }
</style>
