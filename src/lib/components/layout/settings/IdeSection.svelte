<script lang="ts">
  import { ExternalLink, Plus, Trash2, Check, RefreshCw, CircleCheck, CircleX, FolderOpen } from 'lucide-svelte';
  import Icon from '@iconify/svelte';
  import { onMount } from 'svelte';
  import { worktreeStore } from '$lib/stores/worktree.svelte';
  import { getIdeConfig, setIdeConfig, startIdeDetection } from '$lib/ipc/worktree';
  import { uiStore } from '$lib/stores/ui.svelte';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import type { IdeConfig, IdeEntry } from '$lib/types/git';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  // Single source of truth for the IDE catalogue + project-type rows.
  // Lives in `$lib/constants/ide` so other settings pages
  // (ExternalIntegrationsSection, future per-project pickers) read the
  // same data — never duplicate this list inline.
  import { BUILTIN_IDES, PROJECT_TYPES } from '$lib/constants/ide';

  // ── State ─────────────────────────────────────────────────────────────────
  // Read initial values from the store (populated at startup — no extra call).
  let config    = $state<IdeConfig>(
    worktreeStore.ideConfig ?? { default_ide: 'vscode', custom_ides: [], path_overrides: {}, language_defaults: {} }
  );
  let detecting = $state(false);
  let saving    = $state(false);

  // Keep local config in sync if the store changes while panel is open
  $effect(() => {
    if (worktreeStore.ideConfig) config = { ...worktreeStore.ideConfig };
  });

  // Reactive references to store data (read-only, no detection triggered here)
  const detected = $derived(worktreeStore.detectedIdes);

  // New custom IDE form
  let addOpen    = $state(false);
  let newId      = $state('');
  let newName    = $state('');
  let newCommand = $state('');
  let newArgs    = $state('');
  let filePickerIdeId        = $state('');
  /** Snapshot of path_overrides at last save — used to detect changes. */
  let savedPathOverrides = $state<Record<string, string>>({});

  onMount(async () => {
    // Just read the latest persisted config from disk.
    // Detection is NOT triggered here — it runs once at startup via AppShell.
    try {
      const cfg = await getIdeConfig();
      config = cfg;
      savedPathOverrides = { ...cfg.path_overrides };
      worktreeStore.setIdeConfig(cfg);
    } catch { /* keep defaults */ }
  });

  // ── Helpers ───────────────────────────────────────────────────────────────

  function detectionFor(id: string) {
    return detected.find(d => d.id === id);
  }

  async function runDetection() {
    detecting = true;
    try {
      // Fire a new background detection job; results arrive via the store listener.
      await startIdeDetection();
    } catch { /* ignore */ } finally {
      detecting = false;
    }
  }

  function pickPath(ideId: string) {
    filePickerIdeId = ideId;
  }

  function setPathOverride(ideId: string, value: string) {
    const overrides = { ...config.path_overrides };
    if (value.trim()) overrides[ideId] = value.trim();
    else delete overrides[ideId];
    config = { ...config, path_overrides: overrides };
  }

  function setLanguageDefault(projectType: string, ideId: string) {
    const ld = { ...config.language_defaults };
    if (ideId) ld[projectType] = ideId;
    else delete ld[projectType];
    config = { ...config, language_defaults: ld };
  }

  async function save() {
    saving = true;
    try {
      const pathsChanged =
        JSON.stringify(config.path_overrides) !== JSON.stringify(savedPathOverrides);
      await setIdeConfig(config);
      worktreeStore.setIdeConfig(config);
      // Re-run detection only if path overrides actually changed.
      if (pathsChanged) {
        savedPathOverrides = { ...config.path_overrides };
        await startIdeDetection();
      }
      uiStore.showToast('IDE settings saved', 'success');
    } catch (e) {
      uiStore.showToast(`Save failed: ${e}`, 'error');
    } finally {
      saving = false;
    }
  }

  function addCustomIde() {
    if (!newId.trim() || !newName.trim() || !newCommand.trim()) return;
    const entry: IdeEntry = {
      id:      newId.trim(),
      name:    newName.trim(),
      command: newCommand.trim(),
      args:    newArgs.split(' ').map(s => s.trim()).filter(Boolean),
    };
    config = { ...config, custom_ides: [...config.custom_ides, entry] };
    newId = newName = newCommand = newArgs = '';
    addOpen = false;
  }

  function removeCustomIde(id: string) {
    config = { ...config, custom_ides: config.custom_ides.filter(e => e.id !== id) };
    if (config.default_ide === id) config = { ...config, default_ide: 'vscode' };
  }

  function setDefault(id: string) {
    config = { ...config, default_ide: id };
  }

  const allIdeOptions = $derived([
    ...BUILTIN_IDES,
    ...config.custom_ides.map(c => ({ id: c.id, name: c.name, command: c.command })),
  ]);
