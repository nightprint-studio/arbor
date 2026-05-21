<script lang="ts">
  import { FolderGit2, Plus, X, FileCode2, FolderOpen, FileX } from 'lucide-svelte';
  import { tabsStore }                    from '$lib/stores/tabs.svelte';
  import { uiStore }                      from '$lib/stores/ui.svelte';
  import { getRepoConfig, setRepoConfig } from '$lib/ipc/config';
  import type { RepoConfig }              from '$lib/ipc/config';

  const tab = $derived(tabsStore.activeTab);

  let config  = $state<RepoConfig | null>(null);
  let loading = $state(false);
  let saving  = $state(false);
  let dirty   = $state(false);
  let error   = $state('');

  let newExt    = $state('');
  let newFolder = $state('');
  let newFile   = $state('');

  $effect(() => {
    if (tab) {
      loading = true;
      error   = '';
      getRepoConfig(tab.id)
        .then(cfg => {
          cfg.stats_exclude = {
            extensions: cfg.stats_exclude?.extensions ?? [],
            folders:    cfg.stats_exclude?.folders    ?? [],
            files:      cfg.stats_exclude?.files      ?? [],
          };
          config = cfg;
          dirty  = false;
        })
        .catch(e => { error = String(e); })
        .finally(() => { loading = false; });
    }
  });

  async function save() {
    if (!tab || !config) return;
    saving = true;
    error  = '';
    try {
      await setRepoConfig(tab.id, config);
      dirty = false;
      uiStore.showToast('Saved — click Recompute in the Stats panel to apply.', 'success');
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function addExt() {
    if (!config || !newExt.trim()) return;
    const val = newExt.trim().toLowerCase().replace(/^\.+/, '');
    if (!config.stats_exclude!.extensions.includes(val)) {
      config.stats_exclude!.extensions = [...config.stats_exclude!.extensions, val];
      dirty = true;
    }
    newExt = '';
  }
  function removeExt(ext: string) {
    if (!config) return;
    config.stats_exclude!.extensions = config.stats_exclude!.extensions.filter(e => e !== ext);
    dirty = true;
  }

  function addFolder() {
    if (!config || !newFolder.trim()) return;
    const val = newFolder.trim().replace(/\/$/, '');
    if (!config.stats_exclude!.folders.includes(val)) {
      config.stats_exclude!.folders = [...config.stats_exclude!.folders, val];
      dirty = true;
    }
    newFolder = '';
  }
  function removeFolder(f: string) {
    if (!config) return;
    config.stats_exclude!.folders = config.stats_exclude!.folders.filter(x => x !== f);
    dirty = true;
  }

  function addFile() {
    if (!config || !newFile.trim()) return;
    const val = newFile.trim();
    if (!config.stats_exclude!.files.includes(val)) {
      config.stats_exclude!.files = [...config.stats_exclude!.files, val];
      dirty = true;
    }
    newFile = '';
  }
  function removeFile(f: string) {
    if (!config) return;
    config.stats_exclude!.files = config.stats_exclude!.files.filter(x => x !== f);
    dirty = true;
  }

  function onKey(e: KeyboardEvent, fn: () => void) {
    if (e.key === 'Enter') { e.preventDefault(); fn(); }
  }
</script>

<div class="section-header">
  <h2>Statistics</h2>
  <p>Files excluded from statistics computation. Changes apply after <strong>Recompute</strong>.</p>
</div>

{#if !tab}
  <div class="empty-state">
    <FolderGit2 size={20} />
    <span>No repository open</span>
  </div>

{:else if loading}
  <div class="empty-state"><span>Loading…</span></div>

{:else if config}

  <div class="card">

    <!-- Extensions -->
    <div class="excl-row">
      <div class="excl-label">
        <div class="excl-icon excl-ext"><FileCode2 size={14} /></div>
        <div class="excl-meta">
          <span class="excl-title">Extensions</span>
          <span class="excl-hint">e.g. <code>ron</code>, <code>lock</code></span>
        </div>
      </div>
      <div class="excl-right">
        <div class="chip-wrap">
          {#each config.stats_exclude!.extensions as ext}
            <div class="chip chip-ext">
              <span>.{ext}</span>
              <button class="chip-x" onclick={() => removeExt(ext)}><X size={10} /></button>
            </div>
          {/each}
          <div class="inline-add">
            <input class="inline-input" placeholder="add…" bind:value={newExt}
              onkeydown={(e) => onKey(e, addExt)} />
            <button class="inline-btn" onclick={addExt} disabled={!newExt.trim()}>
              <Plus size={12} />
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="row-divider"></div>

    <!-- Folders -->
    <div class="excl-row">
      <div class="excl-label">
        <div class="excl-icon excl-folder"><FolderOpen size={14} /></div>
        <div class="excl-meta">
          <span class="excl-title">Folders</span>
          <span class="excl-hint">e.g. <code>assets/generated</code></span>
        </div>
      </div>
      <div class="excl-right">
        <div class="chip-wrap">
          {#each config.stats_exclude!.folders as folder}
            <div class="chip chip-folder">
              <span>{folder}/</span>
              <button class="chip-x" onclick={() => removeFolder(folder)}><X size={10} /></button>
            </div>
          {/each}
          <div class="inline-add">
            <input class="inline-input" placeholder="add…" bind:value={newFolder}
              onkeydown={(e) => onKey(e, addFolder)} />
            <button class="inline-btn" onclick={addFolder} disabled={!newFolder.trim()}>
              <Plus size={12} />
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="row-divider"></div>

    <!-- Files -->
    <div class="excl-row">
      <div class="excl-label">
        <div class="excl-icon excl-file"><FileX size={14} /></div>
        <div class="excl-meta">
          <span class="excl-title">Files</span>
          <span class="excl-hint">e.g. <code>Cargo.lock</code></span>
        </div>
      </div>
      <div class="excl-right">
        <div class="chip-wrap">
          {#each config.stats_exclude!.files as file}
            <div class="chip chip-file">
              <span>{file}</span>
              <button class="chip-x" onclick={() => removeFile(file)}><X size={10} /></button>
            </div>
          {/each}
          <div class="inline-add">
            <input class="inline-input" placeholder="add…" bind:value={newFile}
              onkeydown={(e) => onKey(e, addFile)} />
            <button class="inline-btn" onclick={addFile} disabled={!newFile.trim()}>
              <Plus size={12} />
            </button>
          </div>
        </div>
      </div>
    </div>

  </div>

  {#if error}<p class="save-error">{error}</p>{/if}

  <div class="save-row">
    <button class="save-btn" onclick={save} disabled={!dirty || saving}>
      {saving ? 'Saving…' : 'Save'}
    </button>
    {#if !dirty && !saving}
      <span class="saved-hint">All changes saved</span>
    {/if}
  </div>

{/if}

<style>
  /* ── Row layout ──────────────────────────────────────────────────────────── */
  .excl-row {
    display: flex;
    align-items: flex-start;
    gap: 16px;
    padding: 14px 16px;
  }

  .excl-label {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 160px;
    flex-shrink: 0;
  }

  .excl-icon {
    width: 28px; height: 28px;
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0;
    color: var(--text-muted);
  }
  .excl-meta {
    display: flex; flex-direction: column; gap: 2px;
  }
  .excl-title {
    font-size: 13px; font-weight: 600;
    color: var(--text-primary); font-family: var(--font-ui-sans);
  }
  .excl-hint {
    font-size: 11px; color: var(--text-disabled); font-family: var(--font-ui-sans);
  }
  .excl-hint code {
    font-family: var(--font-code); font-size: 10px;
    background: var(--bg-hover); padding: 1px 4px; border-radius: var(--radius-sm);
    color: var(--text-muted);
  }

  .excl-right { flex: 1; min-width: 0; }

  .row-divider {
    height: 1px; background: var(--border-subtle); margin: 0 16px;
  }

  /* ── Chips + inline add ──────────────────────────────────────────────────── */
  .chip-wrap {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    min-height: 30px;
  }

  .chip {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 3px 4px 3px 8px;
    border-radius: var(--radius-sm);
    font-size: 12px; font-family: var(--font-code);
    border: 1px solid transparent;
  }
  .chip-ext    { background: hsl(220,35%,16%); border-color: hsl(220,40%,24%); color: hsl(220,75%,70%); }
  .chip-folder { background: hsl(160,30%,14%); border-color: hsl(160,38%,22%); color: hsl(160,60%,55%); }
  .chip-file   { background: hsl(210,30%,14%); border-color: hsl(210,38%,22%); color: hsl(210,70%,65%); }

  .chip-x {
    display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px;
    background: transparent; border: none;
    color: var(--text-muted); cursor: pointer; border-radius: 2px; padding: 0;
  }
  .chip-x:hover { background: rgba(255,255,255,0.08); color: var(--text-primary); }

  /* ── Inline add ──────────────────────────────────────────────────────────── */
  .inline-add {
    display: flex; align-items: center;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    overflow: hidden;
    height: 28px;
  }
  .inline-input {
    width: 110px; height: 100%;
    background: transparent; border: none; outline: none;
    padding: 0 8px;
    font-size: 12px; font-family: var(--font-code);
    color: var(--text-primary);
  }
  .inline-input::placeholder { color: var(--text-disabled); }
  .inline-add:focus-within { border-color: var(--accent); }

  .inline-btn {
    display: flex; align-items: center; justify-content: center;
    width: 28px; height: 100%;
    background: transparent; border: none; border-left: 1px solid var(--border);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .inline-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .inline-btn:disabled { opacity: 0.35; cursor: not-allowed; }

  /* ── Save ────────────────────────────────────────────────────────────────── */
  .save-row { display: flex; align-items: center; gap: 12px; padding-top: 4px; }
  .save-btn {
    padding: 7px 20px; background: var(--accent); color: var(--text-on-accent);
    border: none; border-radius: var(--radius-sm);
    font-size: 13px; font-family: var(--font-ui-sans); font-weight: 600;
    cursor: pointer; transition: background var(--transition-fast), opacity var(--transition-fast);
  }
  .save-btn:hover:not(:disabled) { background: var(--accent-hover, color-mix(in srgb, var(--accent) 85%, white)); }
  .save-btn:disabled { opacity: 0.45; cursor: not-allowed; }
  .saved-hint { font-size: 11px; color: var(--text-disabled); font-family: var(--font-ui-sans); }
  .save-error { font-size: 12px; color: var(--diff-del-bg-strong, #e87474); font-family: var(--font-ui-sans); margin: 0 0 8px; }

  /* ── Empty state ─────────────────────────────────────────────────────────── */
  .empty-state {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; padding: 48px 16px;
    color: var(--text-muted); font-size: 13px; font-family: var(--font-ui-sans);
  }
</style>
