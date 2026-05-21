<script lang="ts">
  import { ShieldCheck, HardDrive, FileWarning, RotateCcw, Plus, X, Clock } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { getRecoveryConfig, setRecoveryConfig } from '$lib/ipc/recovery';
  import type { RecoveryConfig } from '$lib/types/git';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';

  // ── Built-in defaults, mirrored from src-tauri/src/git/recovery.rs so the
  // UI can reset without asking the backend. Kept here (not in types) because
  // the list is a UI default, not part of the data model.
  const DEFAULT_MAX_MB       = 2;
  const DEFAULT_RETENTION    = 30; // days
  const DEFAULT_DENY_EXTS: string[] = [
    'zip','tar','tgz','gz','bz2','xz','7z','rar',
    'mp4','mov','mkv','avi','webm','mp3','wav','flac',
    'psd','ai','tiff',
    'exe','dll','so','dylib','a','lib','obj','o',
    'jar','war','class','pdb','wasm',
    'iso','dmg','pkg','deb','rpm',
    'onnx','pt','pth','ckpt','h5','parquet','safetensors',
  ];

  let cfg     = $state<RecoveryConfig | null>(null);
  let loading = $state(true);
  let error   = $state<string | null>(null);

  // MB representation — user-friendly vs. the byte-value persisted on disk.
  let sizeMb     = $state<number>(DEFAULT_MAX_MB);
  let retention  = $state<number>(DEFAULT_RETENTION);
  let extsStr    = $state<string>('');
  let newExt     = $state<string>('');
  let saved      = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(load);

  async function load() {
    loading = true; error = null;
    try {
      cfg = await getRecoveryConfig();
      sizeMb     = Math.max(0, Math.round(cfg.max_file_size / (1024 * 1024) * 10) / 10);
      retention  = Math.max(0, Math.trunc(cfg.retention_days ?? DEFAULT_RETENTION));
      extsStr    = cfg.deny_extensions.join(', ');
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
    }
  }

  async function persist() {
    if (!cfg) return;
    const next: RecoveryConfig = {
      max_file_size:   Math.max(0, Math.round(sizeMb * 1024 * 1024)),
      deny_extensions: parseExts(extsStr),
      retention_days:  Math.max(0, Math.trunc(retention)),
    };
    try {
      await setRecoveryConfig(next);
      cfg = next;
      saved = true;
      if (saveTimer) clearTimeout(saveTimer);
      saveTimer = setTimeout(() => { saved = false; }, 2000);
    } catch (e) {
      uiStore.showToast(`Save failed: ${e}`, 'error');
    }
  }

  function parseExts(s: string): string[] {
    return s.split(/[,\s]+/)
      .map(x => x.trim().replace(/^\./, '').toLowerCase())
      .filter(Boolean);
  }

  function addExt() {
    const v = newExt.trim().replace(/^\./, '').toLowerCase();
    if (!v) return;
    const current = parseExts(extsStr);
    if (!current.includes(v)) current.push(v);
    extsStr = current.join(', ');
    newExt  = '';
    persist();
  }

  function removeExt(ext: string) {
    const current = parseExts(extsStr).filter(e => e !== ext);
    extsStr = current.join(', ');
    persist();
  }

  function resetDefaults() {
    sizeMb    = DEFAULT_MAX_MB;
    retention = DEFAULT_RETENTION;
    extsStr   = DEFAULT_DENY_EXTS.join(', ');
    persist();
  }

  const currentExts = $derived(parseExts(extsStr));
</script>

<div class="section-header">
  <h2>Recovery Snapshots</h2>
  <p>Automatic safety-net snapshots taken before destructive git operations.
     Configure which files get their content preserved (restorable) vs. only
     logged in the journal (tracked but not restorable).</p>
</div>

