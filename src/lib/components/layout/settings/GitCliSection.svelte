<script lang="ts">
  import { onMount } from 'svelte';
  import { Loader, FolderOpen, RefreshCw, Download, Check, AlertTriangle, Trash2, X } from 'lucide-svelte';
  import { gitCliStore } from '$lib/stores/gitCli.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  const isWindows = navigator.userAgent.toLowerCase().includes('windows');

  let pickerOpen = $state(false);
  let busy       = $state(false);

  onMount(async () => {
    // The store is already populated from AppShell's onMount, but if the
    // user opens Settings from a fresh session that bypassed it (HMR), make
    // sure we have current data.
    if (!gitCliStore.status) await gitCliStore.init();
  });

  async function chooseFile(path: string) {
    pickerOpen = false;
    busy = true;
    try {
      await gitCliStore.setPath(path);
      uiStore.showToast('Git path updated', 'success');
    } catch (e) {
      uiStore.showToast(`Could not use ${path}: ${e}`, 'error');
    } finally { busy = false; }
  }

  async function clearOverride() {
    busy = true;
    try {
      await gitCliStore.setPath(null);
      uiStore.showToast('Override cleared — using auto-detected git', 'success');
    } catch (e) {
      uiStore.showToast(`Failed: ${e}`, 'error');
    } finally { busy = false; }
  }

  async function autoDetect() {
    busy = true;
    try {
      const s = await gitCliStore.refresh();
      uiStore.showToast(s.path ? `Detected: ${s.version}` : 'No git found on PATH', s.path ? 'success' : 'warning');
    } catch (e) {
      uiStore.showToast(`Auto-detect failed: ${e}`, 'error');
    } finally { busy = false; }
  }

  async function downloadPortable() {
    busy = true;
    try {
      const s = await gitCliStore.download();
      uiStore.showToast(`PortableGit installed: ${s.version}`, 'success');
    } catch (e) {
      uiStore.showToast(`Download failed: ${e}`, 'error');
    } finally { busy = false; }
  }

  function bytes(n: number): string {
    if (n <= 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    let i = 0; let v = n;
    while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
    return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
  }

  const progressPct = $derived.by(() => {
    const p = gitCliStore.progress;
    if (!p || p.total <= 0) return null;
    return Math.min(100, Math.round((p.bytes / p.total) * 100));
  });
</script>

<SectionHeader
  title="Git Executable"
  description="Path to the system `git` binary used by Arbor for rebase, stash, submodules, and a few other operations git2 doesn't fully cover."
/>

<div class="card">
  {#if !gitCliStore.status}
    <div class="empty"><Loader size={13} class="spin" /> Loading…</div>
  {:else}
    <div class="status-row" class:ok={gitCliStore.status.path} class:warn={!gitCliStore.status.path}>
      <div class="status-icon">
        {#if gitCliStore.status.path}<Check size={14} />{:else}<AlertTriangle size={14} />{/if}
      </div>
      <div class="status-text">
        {#if gitCliStore.status.path}
          <div class="row">
            <span class="version">{gitCliStore.status.version ?? 'git found'}</span>
            <span class="pill">source: {gitCliStore.status.source}</span>
          </div>
          <code class="path">{gitCliStore.status.path}</code>
        {:else}
          <div class="row">
            <span class="version">No git executable detected</span>
          </div>
          <span class="muted">Arbor cannot run rebase / stash / submodule operations until git is found.</span>
        {/if}
      </div>
    </div>
  {/if}

  <FormRow
    label="Override path"
    description="Pick a specific git executable.  Verified before saving — if the path doesn't run, the override is rejected."
  >
    <div class="actions-row">
      <button class="btn-secondary" onclick={() => (pickerOpen = true)} disabled={busy}>
        <FolderOpen size={13} /> Browse…
      </button>
      {#if gitCliStore.status?.source === 'config'}
        <button class="btn-secondary" onclick={clearOverride} disabled={busy} use:tooltip={'Clear the override and fall back to PATH / portable copy'}>
          <Trash2 size={13} /> Clear override
        </button>
      {/if}
    </div>
  </FormRow>

  <FormRow
    label="Auto-detect"
    description="Re-scan PATH and the bundled portable copy.  Useful after installing git system-wide while Arbor is open."
  >
    <button class="btn-secondary" onclick={autoDetect} disabled={busy}>
      <RefreshCw size={13} /> Re-detect
    </button>
  </FormRow>

  {#if gitCliStore.status?.download_supported}
    <FormRow
      label="Portable git"
      description={`Downloads the latest PortableGit from git-for-windows and unpacks it to ${gitCliStore.status?.portable_dir ?? ''}.`}
    >
      <button class="btn-secondary" onclick={downloadPortable} disabled={busy || gitCliStore.phase === 'downloading'}>
        {#if gitCliStore.phase === 'downloading'}
          <Loader size={13} class="spin" /> Downloading…
        {:else}
          <Download size={13} /> Download portable
        {/if}
      </button>
    </FormRow>
  {/if}

  {#if gitCliStore.phase === 'downloading' && gitCliStore.progress}
    <div class="progress-card">
      <div class="progress-head">
        <span class="progress-line">{gitCliStore.progress.message}</span>
        <button
          class="cancel-btn"
          onclick={() => gitCliStore.cancel()}
          use:tooltip={'Cancel download'}
          aria-label="Cancel download"
        ><X size={11} /> Cancel</button>
      </div>
      {#if gitCliStore.progress.total > 0 && (gitCliStore.progress.stage === 'downloading' || gitCliStore.progress.stage === 'extracting')}
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
      {/if}
    </div>
  {/if}

  {#if gitCliStore.lastError}
    <div class="error-card">
      <AlertTriangle size={13} /> {gitCliStore.lastError}
    </div>
  {/if}
</div>

{#if pickerOpen}
  <FilePickerModal
    mode="file"
    title="Select Git Executable"
    extensions={isWindows ? ['exe'] : undefined}
    onConfirm={chooseFile}
    onCancel={() => (pickerOpen = false)}
  />
{/if}

<style>
  .empty {
    display: flex; align-items: center; gap: 6px;
    padding: 10px;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
  }

  .status-row {
    display: flex; gap: 10px; align-items: flex-start;
    padding: 10px 12px;
    margin-bottom: 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .status-row.ok   { border-color: color-mix(in srgb, var(--success) 40%, transparent); background: color-mix(in srgb, var(--success) 8%, transparent); }
  .status-row.warn { border-color: color-mix(in srgb, var(--error) 40%, transparent);   background: color-mix(in srgb, var(--error) 8%, transparent); }
  .status-row.ok   .status-icon { color: var(--success); }
  .status-row.warn .status-icon { color: var(--error); }
  .status-icon { padding-top: 1px; }
  .status-text { display: flex; flex-direction: column; gap: 4px; flex: 1; min-width: 0; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .version { font-weight: 600; font-size: var(--font-size-sm); color: var(--text-primary); }
  .pill {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
    text-transform: lowercase;
  }
  .path {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-secondary);
    word-break: break-all;
  }
  .muted { color: var(--text-secondary); font-size: var(--font-size-xs); }

  .actions-row { display: flex; gap: 6px; flex-wrap: wrap; }

  .btn-secondary {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 6px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .btn-secondary:hover:not(:disabled) { background: var(--bg-hover); border-color: var(--accent); }
  .btn-secondary:disabled { opacity: 0.55; cursor: not-allowed; }

  .progress-card {
    margin-top: 12px;
    padding: 10px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    display: flex; flex-direction: column; gap: 6px;
  }
  .progress-head {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
  }
  .progress-line { font-size: var(--font-size-sm); color: var(--text-primary); }
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
  .progress-meta { font-size: 10px; font-family: var(--font-code); color: var(--text-secondary); }

  .error-card {
    margin-top: 12px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: rgba(248, 81, 73, 0.08);
    border-left: 3px solid #f85149;
    color: var(--error);
    font-size: var(--font-size-xs);
    display: flex; gap: 6px; align-items: center;
  }

</style>
