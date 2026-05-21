<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Plus, Trash2, Check, RefreshCw, CircleCheck, CircleX, FolderOpen,
  } from 'lucide-svelte';
  import { terminalStore } from '$lib/stores/terminal.svelte';
  import {
    getTerminalsConfig, setTerminalsConfig, startShellDetection,
  } from '$lib/ipc/terminal';
  import { uiStore } from '$lib/stores/ui.svelte';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { TerminalsConfig, TerminalEntry } from '$lib/types/terminal';

  let config = $state<TerminalsConfig>(
    terminalStore.config ?? { default_shell: null, custom_shells: [], path_overrides: {} }
  );
  let detecting = $state(false);
  let saving    = $state(false);

  $effect(() => {
    if (terminalStore.config) config = { ...terminalStore.config };
  });

  const builtinShells  = $derived(terminalStore.builtinShells);
  const detectedShells = $derived(terminalStore.detectedShells);

  let addOpen      = $state(false);
  let newId        = $state('');
  let newName      = $state('');
  let newCommand   = $state('');
  let newArgs      = $state('');
  let filePickerShellId = $state('');
  let savedPathOverrides = $state<Record<string, string>>({});

  onMount(async () => {
    try {
      const cfg = await getTerminalsConfig();
      config = cfg;
      savedPathOverrides = { ...cfg.path_overrides };
      terminalStore.setConfig(cfg);
    } catch { /* keep defaults */ }
  });

  function detectionFor(id: string) {
    return detectedShells.find(d => d.id === id);
  }

  async function runDetection() {
    detecting = true;
    try {
      await startShellDetection();
    } catch { /* ignore */ } finally {
      detecting = false;
    }
  }

  function pickPath(id: string) { filePickerShellId = id; }

  function setPathOverride(id: string, value: string) {
    const overrides = { ...config.path_overrides };
    if (value.trim()) overrides[id] = value.trim();
    else delete overrides[id];
    config = { ...config, path_overrides: overrides };
  }

  function setDefault(id: string | null) {
    config = { ...config, default_shell: id && id.length > 0 ? id : null };
  }

  async function save() {
    saving = true;
    try {
      const pathsChanged =
        JSON.stringify(config.path_overrides) !== JSON.stringify(savedPathOverrides);
      await setTerminalsConfig(config);
      terminalStore.setConfig(config);
      if (pathsChanged) {
        savedPathOverrides = { ...config.path_overrides };
        await startShellDetection();
      }
      uiStore.showToast('Terminal settings saved', 'success');
    } catch (e) {
      uiStore.showToast(`Save failed: ${e}`, 'error');
    } finally {
      saving = false;
    }
  }

  function addCustomShell() {
    if (!newId.trim() || !newName.trim() || !newCommand.trim()) return;
    const entry: TerminalEntry = {
      id:      newId.trim(),
      name:    newName.trim(),
      command: newCommand.trim(),
      args:    newArgs.split(' ').map(s => s.trim()).filter(Boolean),
    };
    config = { ...config, custom_shells: [...config.custom_shells, entry] };
    newId = newName = newCommand = newArgs = '';
    addOpen = false;
  }

  function removeCustomShell(id: string) {
    config = { ...config, custom_shells: config.custom_shells.filter(e => e.id !== id) };
    if (config.default_shell === id) config = { ...config, default_shell: null };
  }

  const allShellOptions = $derived([
    { id: '', name: '— platform default —' },
    ...builtinShells.map(b => ({ id: b.id, name: b.name })),
    ...config.custom_shells.map(c => ({ id: c.id, name: c.name })),
  ]);
</script>

