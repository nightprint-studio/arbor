<script lang="ts">
  import { onMount } from 'svelte';
  import {
    GitBranch, FolderOpen, ChevronDown, Globe, Lock,
    Github, Gitlab, Link, FileText, Scale, Info, AlertTriangle,
    Check, Loader, BookOpen, UserCircle, X,
  } from 'lucide-svelte';
  import Button from './ui/Button.svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import Dropdown from './ui/Dropdown.svelte';
  import type { DropdownItem } from './ui/Dropdown.svelte';
  import Tabs from './ui/Tabs.svelte';
  import FormField from './ui/FormField.svelte';
  import Input from './ui/Input.svelte';
  import { getGitIdentity } from '$lib/ipc/graph';
  import type { InitRepoOptions } from '$lib/types/git';

  // ── Helpers (initialised below `gitignoreOptions` / `licenseOptions`) ───────

  let {
    path,
    onInit,
    onCancel,
  }: {
    path: string;
    onInit: (opts: InitRepoOptions) => void;
    onCancel: () => void;
  } = $props();

  // ── Tabs ────────────────────────────────────────────────────────────────────
  type Tab = 'project' | 'files' | 'remote';
  let activeTab = $state<Tab>('project');
  function setActiveTab(id: string) { activeTab = id as Tab; }

  // ── Project tab state ───────────────────────────────────────────────────────
  let description    = $state('');
  let defaultBranch  = $state('main');
  let customBranch   = $state('');
  let branchPreset   = $state<'main'|'master'|'develop'|'custom'>('main');
  let initialCommit  = $state(true);
  let commitMessage  = $state('Initial commit');
  let authorName     = $state('');
  let authorEmail    = $state('');
  let identityLoaded = $state(false);

  // ── Files tab state ─────────────────────────────────────────────────────────
  let gitignoreTemplate = $state('');
  let license           = $state('');
  let readme            = $state(true);

  // ── Remote tab state ────────────────────────────────────────────────────────
  let provider   = $state<'none'|'github'|'gitlab'|'custom'>('none');
  let visibility = $state<'private'|'public'>('private');
  let org        = $state('');
  let remoteUrl  = $state('');
  let pushInitial = $state(true);

  // ── Derived ─────────────────────────────────────────────────────────────────
  const folderName = $derived(path.replace(/\\/g, '/').split('/').pop() ?? path);

  const resolvedBranch = $derived(
    branchPreset === 'custom' ? customBranch.trim() || 'main' : branchPreset
  );

  const isValid = $derived(
    resolvedBranch.length > 0 &&
    (!initialCommit || commitMessage.trim().length > 0)
  );

  const remoteWarning = $derived(
    (provider === 'github' || provider === 'gitlab')
      ? `Requires a ${provider === 'github' ? 'GitHub' : 'GitLab'} token stored in Settings → Authentication.`
      : ''
  );

  // Flag a custom remote URL that looks like a filesystem path. Setting
  // `origin` to a local non-bare repo makes libgit2 fail the subsequent
  // push with "local push doesn't support non-bare repos" — hard to spot
  // after the fact, easy to catch here.
  const customRemoteLooksLocal = $derived.by(() => {
    if (provider !== 'custom') return false;
    const u = remoteUrl.trim();
    if (!u) return false;
    if (/^(https?|ssh|git|file):\/\//i.test(u)) return false;
    if (/^git@[^:]+:/.test(u)) return false;
    return /^[A-Za-z]:[\\/]/.test(u) || u.startsWith('\\\\') || u.startsWith('/');
  });

  // ── .gitignore options ──────────────────────────────────────────────────────
  const gitignoreOptions = [
    { value: '',       label: 'None' },
    { value: 'rust',   label: 'Rust' },
    { value: 'node',   label: 'Node / JavaScript / TypeScript' },
    { value: 'python', label: 'Python' },
    { value: 'go',     label: 'Go' },
    { value: 'java',   label: 'Java' },
    { value: 'c',      label: 'C' },
    { value: 'cpp',    label: 'C++' },
    { value: 'dotnet', label: '.NET / C#' },
    { value: 'swift',  label: 'Swift' },
    { value: 'ruby',   label: 'Ruby' },
    { value: 'php',    label: 'PHP' },
    { value: 'unity',  label: 'Unity' },
  ];

  // ── License options ─────────────────────────────────────────────────────────
  const licenseOptions = [
    { value: '',             label: 'None' },
    { value: 'mit',          label: 'MIT' },
    { value: 'apache-2.0',   label: 'Apache 2.0' },
    { value: 'gpl-3.0',      label: 'GNU GPL v3' },
    { value: 'lgpl-3.0',     label: 'GNU LGPL v3' },
    { value: 'agpl-3.0',     label: 'GNU AGPL v3' },
    { value: 'bsd-2-clause', label: 'BSD 2-Clause' },
    { value: 'bsd-3-clause', label: 'BSD 3-Clause' },
    { value: 'isc',          label: 'ISC' },
    { value: 'mpl-2.0',      label: 'Mozilla Public License 2.0' },
  ];

  // ── Load git identity ───────────────────────────────────────────────────────
  onMount(async () => {
    try {
      const [name, email] = await getGitIdentity();
      if (name)  authorName  = name;
      if (email) authorEmail = email;
    } catch { /* ignore */ }
    identityLoaded = true;
  });

  const gitignoreItems = $derived<DropdownItem[]>(
    gitignoreOptions.map(o => ({
      kind:    'item',
      id:      o.value || '__none__',
      label:   o.label,
      active:  gitignoreTemplate === o.value,
      onclick: () => { gitignoreTemplate = o.value; },
    })),
  );
  const gitignoreLabel = $derived(
    gitignoreOptions.find(o => o.value === gitignoreTemplate)?.label ?? 'None',
  );

  const licenseItems = $derived<DropdownItem[]>(
    licenseOptions.map(o => ({
      kind:    'item',
      id:      o.value || '__none__',
      label:   o.label,
      active:  license === o.value,
      onclick: () => { license = o.value; },
    })),
  );
  const licenseLabel = $derived(
    licenseOptions.find(o => o.value === license)?.label ?? 'None',
  );

  // ── Submit ──────────────────────────────────────────────────────────────────
  let submitting = $state(false);

  function handleSubmit() {
    if (!isValid || submitting) return;
    submitting = true;
    onInit({
      default_branch:    resolvedBranch,
      description:       description.trim(),
      initial_commit:    initialCommit,
      commit_message:    commitMessage.trim() || 'Initial commit',
      author_name:       authorName.trim(),
      author_email:      authorEmail.trim(),
      gitignore_template: gitignoreTemplate,
      license,
      readme,
      provider,
      visibility,
      org:               org.trim(),
      remote_url:        provider === 'custom' ? remoteUrl.trim() : '',
      push_initial:      provider !== 'none' && initialCommit && pushInitial,
    });
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); handleSubmit(); }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<Modal onClose={onCancel} width="680px" height="640px" padBody={false} ariaLabel="Initialize Git Repository">
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      <div class="header-icon"><GitBranch size={14} /></div>
      <div class="header-text">
        <span class="modal-title">Initialize Git Repository</span>
        <span class="header-path">
          <FolderOpen size={11} />
          {path}
        </span>
      </div>
    </ModalHeader>
  {/snippet}

  <div class="ir-body">
    <!-- ── Tab bar ────────────────────────────────────────────────────────── -->
    <div class="tab-bar">
      <Tabs
        items={[
          { id: 'project', label: 'Project', icon: UserCircle },
          { id: 'files',   label: 'Files',   icon: FileText },
          {
            id:    'remote',
            label: 'Remote',
            icon:  Globe,
            badge: provider !== 'none' ? (provider === 'custom' ? 'URL' : provider) : undefined,
          },
        ]}
        value={activeTab}
        variant="underline"
        size="md"
        ariaLabel="Init repository steps"
        onSelect={setActiveTab}
      />
    </div>

    <!-- ── Body ───────────────────────────────────────────────────────────── -->
    <div class="ir-content">

      <!-- ── Project tab ────────────────────────────────────────────────── -->
      {#if activeTab === 'project'}
        <div class="form-section">
          <div class="section-label">
            <Info size={12} />
            Repository info
          </div>

          <FormField label="Description" optionalText="(optional)">
            <textarea
              class="field-textarea"
              placeholder="Short description of this project…"
              rows="2"
              bind:value={description}
            ></textarea>
          </FormField>

          <FormField label="Default branch">
            <div class="branch-row">
              {#each (['main','master','develop'] as const) as preset}
                <button
                  class="branch-chip"
                  class:selected={branchPreset === preset}
                  onclick={() => { branchPreset = preset; }}
                >
                  {preset}
                </button>
              {/each}
              <button
                class="branch-chip"
                class:selected={branchPreset === 'custom'}
                onclick={() => { branchPreset = 'custom'; }}
              >
                Custom…
              </button>
            </div>
            {#if branchPreset === 'custom'}
              <div class="mt-4">
                <Input
                  placeholder="branch-name"
                  bind:value={customBranch}
                  autofocus
                />
              </div>
            {/if}
          </FormField>
        </div>

        <div class="form-section">
          <div class="section-label">
            <GitBranch size={12} />
            Initial commit
          </div>

          <label class="checkbox-row">
            <input type="checkbox" bind:checked={initialCommit} />
            <span>Create initial commit after init</span>
          </label>

          {#if initialCommit}
            <FormField label="Commit message">
              <Input placeholder="Initial commit" bind:value={commitMessage} />
            </FormField>

            <div class="two-col">
              <FormField label="Author name" optionalText={!identityLoaded ? '(loading…)' : undefined}>
                <Input placeholder="Your Name" bind:value={authorName} />
              </FormField>
              <FormField label="Author email">
                <Input type="email" placeholder="you@example.com" bind:value={authorEmail} />
              </FormField>
            </div>

            {#if identityLoaded && (!authorName || !authorEmail)}
              <div class="info-note">
                <Info size={11} />
                Name/email will fall back to defaults if left empty. Set them in Settings → git config or fill them in above.
              </div>
            {/if}
          {/if}
        </div>

      <!-- ── Files tab ──────────────────────────────────────────────────── -->
      {:else if activeTab === 'files'}
        <div class="form-section">
          <div class="section-label">
            <FileText size={12} />
            Generated files
          </div>

          <label class="checkbox-row">
            <input type="checkbox" bind:checked={readme} />
            <span>Create <code>README.md</code> with project name and description</span>
          </label>

          <FormField label=".gitignore template">
            <div class="select-wrapper">
              <Dropdown
                position="fixed"
                direction="down"
                matchTriggerWidth
                items={gitignoreItems}
              >
                {#snippet trigger({ open, toggle })}
                  <button
                    class="field-select"
                    onclick={toggle}
                    type="button"
                    aria-haspopup="listbox"
                    aria-expanded={open}
                  >
                    <span class="field-select-label">{gitignoreLabel}</span>
                    <ChevronDown size={13} />
                  </button>
                {/snippet}
              </Dropdown>
            </div>
          </FormField>

          <FormField label="License">
            <div class="select-wrapper">
              <Dropdown
                position="fixed"
                direction="down"
                matchTriggerWidth
                items={licenseItems}
              >
                {#snippet trigger({ open, toggle })}
                  <button
                    class="field-select"
                    onclick={toggle}
                    type="button"
                    aria-haspopup="listbox"
                    aria-expanded={open}
                  >
                    <span class="field-select-label">{licenseLabel}</span>
                    <ChevronDown size={13} />
                  </button>
                {/snippet}
              </Dropdown>
            </div>
            {#if license}
              <span class="field-hint">
                <Scale size={11} />
                A <code>LICENSE</code> file will be created with your author name and the current year.
              </span>
            {/if}
          </FormField>

          <div class="files-preview">
            <span class="preview-label">Files that will be created:</span>
            <div class="file-list">
              <span class="file-entry always"><FileText size={11} /> .git/ <span class="badge">always</span></span>
              {#if readme}<span class="file-entry"><Check size={11} /> README.md</span>{/if}
              {#if gitignoreTemplate}<span class="file-entry"><Check size={11} /> .gitignore</span>{/if}
              {#if license}<span class="file-entry"><Check size={11} /> LICENSE</span>{/if}
              {#if initialCommit}<span class="file-entry commit"><GitBranch size={11} /> Initial commit on <code>{resolvedBranch}</code></span>{/if}
            </div>
          </div>
        </div>

      <!-- ── Remote tab ─────────────────────────────────────────────────── -->
      {:else if activeTab === 'remote'}
        <div class="form-section">
          <div class="section-label">
            <Globe size={12} />
            Remote provider
          </div>

          <div class="provider-grid">
            <button
              class="provider-card"
              class:selected={provider === 'none'}
              onclick={() => provider = 'none'}
            >
              <X size={18} />
              <span>None</span>
              <span class="provider-sub">Local only</span>
            </button>
            <button
              class="provider-card"
              class:selected={provider === 'github'}
              onclick={() => provider = 'github'}
            >
              <Github size={18} />
              <span>GitHub</span>
              <span class="provider-sub">github.com</span>
            </button>
            <button
              class="provider-card"
              class:selected={provider === 'gitlab'}
              onclick={() => provider = 'gitlab'}
            >
              <Gitlab size={18} />
              <span>GitLab</span>
              <span class="provider-sub">gitlab.com</span>
            </button>
            <button
              class="provider-card"
              class:selected={provider === 'custom'}
              onclick={() => provider = 'custom'}
            >
              <Link size={18} />
              <span>Custom</span>
              <span class="provider-sub">Any URL</span>
            </button>
          </div>

          <!-- GitHub / GitLab options -->
          {#if provider === 'github' || provider === 'gitlab'}
            <div class="provider-options">
              <FormField label="Visibility">
                <div class="visibility-row">
                  <button
                    class="vis-btn"
                    class:selected={visibility === 'private'}
                    onclick={() => visibility = 'private'}
                  >
                    <Lock size={12} />
                    Private
                  </button>
                  <button
                    class="vis-btn"
                    class:selected={visibility === 'public'}
                    onclick={() => visibility = 'public'}
                  >
                    <Globe size={12} />
                    Public
                  </button>
                </div>
              </FormField>

              <FormField
                label={provider === 'github' ? 'Organization' : 'Group / Namespace'}
                optionalText="(optional — leave empty for personal account)"
              >
                <Input
                  placeholder={provider === 'github' ? 'my-org' : 'my-group'}
                  bind:value={org}
                />
              </FormField>

              <div class="info-note warning">
                <AlertTriangle size={12} />
                {remoteWarning}
              </div>

              <div class="info-note">
                <Info size={11} />
                The remote repository named <strong>{folderName}</strong> will be created on {provider === 'github' ? 'GitHub' : 'GitLab'} and added as <code>origin</code>.
                If creation fails the local repository is still initialized.
              </div>
            </div>

          <!-- Custom URL option -->
          {:else if provider === 'custom'}
            <div class="provider-options">
              <FormField label="Remote URL" optionalText="(added as origin)">
                <Input
                  type="url"
                  placeholder="https://git.example.com/user/repo.git"
                  bind:value={remoteUrl}
                />
              </FormField>
              {#if customRemoteLooksLocal}
                <div class="info-note warning">
                  <AlertTriangle size={11} />
                  This looks like a <strong>local path</strong>. If the target
                  folder is a non-bare repo, a later <code>git push</code> will
                  fail with <code>local push doesn't support non-bare repos</code>.
                  Use an HTTPS/SSH URL, or point at a <code>--bare</code> repo.
                </div>
              {/if}
            </div>
          {/if}

          {#if provider !== 'none'}
            <div class="push-section">
              <label class="checkbox-row">
                <input type="checkbox" bind:checked={pushInitial} disabled={!initialCommit} />
                <span>
                  Push initial commit to <code>origin</code> after init
                  {#if !initialCommit}
                    <span class="optional">(requires an initial commit)</span>
                  {/if}
                </span>
              </label>
              {#if initialCommit && pushInitial}
                <div class="info-note">
                  <Info size={11} />
                  The initial commit will be pushed to <code>origin/{resolvedBranch}</code> and upstream tracking will be set. Requires valid credentials for the remote.
                </div>
              {:else if initialCommit && !pushInitial}
                <div class="info-note warning">
                  <AlertTriangle size={11} />
                  The initial commit will stay <strong>local only</strong>. You'll need to push manually to publish it to the remote.
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>

  </div>

  {#snippet footer()}
    <div class="footer-hint">
      <kbd>Ctrl</kbd>+<kbd>Enter</kbd> to initialize
    </div>
    <div class="footer-actions">
      <Button variant="secondary" onclick={onCancel}>Cancel</Button>
      <Button
        variant="primary"
        disabled={!isValid || submitting}
        loading={submitting}
        onclick={handleSubmit}
      >
        {#snippet iconStart()}
          <Check size={13} />
        {/snippet}
        {submitting ? 'Initializing…' : 'Initialize Repository'}
      </Button>
    </div>
  {/snippet}
</Modal>

<style>
  .ir-body {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  /* ── Header bits ── */
  .header-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    background: var(--accent-subtle);
    color: var(--accent);
    flex-shrink: 0;
  }

  .header-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .header-path {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--text-muted);
    font-family: var(--font-code);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }


  /* ── Tabs ──
     Strip rendered by shared <Tabs variant="underline">. The wrapper just
     contributes the modal's side-padding + bg. */
  .tab-bar {
    background: var(--bg-base);
    flex-shrink: 0;
    padding: 0 8px;
  }

  /* ── Body ── */
  .ir-content {
    flex: 1;
    overflow-y: auto;
    padding: 20px;
    min-height: 0;
  }

  .form-section {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .form-section + .form-section {
    margin-top: 24px;
    padding-top: 20px;
    border-top: 1px solid var(--border-subtle);
  }

  .section-label {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.07em;
  }

  /* ── Fields ── */
  .field-textarea,
  .field-select {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: 12px;
    font-family: var(--font-ui-sans);
    padding: 6px 10px;
    transition: border-color 120ms, box-shadow 120ms;
    box-sizing: border-box;
    cursor: pointer;
    text-align: left;
  }
  .field-select-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .field-select:hover,
  .field-select[aria-expanded='true'] { border-color: var(--accent); }
  .field-textarea:focus,
  .field-select:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-subtle);
  }
  .field-textarea { resize: vertical; min-height: 52px; }

  .select-wrapper { display: flex; }
  .select-wrapper :global(.dd-root) { width: 100%; }

  .field-hint {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .mt-4 { margin-top: 4px; }

  .two-col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  /* ── Branch chips ── */
  .branch-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .branch-chip {
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-base);
    color: var(--text-secondary);
    font-size: 12px;
    font-family: var(--font-code);
    cursor: pointer;
    transition: background 120ms, border-color 120ms, color 120ms;
  }
  .branch-chip:hover { background: var(--bg-hover); color: var(--text-primary); }
  .branch-chip.selected {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }

  /* ── Checkbox ── */
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
    user-select: none;
  }
  .checkbox-row input[type="checkbox"] {
    accent-color: var(--accent);
    width: 14px;
    height: 14px;
    flex-shrink: 0;
  }

  /* ── Files preview ── */
  .files-preview {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 10px 14px;
  }

  .preview-label {
    font-size: 11px;
    color: var(--text-muted);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    display: block;
    margin-bottom: 8px;
  }

  .file-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .file-entry {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
    font-family: var(--font-code);
  }
  .file-entry.always { color: var(--text-muted); }
  .file-entry.commit { font-family: var(--font-ui-sans); color: var(--accent); }

  .badge {
    font-size: 10px;
    font-family: var(--font-ui-sans);
    font-weight: 500;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
  }

  /* ── Provider grid ── */
  .provider-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }

  .provider-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 14px 8px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 120ms, border-color 120ms, color 120ms;
    font-size: 12px;
    font-weight: 500;
    text-align: center;
  }
  .provider-card:hover { background: var(--bg-hover); color: var(--text-primary); }
  .provider-card.selected {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }

  .provider-sub {
    font-size: 10px;
    font-weight: 400;
    color: var(--text-muted);
  }
  .provider-card.selected .provider-sub { color: var(--accent); opacity: 0.7; }

  .provider-options {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .push-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px dashed var(--border-subtle);
  }

  /* ── Visibility row ── */
  .visibility-row {
    display: flex;
    gap: 6px;
  }

  .vis-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
    transition: background 120ms, border-color 120ms, color 120ms;
  }
  .vis-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .vis-btn.selected {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }

  /* ── Info / warning notes ── */
  .info-note {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
    font-size: 11.5px;
    color: var(--text-secondary);
    line-height: 1.5;
  }
  .info-note :global(svg) { flex-shrink: 0; margin-top: 1px; color: var(--accent); }
  .info-note.warning {
    background: color-mix(in srgb, var(--color-warning, #e8a04a) 8%, transparent);
    border-color: color-mix(in srgb, var(--color-warning, #e8a04a) 24%, transparent);
  }
  .info-note.warning :global(svg) { color: var(--color-warning, #e8a04a); }

  /* ── Footer bits ── */
  .footer-hint {
    font-size: 11px;
    color: var(--text-disabled);
    display: flex;
    align-items: center;
    gap: 3px;
    margin-right: auto;
  }

  .footer-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  kbd {
    display: inline-flex;
    align-items: center;
    padding: 1px 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-family: var(--font-code);
    color: var(--text-muted);
    background: var(--bg-overlay);
  }

  code {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--accent);
  }
</style>
