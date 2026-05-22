<script lang="ts">
  import { GitBranch, Download, FolderOpen, RefreshCw, Loader, AlertTriangle, Check, ExternalLink, X } from 'lucide-svelte';
  import { gitCliStore } from '$lib/stores/gitCli.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // The modal is always rendered as a bouncer over the rest of the UI when
  // git is missing.  It can also be opened from Settings to reconfigure or
  // re-download — pass `dismissable={true}` and an `onClose` handler then.
  let { dismissable = false, onClose }: {
    dismissable?: boolean;
    onClose?:     () => void;
  } = $props();

  const isWindows = navigator.userAgent.toLowerCase().includes('windows');

  let pickerOpen = $state(false);

  async function pickGitExecutable(path: string) {
    pickerOpen = false;
    try {
      await gitCliStore.setPath(path);
    } catch {
      // store already captured the error
    }
  }

  async function autoDetect() {
    try { await gitCliStore.refresh(); } catch { /* shown via store */ }
  }

  async function startDownload() {
    try { await gitCliStore.download(); } catch { /* shown via store */ }
  }

  function bytes(n: number): string {
    if (n <= 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    let i = 0;
    let v = n;
    while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
    return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
  }

  const progressPct = $derived.by(() => {
    const p = gitCliStore.progress;
    if (!p || p.total <= 0) return null;
    return Math.min(100, Math.round((p.bytes / p.total) * 100));
  });

  function handleClose() {
    if (dismissable) onClose?.();
  }
</script>

<Modal
  onClose={handleClose}
  width="580px"
  closeOnBackdrop={dismissable}
  ariaLabel="Configure Git Executable"
>
  {#snippet header()}
    <ModalHeader onClose={handleClose} hideClose={!dismissable}>
      <div class="header-icon"><GitBranch size={16} /></div>
      <div class="header-text">
        <span class="modal-title">Git executable required</span>
        <span class="header-sub">
          Arbor uses git2 for most operations but needs the system <code>git</code> binary
          for rebase, stash, submodules, and a few other commands.
        </span>
      </div>
    </ModalHeader>
  {/snippet}

  <div class="gs-body">
    <!-- Current detection state -->
    {#if gitCliStore.phase === 'ready' && gitCliStore.status?.path}
      <div class="status-card ok">
        <div class="status-icon"><Check size={14} /></div>
        <div class="status-text">
          <div class="status-line">
            <span class="version">{gitCliStore.status.version ?? 'git found'}</span>
            <span class="source-pill">source: {gitCliStore.status.source}</span>
          </div>
          <div class="status-path">{gitCliStore.status.path}</div>
        </div>
      </div>
    {:else if gitCliStore.phase === 'detecting'}
      <div class="status-card neutral">
        <div class="status-icon"><Loader size={14} class="spin" /></div>
        <div class="status-text">Looking for git…</div>
      </div>
    {:else if gitCliStore.phase === 'downloading'}
      <div class="status-card neutral">
        <div class="status-icon"><Loader size={14} class="spin" /></div>
        <div class="status-text">
          <div class="status-line">
            <span>{gitCliStore.progress?.message ?? 'Downloading…'}</span>
            <button
              class="cancel-btn"
              onclick={() => gitCliStore.cancel()}
              use:tooltip={'Cancel download'}
              aria-label="Cancel download"
            ><X size={12} /> Cancel</button>
          </div>
          {#if gitCliStore.progress && gitCliStore.progress.total > 0 && (gitCliStore.progress.stage === 'downloading' || gitCliStore.progress.stage === 'extracting')}
            <div class="progress">
              <div class="progress-track">
                <div class="progress-fill" style="width: {progressPct ?? 0}%"></div>
              </div>
              <div class="progress-meta">
                {#if gitCliStore.progress.stage === 'downloading'}
                  {bytes(gitCliStore.progress.bytes)} / {bytes(gitCliStore.progress.total)} ({progressPct}%)
                {:else}
                  {gitCliStore.progress.bytes} / {gitCliStore.progress.total} files ({progressPct}%)
                {/if}
              </div>
            </div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="status-card warn">
        <div class="status-icon"><AlertTriangle size={14} /></div>
        <div class="status-text">
          <div class="status-line">No git executable found.</div>
          {#if gitCliStore.lastError}
            <div class="status-error">{gitCliStore.lastError}</div>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Action buttons -->
    <div class="actions">
      {#if gitCliStore.status?.download_supported}
        <button
          class="action primary"
          onclick={startDownload}
          disabled={gitCliStore.phase === 'downloading'}
        >
          <Download size={14} />
          <div class="action-text">
            <div class="action-title">Download portable git</div>
            <div class="action-sub">Fetches PortableGit from git-for-windows and unpacks it into Arbor's config folder.</div>
          </div>
        </button>
      {:else if !isWindows}
        <div class="action info">
          <ExternalLink size={14} />
          <div class="action-text">
            <div class="action-title">Install git via your package manager</div>
            <div class="action-sub">
              <code>brew install git</code> on macOS, <code>apt install git</code> / <code>dnf install git</code> on Linux,
              then click <em>Auto-detect</em>. Or use <em>Browse</em> if it's already installed in a non-standard location.
            </div>
          </div>
        </div>
      {/if}

      <button
        class="action"
        onclick={() => (pickerOpen = true)}
        disabled={gitCliStore.phase === 'downloading'}
      >
        <FolderOpen size={14} />
        <div class="action-text">
          <div class="action-title">Browse for git executable…</div>
          <div class="action-sub">
            Pick the {isWindows ? 'git.exe' : 'git'} you want Arbor to use. Verified before saving.
          </div>
        </div>
      </button>

      <button
        class="action"
        onclick={autoDetect}
        disabled={gitCliStore.phase === 'detecting' || gitCliStore.phase === 'downloading'}
      >
        <RefreshCw size={14} />
        <div class="action-text">
          <div class="action-title">Auto-detect</div>
          <div class="action-sub">Re-scan PATH and the bundled portable copy.</div>
        </div>
      </button>
    </div>

    <div class="footer-note">
      You can change this any time from <strong>Settings → Git CLI</strong>.
      <button
        class="link-btn"
        onclick={() => openUrl('https://git-scm.com/downloads').catch(() => {})}
      >
        git-scm.com/downloads <ExternalLink size={10} />
      </button>
    </div>
  </div>
</Modal>

{#if pickerOpen}
  <FilePickerModal
    mode="file"
    title="Select Git Executable"
    extensions={isWindows ? ['exe'] : undefined}
    onConfirm={pickGitExecutable}
    onCancel={() => (pickerOpen = false)}
  />
{/if}

<style>
  .header-icon {
    width: 28px; height: 28px;
    border-radius: var(--radius-md);
    background: var(--accent-subtle);
    color: var(--accent);
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0;
  }
  .header-text { display: flex; flex-direction: column; gap: 3px; min-width: 0; flex: 1; }
  .header-sub   { font-size: 11px; color: var(--text-secondary); line-height: 1.4; white-space: normal; }
  .header-sub code { background: var(--bg-base); padding: 1px 4px; border-radius: var(--radius-sm); font-family: var(--font-code); font-size: 10px; }

  .gs-body {
    display: flex; flex-direction: column; gap: 12px;
  }

  .status-card {
    display: flex;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .status-card.ok      { border-color: color-mix(in srgb, var(--success) 40%, transparent); background: color-mix(in srgb, var(--success) 10%, transparent); }
  .status-card.warn    { border-color: color-mix(in srgb, var(--error) 40%, transparent);   background: color-mix(in srgb, var(--error) 10%, transparent); }
  .status-card.neutral { background: var(--bg-overlay); }
  .status-icon { flex-shrink: 0; padding-top: 1px; }
  .status-card.ok      .status-icon { color: var(--success); }
  .status-card.warn    .status-icon { color: var(--error); }
  .status-card.neutral .status-icon { color: var(--text-secondary); }
  .status-text { display: flex; flex-direction: column; gap: 4px; min-width: 0; flex: 1; font-size: 11px; }
  .status-line { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .status-line .version { font-weight: 600; color: var(--text-primary); }
  .status-path { font-family: var(--font-code); font-size: 10px; color: var(--text-secondary); word-break: break-all; }
  .status-error { color: var(--error); font-size: 11px; }
  .source-pill {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
    text-transform: lowercase;
  }

  .status-line { justify-content: space-between; }
  .cancel-btn {
    display: inline-flex; align-items: center; gap: 3px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: 10px;
    cursor: pointer;
    flex-shrink: 0;
    transition: border-color 100ms ease, color 100ms ease, background-color 100ms ease;
  }
  .cancel-btn:hover {
    border-color: var(--error);
    color: var(--error);
    background: color-mix(in srgb, var(--error) 10%, transparent);
  }

  .progress { margin-top: 4px; display: flex; flex-direction: column; gap: 4px; }
  .progress-track {
    width: 100%;
    height: 4px;
    background: var(--bg-base);
    border-radius: 999px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 120ms ease;
  }
  .progress-meta { font-size: 10px; color: var(--text-secondary); font-family: var(--font-code); }

  .actions { display: flex; flex-direction: column; gap: 8px; }
  .action {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px 14px;
    text-align: left;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    cursor: pointer;
    transition: border-color 100ms ease, background-color 100ms ease;
  }
  .action:hover:not(:disabled) { border-color: var(--accent); background: var(--bg-hover); }
  .action:disabled { opacity: 0.5; cursor: not-allowed; }
  .action.primary {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }
  .action.primary:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .action.info { cursor: default; }
  .action.info:hover { border-color: var(--border-subtle); background: var(--bg-elevated); }

  .action-text { display: flex; flex-direction: column; gap: 3px; flex: 1; min-width: 0; }
  .action-title { font-size: 12px; font-weight: 600; }
  .action-sub   { font-size: 11px; color: var(--text-secondary); line-height: 1.4; }
  .action-sub code { font-family: var(--font-code); background: var(--bg-base); padding: 1px 4px; border-radius: var(--radius-sm); font-size: 10px; }

  .footer-note {
    margin-top: 4px;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--text-secondary);
    display: flex; flex-wrap: wrap; gap: 6px; align-items: center;
  }
  .link-btn {
    display: inline-flex; align-items: center; gap: 3px;
    background: transparent; border: none;
    color: var(--accent);
    cursor: pointer;
    padding: 0;
    font-size: 11px;
  }
  .link-btn:hover { text-decoration: underline; }

</style>
