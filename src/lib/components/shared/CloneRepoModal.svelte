<script lang="ts">
  import { Download, GitBranch, RefreshCw, AlertCircle, FolderOpen, ChevronDown } from 'lucide-svelte';
  import Button from './ui/Button.svelte';
  import { listRemoteBranchesForUrl, cloneRepo, openRepo, closeRepo } from '$lib/ipc/graph';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import FormField from './ui/FormField.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let { onClose, onCloned }: {
    onClose:  () => void;
    onCloned: (path: string) => void;
  } = $props();

  // ── Form state ────────────────────────────────────────────────────────────
  let url        = $state('');
  let baseFolder = $state('');    // parent folder — picker + typable
  let folderName = $state('');    // repo dir name — auto-filled from URL,
                                  // manually overridable without being
                                  // clobbered by subsequent URL edits
  let nameTouched = $state(false); // flips true on the first manual edit of
                                   // `folderName`; clearing the field resets it
  let branch     = $state('');
  let shallow    = $state(false);
  let recurse    = $state(false);

  // ── Async / UI state ──────────────────────────────────────────────────────
  let fetchingBranches = $state(false);
  let branches         = $state<string[]>([]);
  let branchFetchError = $state('');
  let cloning          = $state(false);
  let cloneError       = $state('');
  let showPicker       = $state(false);

  // ── Derived ───────────────────────────────────────────────────────────────
  const urlTrimmed  = $derived(url.trim());
  const baseTrimmed = $derived(baseFolder.trim());
  const nameTrimmed = $derived(folderName.trim());

  function joinBaseAndName(base: string, name: string): string {
    if (!base) return name;
    if (!name) return base;
    const sep = base.includes('\\') ? '\\' : '/';
    return base.replace(/[\\/]+$/, '') + sep + name;
  }

  const fullPath = $derived(joinBaseAndName(baseTrimmed, nameTrimmed));
  const canClone = $derived(
    urlTrimmed !== '' && baseTrimmed !== '' && nameTrimmed !== '' && !cloning,
  );

  const branchItems = $derived<DropdownItem[]>([
    {
      kind: 'item',
      id: '__default__',
      label: 'Default branch',
      active: branch === '',
      onclick: () => { branch = ''; },
    },
    ...branches.map(b => ({
      kind: 'item' as const,
      id: b,
      label: b,
      active: branch === b,
      onclick: () => { branch = b; },
    })),
  ]);

  function repoNameFromUrl(u: string): string {
    return u.replace(/\.git$/, '').split(/[/\\?#]/).filter(Boolean).pop() ?? '';
  }

  // Auto-fill `folderName` from the URL while the user hasn't typed a
  // custom name. Once they edit the Folder name field, `nameTouched` pins
  // the chosen value — pasting another URL no longer overwrites it.
  // Clearing the field back to empty re-enables auto-fill.
  $effect(() => {
    const derived = repoNameFromUrl(urlTrimmed);
    if (!derived) return;
    if (nameTouched) return;
    folderName = derived;
  });

  function onFolderNameInput(newValue: string) {
    folderName  = newValue;
    // Empty string reopens the auto-fill door — useful if the user wants
    // to start over from the URL-derived name.
    nameTouched = newValue.trim() !== '';
  }

  // ── Actions ───────────────────────────────────────────────────────────────
  async function fetchBranches() {
    if (!urlTrimmed) return;
    fetchingBranches = true;
    branchFetchError = '';
    branches         = [];
    try {
      branches = await listRemoteBranchesForUrl(urlTrimmed);
    } catch (err) {
      branchFetchError = String(err).replace(/^.*error:/i, '').trim();
    } finally {
      fetchingBranches = false;
    }
  }

  function onUrlBlur() {
    const u = urlTrimmed;
    if (u.startsWith('http') || u.startsWith('git@') || u.startsWith('ssh://') || u.startsWith('git://')) {
      fetchBranches();
    }
  }

  function onPickerConfirm(path: string) {
    baseFolder = path.replace(/[\\/]+$/, '');
    showPicker = false;
  }

  /** Path to seed the picker with. Prefer the current base folder; fall
   *  back to undefined (picker picks its own default) otherwise. */
  function pickerInitialPath(): string | undefined {
    return baseTrimmed || undefined;
  }

  // ── Local-path detection ──────────────────────────────────────────────────
  // If the URL looks like a filesystem path (drive letter, UNC, absolute
  // POSIX) rather than an HTTP/SSH URL, we warn the user: cloning from a
  // local non-bare repo sets `origin` to that path and push will fail with
  // libgit2's "local push doesn't support non-bare repos" error. Very
  // easy to hit by accident when the user types a path in the URL field
  // instead of an URL.
  const urlLooksLocal = $derived.by(() => {
    const u = urlTrimmed;
    if (!u) return false;
    if (/^(https?|ssh|git|file):\/\//i.test(u)) return false;
    if (/^git@[^:]+:/.test(u)) return false; // scp-style git@host:path
    // Anything left that starts with a Windows drive, UNC share, or POSIX
    // absolute path is treated as local.
    return /^[A-Za-z]:[\\/]/.test(u) || u.startsWith('\\\\') || u.startsWith('/');
  });

  async function handleClone() {
    if (!canClone) return;
    cloning    = true;
    cloneError = '';
    try {
      // Clone via a temp tab id — the backend opens the repo handle keyed by
      // this id.  We then close it and reopen with the canonical workspace
      // registry id, which also adds the repo to the active workspace.
      const tempTabId = crypto.randomUUID();
      const cloned    = await cloneRepo(
        {
          url:                urlTrimmed,
          dest_path:          fullPath,
          branch:             branch || undefined,
          shallow,
          recurse_submodules: recurse,
        },
        tempTabId,
      );
      try { await closeRepo(tempTabId); } catch { /* best effort */ }

      const repoId = await workspacesStore.ensureRepoRegistered(cloned.path);
      const info   = await openRepo(cloned.path, repoId);
      tabsStore.addTab(info);
      uiStore.addRecentRepo(info.path);
      uiStore.showToast(`Cloned ${info.name}`, 'success');
      onCloned(info.path);
    } catch (err) {
      cloneError = String(err);
      cloning    = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (showPicker) return;
    if (e.key !== 'Enter' || e.shiftKey) return;
    // Don't hijack Enter when focus is on a button/checkbox — let the
    // element's native activation (e.g. opening the folder picker) win.
    const t = e.target as HTMLElement | null;
    if (t instanceof HTMLButtonElement) return;
    if (t instanceof HTMLInputElement && (t.type === 'checkbox' || t.type === 'button')) return;
    if (canClone) handleClone();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if showPicker}
  <FilePickerModal
    mode="folder"
    title="Choose destination folder"
    initialPath={pickerInitialPath()}
    onConfirm={onPickerConfirm}
    onCancel={() => showPicker = false}
  />
{:else}
  <Modal {onClose} width="500px" ariaLabel="Clone Repository">
    {#snippet header()}
      <ModalHeader {onClose}>
        <Download size={15} strokeWidth={2} />
        <span class="modal-title">Clone Repository</span>
      </ModalHeader>
    {/snippet}

    <div class="dialog-body">

      <!-- URL -->
      <FormField label="Repository URL" for="clone-url">
        <input
          id="clone-url"
          type="url"
          class="input"
          placeholder="https://github.com/owner/repository.git"
          bind:value={url}
          onblur={onUrlBlur}
          autocomplete="off"
          spellcheck="false"
        />
        {#if urlLooksLocal}
          <div class="inline-warn">
            <AlertCircle size={12} />
            <span>
              Stai clonando da un <strong>path locale</strong>. L'<code>origin</code>
              del clone punterà a questa cartella, non a un server. Se è una copia
              locale di un repo con upstream remoto, usa direttamente l'URL
              HTTPS/SSH — altrimenti il <code>git push</code> successivo fallirà
              con <code>local push doesn't support non-bare repos</code>.
            </span>
          </div>
        {/if}
      </FormField>

      <!-- Base folder -->
      <FormField label="Base folder" for="clone-base">
        <div class="input-with-action">
          <input
            id="clone-base"
            type="text"
            class="input"
            placeholder="Parent folder (e.g. C:\Sviluppo)"
            bind:value={baseFolder}
            spellcheck="false"
            autocomplete="off"
          />
          <button
            type="button"
            class="input-action-btn"
            onclick={() => showPicker = true}
            use:tooltip={'Browse…'}
            aria-label="Browse for folder"
          >
            <FolderOpen size={14} strokeWidth={1.8} />
          </button>
        </div>
      </FormField>

      <!-- Folder name -->
      <FormField label="Folder name" for="clone-name">
        <input
          id="clone-name"
          type="text"
          class="input"
          placeholder="Auto-filled from URL"
          value={folderName}
          oninput={(e) => onFolderNameInput((e.currentTarget as HTMLInputElement).value)}
          spellcheck="false"
          autocomplete="off"
        />
        {#if fullPath}
          <div class="path-preview" use:tooltip={fullPath}>
            Clones into <code>{fullPath}</code>
          </div>
        {/if}
      </FormField>

      <!-- Branch -->
      <FormField label="Branch" for="clone-branch">
        {#snippet actions()}
          <button
            class="link-btn"
            onclick={fetchBranches}
            disabled={fetchingBranches || !urlTrimmed}
          >
            {#if fetchingBranches}
              <RefreshCw size={11} class="spin" />
              Fetching…
            {:else}
              <RefreshCw size={11} />
              Fetch branches
            {/if}
          </button>
        {/snippet}

        <div class="branch-select-wrap">
          <Dropdown
            position="fixed"
            direction="down"
            matchTriggerWidth
            items={branchItems}
          >
            {#snippet trigger({ open, toggle })}
              <button
                class="input branch-trigger"
                onclick={toggle}
                type="button"
                aria-expanded={open}
              >
                <span class="branch-prefix-icon"><GitBranch size={13} /></span>
                <span class="branch-trigger-value">
                  {branch || 'Default branch'}
                </span>
                <ChevronDown size={12} />
              </button>
            {/snippet}
          </Dropdown>
        </div>

        {#if branchFetchError}
          <div class="inline-error">
            <AlertCircle size={12} />
            <span>{branchFetchError}</span>
          </div>
        {/if}
      </FormField>

      <!-- Options -->
      <div class="options">
        <label class="checkbox-item">
          <input type="checkbox" bind:checked={shallow} />
          <span>Shallow clone</span>
          <span class="opt-note">depth 1</span>
        </label>
        <label class="checkbox-item">
          <input type="checkbox" bind:checked={recurse} />
          <span>Recurse submodules</span>
        </label>
      </div>

      <!-- Clone error -->
      {#if cloneError}
        <div class="error-panel">
          <AlertCircle size={13} class="error-icon" />
          <pre class="error-text">{cloneError}</pre>
        </div>
      {/if}

    </div>

    {#snippet footer()}
      <Button variant="secondary" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        onclick={handleClone}
        disabled={!canClone}
        loading={cloning}
      >
        {#snippet iconStart()}
          <Download size={13} />
        {/snippet}
        {cloning ? 'Cloning…' : 'Clone'}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  /* ── Body ── */
  .dialog-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .dialog-body .input {
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
  .dialog-body .input:focus {
    border-color: var(--border-focus);
    box-shadow: 0 0 0 2px rgba(61,127,255,0.15);
  }
  .dialog-body .input::placeholder { color: var(--text-disabled); }

  /* Input + browse button */
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

  /* Branch dropdown */
  .branch-select-wrap { width: 100%; }
  .branch-select-wrap :global(.dd-root) { width: 100%; }

  .branch-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    text-align: left;
  }
  .branch-trigger:hover,
  .branch-trigger[aria-expanded='true'] { border-color: var(--border-focus); }
  .branch-prefix-icon {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .branch-trigger-value {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Link-style fetch button */
  .link-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: none;
    color: var(--accent);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    padding: 0;
    opacity: 0.85;
    transition: opacity var(--transition-fast);
  }
  .link-btn:hover:not(:disabled) { opacity: 1; }
  .link-btn:disabled { color: var(--text-disabled); cursor: default; opacity: 1; }

  /* Options */
  .options { display: flex; gap: 20px; padding-top: 2px; }

  .checkbox-item {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
    user-select: none;
  }
  .checkbox-item input[type="checkbox"] {
    accent-color: var(--accent);
    width: 13px; height: 13px;
    cursor: pointer; flex-shrink: 0;
  }
  .opt-note { color: var(--text-disabled); font-size: 10px; font-family: var(--font-code); }

  /* Error panel */
  .error-panel {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    background: var(--error-subtle);
    border: 1px solid rgba(199,84,80,0.3);
    border-radius: var(--radius-sm);
    padding: 9px 12px;
    overflow: auto;
    max-height: 110px;
  }
  :global(.error-icon) { color: var(--error); flex-shrink: 0; margin-top: 1px; }
  .error-text {
    margin: 0;
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--error);
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.5;
  }

  /* Inline warning (e.g. URL looks local) */
  .inline-warn {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 6px 8px;
    margin-top: 2px;
    background: color-mix(in srgb, var(--status-warning, #fbbf24) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--status-warning, #fbbf24) 28%, transparent);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 11px;
    line-height: 1.45;
  }
  .inline-warn :global(svg) {
    color: var(--status-warning, #fbbf24);
    flex-shrink: 0;
    margin-top: 1px;
  }
  .inline-warn code {
    font-family: var(--font-code);
    font-size: 10.5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    padding: 0 4px;
    border-radius: var(--radius-sm);
  }
  .inline-warn strong { color: var(--text-primary); }

  /* Composed-path preview under the Folder name field */
  .path-preview {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path-preview code {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-secondary);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    padding: 0 4px;
    border-radius: var(--radius-sm);
  }

  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .inline-error {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--error);
  }
</style>