{#if loading}
  <div class="state-msg">Loading…</div>
{:else if error}
  <div class="state-msg err">{error}</div>
{:else if cfg}

  <!-- Retention window -->
  <div class="card">
    <div class="card-section-title"><Clock size={12} /> Retention</div>

    <FormRow
      label="Keep snapshots for (days)"
      description="Older snapshots are dropped automatically the next time the Recovery panel is opened. Set to 0 to disable time-based expiry — the 500-entry cap still prevents unbounded growth."
    >
      <NumberStepper
        bind:value={retention}
        min={0}
        step={1}
        ariaLabel="Snapshot retention (days)"
        onchange={persist}
      />
    </FormRow>
  </div>

  <!-- Size limit -->
  <div class="card">
    <div class="card-section-title"><HardDrive size={12} /> Size policy</div>

    <FormRow
      label="Max file size (MB)"
      description="Files larger than this are logged in the journal but their bytes are not preserved — so they cannot be restored. Keeps the .git store from bloating."
    >
      <NumberStepper
        bind:value={sizeMb}
        min={0}
        step={0.5}
        ariaLabel="Max file size (MB)"
        onchange={persist}
      />
    </FormRow>
  </div>

  <!-- Extension deny list -->
  <div class="card">
    <div class="card-section-title"><FileWarning size={12} /> Excluded extensions</div>

    <div class="row-hint block-hint">
      Files matching any of these extensions are logged but not preserved,
      regardless of size. Intended for binaries, build outputs, archives and
      large ML blobs where a restore would rarely be meaningful.
    </div>

    <div class="chip-list">
      {#each currentExts as ext (ext)}
        <span class="ext-chip">
          .{ext}
          <button class="ext-x" onclick={() => removeExt(ext)} use:tooltip={'Remove'}>
            <X size={10} />
          </button>
        </span>
      {/each}
      {#if currentExts.length === 0}
        <span class="empty-exts">No extensions excluded — only the size limit applies.</span>
      {/if}
    </div>

    <div class="add-row">
      <input
        class="row-input"
        type="text"
        placeholder="Add extension (e.g. mp4)"
        bind:value={newExt}
        onkeydown={(e) => { if (e.key === 'Enter') addExt(); }}
      />
      <button class="btn" onclick={addExt} disabled={!newExt.trim()}>
        <Plus size={11} /> Add
      </button>
    </div>
  </div>

  <!-- Reset -->
  <div class="card footer-card">
    <button class="btn btn-ghost" onclick={resetDefaults}>
      <RotateCcw size={11} /> Reset to defaults
    </button>
    {#if saved}
      <span class="saved-pill"><ShieldCheck size={11} /> Saved</span>
    {/if}
  </div>

{/if}

<style>
  .section-header {
    margin-bottom: 14px;
  }
  .section-header h2 {
    font-size: 15px; font-weight: 600; margin: 0 0 3px;
    color: var(--text-primary); font-family: var(--font-ui-sans);
  }
  .section-header p {
    margin: 0; font-size: 12px; color: var(--text-secondary); line-height: 1.45;
    font-family: var(--font-ui-sans);
  }

  .card {
    padding: 10px 12px; margin-bottom: 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-family: var(--font-ui-sans);
  }
  .card-section-title {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: 11px; font-weight: 600; color: var(--text-secondary);
    text-transform: uppercase; letter-spacing: 0.5px;
    margin-bottom: 8px;
  }

  .row-hint {
    display: block; font-size: 11px; color: var(--text-muted);
    line-height: 1.4;
  }
  .block-hint { margin-bottom: 8px; }

  .row-input {
    padding: 4px 8px; font-size: 12px; color: var(--text-primary);
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); outline: none;
    min-width: 110px; font-family: var(--font-ui-sans);
  }
  .row-input:focus { border-color: var(--accent); }

  .chip-list {
    display: flex; flex-wrap: wrap; gap: 4px;
    min-height: 24px;
    padding: 4px 0 8px;
  }
  .ext-chip {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 2px 4px 2px 7px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-family: var(--font-code);
    font-size: 10.5px; color: var(--text-secondary);
  }
  .ext-x {
    display: flex; align-items: center; justify-content: center;
    width: 14px; height: 14px;
    border: none; background: transparent;
    border-radius: 50%; cursor: pointer; color: var(--text-muted);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ext-x:hover { background: var(--bg-hover); color: var(--color-error, #e06c75); }
  .empty-exts { font-size: 11px; color: var(--text-muted); font-style: italic; }

  .add-row {
    display: flex; gap: 6px; align-items: center;
    padding-top: 6px;
    border-top: 1px dashed var(--border-subtle);
  }

  .btn {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 4px 10px; font-size: 11.5px; font-weight: 500;
    background: var(--accent-subtle); color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
    border-radius: var(--radius-sm); cursor: pointer;
    font-family: var(--font-ui-sans);
    transition: all var(--transition-fast);
  }
  .btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-ghost {
    background: transparent; color: var(--text-secondary);
    border-color: var(--border-subtle);
  }
  .btn-ghost:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border); }

  .footer-card {
    display: flex; align-items: center; gap: 10px;
    margin-top: 6px;
  }
  .saved-pill {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: 11px; color: var(--accent);
  }

  .state-msg {
    padding: 20px; color: var(--text-muted);
    font-size: 12px; font-family: var(--font-ui-sans);
    text-align: center;
  }
  .state-msg.err { color: var(--color-error, #e06c75); }
</style>