{#if filePickerShellId}
  <FilePickerModal
    mode="file"
    title="Select Shell Executable"
    onConfirm={(path) => {
      config = {
        ...config,
        path_overrides: { ...config.path_overrides, [filePickerShellId]: path },
      };
      filePickerShellId = '';
    }}
    onCancel={() => { filePickerShellId = ''; }}
  />
{/if}

<SectionHeader
  title="Terminals"
  description="Detect installed shells, set executable paths, and define custom terminals available from the integrated terminal panel."
/>

<div class="card">
  <FormRow label="Default shell" description="Opened by the bare “+” button">
    <Select
      value={config.default_shell ?? ''}
      options={allShellOptions.map(s => ({ value: s.id, label: s.name }))}
      onchange={(v) => setDefault(v)}
    />
  </FormRow>
</div>

<div class="card">
  <div class="card-section-title">
    Detected Shells
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
  <div class="hint">
    Shells are probed once at startup. Set a custom path if a shell is installed in a non-standard location — then re-detect.
  </div>

  {#each builtinShells as sh}
    {@const det = detectionFor(sh.id)}
    {@const override = config.path_overrides[sh.id] ?? ''}
    <div class="row" class:is-default={config.default_shell === sh.id}>
      <span
        class="status-dot"
        class:dot-ok={det?.available}
        class:dot-no={det && !det.available}
        class:dot-unknown={!det}
        use:tooltip={det?.available
          ? `Found: ${det.detected_path ?? sh.cmd}`
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

      <div class="name-col">
        <span class="name">{sh.name}</span>
        <span class="cmd">{override || det?.detected_path || sh.cmd}</span>
      </div>

      <div class="path-col">
        <div class="path-input-row">
          <input
            class="input input-path"
            type="text"
            placeholder={det?.detected_path ?? `Path to ${sh.cmd}…`}
            value={override}
            oninput={(e) => setPathOverride(sh.id, (e.target as HTMLInputElement).value)}
          />
          <button class="browse-btn" onclick={() => pickPath(sh.id)} use:tooltip={'Browse…'}>
            <FolderOpen size={12} />
          </button>
        </div>
      </div>

      <button
        class="default-btn"
        class:is-active={config.default_shell === sh.id}
        onclick={() => setDefault(sh.id)}
        use:tooltip={config.default_shell === sh.id ? 'Default shell' : 'Set as default'}
        disabled={config.default_shell === sh.id}
      >
        <Check size={11} />
      </button>
    </div>
  {/each}
</div>

<div class="card">
  <div class="card-section-title">
    Custom Terminals
    <button class="add-btn" onclick={() => addOpen = !addOpen}>
      <Plus size={11} /> Add
    </button>
  </div>

  {#if addOpen}
    <div class="add-form">
      <div class="add-form-grid">
        <FormField label="ID (unique key)" for="new-shell-id">
          <input id="new-shell-id" class="input" placeholder="my-shell" bind:value={newId} />
        </FormField>
        <FormField label="Display name" for="new-shell-name">
          <input id="new-shell-name" class="input" placeholder="My Shell" bind:value={newName} />
        </FormField>
        <FormField label="Command / full path" for="new-shell-cmd">
          <input id="new-shell-cmd" class="input" placeholder="/usr/local/bin/myshell" bind:value={newCommand} />
        </FormField>
        <FormField label="Extra args (space-separated)" for="new-shell-args">
          <input id="new-shell-args" class="input" placeholder="--login -i" bind:value={newArgs} />
        </FormField>
      </div>
      <div class="add-form-footer">
        <button
          class="btn btn-primary"
          onclick={addCustomShell}
          disabled={!newId || !newName || !newCommand}
        >
          Add Terminal
        </button>
        <button class="btn btn-ghost" onclick={() => addOpen = false}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if config.custom_shells.length === 0 && !addOpen}
    <div class="empty-msg">No custom terminals. Click “Add” to define one.</div>
  {:else}
    {#each config.custom_shells as entry (entry.id)}
      <div class="custom-row">
        <div class="custom-info">
          <span class="custom-name">{entry.name}</span>
          <span class="custom-cmd">{entry.command} {entry.args.join(' ')}</span>
        </div>
        <div class="custom-actions">
          {#if config.default_shell === entry.id}
            <span class="default-pill">default</span>
          {:else}
            <button class="action-btn" onclick={() => setDefault(entry.id)} use:tooltip={'Set as default'}>
              <Check size={11} />
            </button>
          {/if}
          <button class="action-btn danger" onclick={() => removeCustomShell(entry.id)} use:tooltip={'Remove'}>
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

  .hint {
    padding: 7px 14px 5px;
    font-size: 11px;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
  }

  .refresh-btn, .add-btn {
    display: flex; align-items: center; gap: 4px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 2px 8px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
    text-transform: none; letter-spacing: 0;
  }
  .refresh-btn:hover:not(:disabled),
  .add-btn:hover { background: var(--bg-hover); color: var(--accent); }
  .refresh-btn:disabled { opacity: 0.5; pointer-events: none; }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border-subtle);
    transition: background 0.1s;
  }
  .row:last-child { border-bottom: none; }
  .row.is-default { background: var(--accent-subtle); }

  .status-dot {
    width: 18px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
  }
  .dot-ok      { color: var(--success); }
  .dot-no      { color: var(--text-disabled); }
  .dot-unknown { color: var(--text-disabled); }
  .dot-pending { font-size: 20px; color: var(--text-disabled); line-height: 1; }

  .name-col {
    display: flex; flex-direction: column; gap: 1px;
    width: 140px; flex-shrink: 0;
  }
  .name { font-size: 12.5px; font-weight: 500; color: var(--text-primary); }
  .cmd  { font-size: 10.5px; color: var(--text-muted); font-family: var(--font-code);
          white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 130px; }

  .path-col      { flex: 1; min-width: 0; }
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
  .input-path  { font-family: var(--font-code); }

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
  .default-btn:hover:not(:disabled) {
    color: var(--accent); background: var(--accent-subtle); border-color: var(--accent);
  }
  .default-btn.is-active {
    color: var(--accent); border-color: var(--accent);
    background: var(--accent-subtle); cursor: default;
  }
  .default-btn:disabled { pointer-events: none; }

  .add-form        { padding: 12px 14px; border-bottom: 1px solid var(--border-subtle); background: var(--bg-base); }
  .add-form-grid   { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-bottom: 10px; }
  .add-form-footer { display: flex; gap: 8px; }

  .custom-row {
    display: flex; align-items: center; justify-content: space-between;
    padding: 8px 14px; border-bottom: 1px solid var(--border-subtle);
  }
  .custom-row:last-child { border-bottom: none; }
  .custom-info    { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .custom-name    { font-size: 12.5px; font-weight: 500; color: var(--text-primary); }
  .custom-cmd     { font-size: 11px; color: var(--text-muted); font-family: var(--font-code); }
  .custom-actions { display: flex; align-items: center; gap: 5px; }

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
  .action-btn:hover         { background: var(--bg-hover); color: var(--text-primary); }
  .action-btn.danger:hover  { color: var(--error); background: var(--error-subtle); }

  .empty-msg { padding: 12px 14px; font-size: 12px; color: var(--text-muted); font-style: italic; }

  .save-row { display: flex; justify-content: flex-end; padding-top: 4px; }

  .btn {
    display: flex; align-items: center; gap: 5px;
    padding: 5px 14px; border-radius: var(--radius-md);
    font-size: 12.5px; font-weight: 500;
    cursor: pointer; border: 1px solid transparent;
    transition: background 0.12s, opacity 0.12s;
  }
  .btn:disabled    { opacity: 0.5; pointer-events: none; }
  .btn-primary     { background: var(--accent); color: var(--text-on-accent); }
  .btn-primary:hover { opacity: 0.88; }
  .btn-ghost        { background: transparent; color: var(--text-secondary); }
  .btn-ghost:hover  { color: var(--text-primary); background: var(--bg-hover); }

  :global(.spin) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