</script>

{#if filePickerIdeId}
  <FilePickerModal
    mode="file"
    title="Select IDE Executable"
    onConfirm={(path) => {
      config = { ...config, path_overrides: { ...config.path_overrides, [filePickerIdeId]: path } };
      filePickerIdeId = '';
    }}
    onCancel={() => { filePickerIdeId = ''; }}
  />
{/if}

<!-- ── Default IDE ── -->
<SectionHeader title="IDE Integration" description="Configure which IDE opens when you use Open in IDE from a workspace." />

<div class="card">
  <FormRow label="Default IDE" description="Fallback when no language-specific default is set">
    <Select
      value={config.default_ide}
      options={allIdeOptions.map(ide => ({ value: ide.id, label: ide.name }))}
      onchange={(v) => setDefault(v)}
    />
  </FormRow>
</div>

<!-- ── Per-language defaults ── -->
<div class="card">
  <div class="card-section-title">IDE by Language</div>
  <div class="lang-hint">Override the default IDE for specific project types.</div>
  {#each PROJECT_TYPES as pt}
    {@const currentIde = config.language_defaults[pt.id] ?? ''}
    <div class="lang-row">
      <span class="lang-icon" style="color: {pt.color}"><Icon icon={pt.iconify} width="14" height="14" /></span>
      <span class="lang-label">{pt.label}</span>
      <Select
        value={currentIde}
        options={[
          { value: '', label: '— use default —' },
          ...allIdeOptions.map(ide => ({ value: ide.id, label: ide.name })),
        ]}
        onchange={(v) => setLanguageDefault(pt.id, v)}
      />
    </div>
  {/each}
</div>

<!-- ── Built-in IDEs ── -->
<div class="card">
  <div class="card-section-title">
    Executable Paths
    <button
      class="refresh-btn"
      onclick={runDetection}
      disabled={detecting}
      use:tooltip={'Re-run detection'}
    >
      <RefreshCw size={11} class={detecting ? 'spin' : ''} />
      {detecting ? 'Detecting…' : 'Re-detect'}
    </button>
  </div>
  <div class="detection-hint">
    Paths are detected once at startup. Set a custom path if an IDE is installed in a non-standard location.
  </div>

  {#each BUILTIN_IDES as ide}
    {@const det = detectionFor(ide.id)}
    {@const override = config.path_overrides[ide.id] ?? ''}
    <div class="ide-row" class:is-default={config.default_ide === ide.id}>
      <span
        class="status-dot"
        class:dot-ok={det?.available}
        class:dot-no={det && !det.available}
        class:dot-unknown={!det}
        use:tooltip={det?.available
          ? `Found: ${det.detected_path ?? ide.command}`
          : det ? 'Not found in PATH' : 'Not yet detected'}
      >
        {#if det?.available}
          <CircleCheck size={13} />
        {:else if det}
          <CircleX size={13} />
        {:else}
          <span class="dot-pending">·</span>
        {/if}
      </span>

      <div class="ide-name-col">
        <span class="ide-name">{ide.name}</span>
        <span class="ide-cmd">{override || det?.detected_path || ide.command}</span>
      </div>

      <div class="ide-path-col">
        <div class="path-input-row">
          <input
            class="input input-path"
            type="text"
            placeholder={det?.detected_path ?? `Path to ${ide.command}…`}
            value={override}
            oninput={(e) => setPathOverride(ide.id, (e.target as HTMLInputElement).value)}
          />
          <button class="browse-btn" onclick={() => pickPath(ide.id)} use:tooltip={'Browse…'}>
            <FolderOpen size={12} />
          </button>
        </div>
      </div>

      <button
        class="default-btn"
        class:is-active={config.default_ide === ide.id}
        onclick={() => setDefault(ide.id)}
        use:tooltip={config.default_ide === ide.id ? 'Default IDE' : 'Set as default'}
        disabled={config.default_ide === ide.id}
      >
        <Check size={11} />
      </button>
    </div>
  {/each}
</div>

<!-- ── Custom IDEs ── -->
<div class="card">
  <div class="card-section-title">
    Custom IDEs
    <button class="add-ide-btn" onclick={() => addOpen = !addOpen}>
      <Plus size={11} /> Add
    </button>
  </div>

  {#if addOpen}
    <div class="add-form">
      <div class="add-form-grid">
        <FormField label="ID (unique key)" for="new-ide-id">
          <input id="new-ide-id" class="input" placeholder="my-editor" bind:value={newId} />
        </FormField>
        <FormField label="Display name" for="new-ide-name">
          <input id="new-ide-name" class="input" placeholder="My Editor" bind:value={newName} />
        </FormField>
        <FormField label="Command / full path" for="new-ide-cmd">
          <input id="new-ide-cmd" class="input" placeholder="/usr/bin/myeditor" bind:value={newCommand} />
        </FormField>
        <FormField label="Extra args (space-separated)" for="new-ide-args">
          <input id="new-ide-args" class="input" placeholder="--new-window" bind:value={newArgs} />
        </FormField>
      </div>
      <div class="add-form-footer">
        <button class="btn btn-primary" onclick={addCustomIde} disabled={!newId || !newName || !newCommand}>
          Add IDE
        </button>
        <button class="btn btn-ghost" onclick={() => addOpen = false}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if config.custom_ides.length === 0 && !addOpen}
    <div class="empty-msg">No custom IDEs. Click "Add" to define one.</div>
  {:else}
    {#each config.custom_ides as entry (entry.id)}
      <div class="custom-ide-row">
        <div class="custom-ide-info">
          <span class="custom-ide-name">{entry.name}</span>
          <span class="custom-ide-cmd">{entry.command} {entry.args.join(' ')}</span>
        </div>
        <div class="custom-ide-actions">
          {#if config.default_ide === entry.id}
            <span class="default-pill">default</span>
          {:else}
            <button class="action-btn" onclick={() => setDefault(entry.id)} use:tooltip={'Set as default'}>
              <Check size={11} />
            </button>
          {/if}
          <button class="action-btn danger" onclick={() => removeCustomIde(entry.id)} use:tooltip={'Remove'}>
            <Trash2 size={11} />
          </button>
        </div>
      </div>
    {/each}
  {/if}
</div>

<div class="save-row">
  <button class="btn btn-primary" onclick={save} disabled={saving}>
    {saving ? 'Saving…' : 'Save'}
  </button>
</div>

<style>
  .card {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    margin-bottom: 12px;
  }


  /* ── Section title ── */
  .card-section-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 9px 14px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .detection-hint, .lang-hint {
    padding: 7px 14px 5px;
    font-size: 11px;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
  }

  .refresh-btn {
    display: flex; align-items: center; gap: 4px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 2px 8px;
    font-size: 11.5px;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
    text-transform: none; letter-spacing: 0; font-weight: 500;
  }
  .refresh-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--accent); }
  .refresh-btn:disabled { opacity: 0.5; pointer-events: none; }


  /* ── Language defaults ── */
  .lang-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 14px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .lang-row:last-child { border-bottom: none; }
  /* Iconify renders an SVG inside this span — center it and let it sit on
     the row baseline like the legacy emoji did. The per-row brand colour
     comes in via an inline `style="color: …"` set in the markup (see
     PROJECT_TYPES.color); we leave a `currentColor` fallback so the icon
     stays visible if the constant ever ships without a colour. */
  .lang-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    flex-shrink: 0;
    color: var(--text-secondary);
  }
  .lang-icon :global(svg) { display: block; color: inherit; }
  .lang-label { font-size: 12.5px; color: var(--text-primary); flex: 1; }

  /* ── Built-in IDE rows ── */
  .ide-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border-subtle);
    transition: background 0.1s;
  }
  .ide-row:last-child { border-bottom: none; }
  .ide-row.is-default { background: var(--accent-subtle); }

  .status-dot {
    width: 18px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
  }
  .dot-ok      { color: var(--success); }
  .dot-no      { color: var(--text-disabled); }
  .dot-unknown { color: var(--text-disabled); }
  .dot-pending { font-size: 20px; color: var(--text-disabled); line-height: 1; }

  .ide-name-col {
    display: flex; flex-direction: column; gap: 1px;
    width: 120px; flex-shrink: 0;
  }
  .ide-name { font-size: 12.5px; font-weight: 500; color: var(--text-primary); }
  .ide-cmd  { font-size: 10.5px; color: var(--text-muted); font-family: var(--font-code);
              white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 110px; }

  .ide-path-col { flex: 1; min-width: 0; }
  .path-input-row { display: flex; gap: 4px; }
  .path-input-row .input { flex: 1; }

  .input {
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 4px 8px;
    font-size: 11.5px;
    color: var(--text-primary);
    outline: none;
    transition: border-color 0.15s;
    width: 100%;
    box-sizing: border-box;
    font-family: inherit;
  }
  .input:focus { border-color: var(--accent); }
  .input-path { font-family: var(--font-code); }

  .browse-btn {
    display: flex; align-items: center; justify-content: center;
    padding: 0 7px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 5px;
    color: var(--text-secondary);
    cursor: pointer; flex-shrink: 0;
    transition: background 0.12s, color 0.12s;
  }
  .browse-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .default-btn {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    background: none; border: 1px solid transparent;
    border-radius: var(--radius-sm); color: var(--text-disabled);
    cursor: pointer; flex-shrink: 0;
    transition: color 0.12s, background 0.12s, border-color 0.12s;
  }
  .default-btn:hover:not(:disabled) { color: var(--accent); background: var(--accent-subtle); border-color: var(--accent); }
  .default-btn.is-active { color: var(--accent); border-color: var(--accent); background: var(--accent-subtle); cursor: default; }
  .default-btn:disabled { pointer-events: none; }

  /* ── Custom IDEs ── */
  .add-ide-btn {
    display: flex; align-items: center; gap: 4px;
    background: var(--bg-overlay); border: 1px solid var(--border);
    border-radius: var(--radius-sm); padding: 2px 8px;
    font-size: 11.5px; font-weight: 500; color: var(--text-secondary);
    cursor: pointer; transition: background 0.12s, color 0.12s;
    text-transform: none; letter-spacing: 0;
  }
  .add-ide-btn:hover { background: var(--bg-hover); color: var(--accent); }

  .add-form { padding: 12px 14px; border-bottom: 1px solid var(--border-subtle); background: var(--bg-base); }
  .add-form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-bottom: 10px; }
  .add-form-footer { display: flex; gap: 8px; }

  .custom-ide-row {
    display: flex; align-items: center; justify-content: space-between;
    padding: 8px 14px; border-bottom: 1px solid var(--border-subtle);
  }
  .custom-ide-row:last-child { border-bottom: none; }
  .custom-ide-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .custom-ide-name { font-size: 12.5px; font-weight: 500; color: var(--text-primary); }
  .custom-ide-cmd  { font-size: 11px; color: var(--text-muted); font-family: var(--font-code); }
  .custom-ide-actions { display: flex; align-items: center; gap: 5px; }

  .default-pill {
    font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: var(--radius-sm);
    background: var(--accent-subtle); color: var(--accent); text-transform: uppercase;
  }
  .action-btn {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
    background: none; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .action-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .action-btn.danger:hover { color: var(--error); background: var(--error-subtle); }

  .empty-msg { padding: 12px 14px; font-size: 12px; color: var(--text-muted); font-style: italic; }

  .save-row { display: flex; justify-content: flex-end; padding-top: 4px; }

  .btn {
    display: flex; align-items: center; gap: 5px;
    padding: 5px 14px; border-radius: var(--radius-md);
    font-size: 12.5px; font-weight: 500;
    cursor: pointer; border: 1px solid transparent;
    transition: background 0.12s, opacity 0.12s;
  }
  .btn:disabled { opacity: 0.5; pointer-events: none; }
  .btn-primary { background: var(--accent); color: var(--text-on-accent); }
  .btn-primary:hover { opacity: 0.88; }
  .btn-ghost { background: transparent; color: var(--text-secondary); }
  .btn-ghost:hover { color: var(--text-primary); background: var(--bg-hover); }

  :global(.spin) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
