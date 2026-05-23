<script lang="ts">
  import {
    X, Plus, Trash2, Copy, Lock, Check, RotateCcw, Palette,
    ChevronDown, ChevronRight, Upload, Download,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { themeStore, type ImportResult } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { fsReadTextFile, fsWriteTextFile } from '$lib/ipc/fs';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import type { Theme } from '$lib/types/theme';
  import { tooltip } from '$lib/actions/tooltip';

  let { onClose }: { onClose: () => void } = $props();

  // ── Import / Export pickers ───────────────────────────────────────────────
  // Use the in-app FilePickerModal — never the OS dialog — for all path
  // selection so the picker stays themed and consistent across the app.
  let showImportPicker = $state(false);
  let showExportPicker = $state(false);
  let exportTarget     = $state<Theme | null>(null);

  // ── Variable groups ───────────────────────────────────────────────────────

  type VarDef  = { key: string; label: string };
  type VarGroup = { label: string; vars: VarDef[] };

  const VAR_GROUPS: VarGroup[] = [
    { label: 'Backgrounds', vars: [
      { key: '--bg-base',      label: 'Main background' },
      { key: '--bg-elevated',  label: 'Panel / card' },
      { key: '--bg-overlay',   label: 'Overlay / dropdown' },
      { key: '--bg-hover',     label: 'Hover state' },
      { key: '--bg-selected',  label: 'Selected item' },
      { key: '--bg-input',     label: 'Input field' },
    ]},
    { label: 'Text', vars: [
      { key: '--text-primary',   label: 'Primary' },
      { key: '--text-secondary', label: 'Secondary' },
      { key: '--text-muted',     label: 'Muted' },
      { key: '--text-disabled',  label: 'Disabled' },
      { key: '--text-on-accent', label: 'On accent' },
    ]},
    { label: 'Borders', vars: [
      { key: '--border',        label: 'Standard' },
      { key: '--border-subtle', label: 'Subtle separator' },
      { key: '--border-focus',  label: 'Focus ring' },
    ]},
    { label: 'Accent', vars: [
      { key: '--accent',        label: 'Accent' },
      { key: '--accent-hover',  label: 'Hover' },
      { key: '--accent-active', label: 'Pressed' },
      { key: '--accent-subtle', label: 'Subtle background' },
    ]},
    { label: 'Status', vars: [
      { key: '--success',        label: 'Success' },
      { key: '--success-subtle', label: 'Success bg' },
      { key: '--warning',        label: 'Warning' },
      { key: '--warning-subtle', label: 'Warning bg' },
      { key: '--error',          label: 'Error' },
      { key: '--error-subtle',   label: 'Error bg' },
      { key: '--info',           label: 'Info' },
      { key: '--info-subtle',    label: 'Info bg' },
    ]},
    { label: 'Diff', vars: [
      { key: '--diff-add-bg',   label: 'Added line bg' },
      { key: '--diff-add-line', label: 'Added highlight' },
      { key: '--diff-del-bg',   label: 'Deleted line bg' },
      { key: '--diff-del-line', label: 'Deleted highlight' },
    ]},
    { label: 'Graph Lanes', vars: [
      { key: '--graph-lane-0', label: 'Lane 1' },
      { key: '--graph-lane-1', label: 'Lane 2' },
      { key: '--graph-lane-2', label: 'Lane 3' },
      { key: '--graph-lane-3', label: 'Lane 4' },
      { key: '--graph-lane-4', label: 'Lane 5' },
      { key: '--graph-lane-5', label: 'Lane 6' },
      { key: '--graph-lane-6', label: 'Lane 7' },
      { key: '--graph-lane-7', label: 'Lane 8' },
      { key: '--graph-lane-8', label: 'Lane 9' },
      { key: '--graph-lane-9', label: 'Lane 10' },
    ]},
    { label: 'Scrollbar', vars: [
      { key: '--scrollbar-thumb',       label: 'Thumb' },
      { key: '--scrollbar-thumb-hover', label: 'Thumb hover' },
    ]},
    { label: 'Shadows', vars: [
      { key: '--shadow-sm',    label: 'Small' },
      { key: '--shadow-md',    label: 'Medium' },
      { key: '--shadow-lg',    label: 'Large' },
      { key: '--shadow-popup', label: 'Popup' },
    ]},
    { label: 'Git Labels', vars: [
      { key: '--color-stash',     label: 'Stash' },
      { key: '--color-tag',       label: 'Tag' },
      { key: '--color-submodule', label: 'Submodule' },
      { key: '--color-bisect',    label: 'Bisect' },
      { key: '--color-workspace', label: 'Workspace' },
      { key: '--color-reflog',    label: 'Reflog' },
    ]},
    { label: 'File Status', vars: [
      { key: '--color-file-added',     label: 'Added' },
      { key: '--color-file-modified',  label: 'Modified' },
      { key: '--color-file-deleted',   label: 'Deleted' },
      { key: '--color-file-renamed',   label: 'Renamed' },
      { key: '--color-file-untracked', label: 'Untracked' },
    ]},
    { label: 'Terminal', vars: [
      { key: '--terminal-bg',             label: 'Background' },
      { key: '--terminal-fg',             label: 'Foreground' },
      { key: '--terminal-cursor',         label: 'Cursor' },
      { key: '--terminal-black',          label: 'Black' },
      { key: '--terminal-red',            label: 'Red' },
      { key: '--terminal-green',          label: 'Green' },
      { key: '--terminal-yellow',         label: 'Yellow' },
      { key: '--terminal-blue',           label: 'Blue' },
      { key: '--terminal-magenta',        label: 'Magenta' },
      { key: '--terminal-cyan',           label: 'Cyan' },
      { key: '--terminal-white',          label: 'White' },
      { key: '--terminal-bright-black',   label: 'Bright black' },
      { key: '--terminal-bright-red',     label: 'Bright red' },
      { key: '--terminal-bright-green',   label: 'Bright green' },
      { key: '--terminal-bright-yellow',  label: 'Bright yellow' },
      { key: '--terminal-bright-blue',    label: 'Bright blue' },
      { key: '--terminal-bright-magenta', label: 'Bright magenta' },
      { key: '--terminal-bright-cyan',    label: 'Bright cyan' },
      { key: '--terminal-bright-white',   label: 'Bright white' },
    ]},
    /* ── Non-colour groups ───────────────────────────────────────
       These hold geometry, selection feel and optional typography.
       Values are plain strings (lengths / numbers / font stacks),
       so the colour picker simply doesn't render — the inline text
       input is the only editor for them. */
    { label: 'Geometry', vars: [
      { key: '--radius-sm',         label: 'Radius — small'  },
      { key: '--radius-md',         label: 'Radius — medium' },
      { key: '--radius-lg',         label: 'Radius — large'  },
      { key: '--scrollbar-width',   label: 'Scrollbar width' },
      { key: '--scrollbar-radius',  label: 'Scrollbar radius' },
    ]},
    { label: 'Selection', vars: [
      { key: '--selection-strength', label: 'Selection strength (0.5–1.5)' },
    ]},
    { label: 'Typography', vars: [
      { key: '--theme-font-ui',   label: 'UI font (optional)' },
      { key: '--theme-font-code', label: 'Code font (optional)' },
    ]},
  ];

  // ── State ─────────────────────────────────────────────────────────────────

  const originalId = themeStore.activeId;

  // Currently selected theme in the left panel
  let selectedId = $state<string>(themeStore.activeId);

  // Editable copy of the selected theme's vars (only used when editing custom)
  let editVars = $state<Record<string, string>>({});

  // Name editing for custom themes
  let editName = $state('');

  // Track which groups are collapsed
  let collapsed = $state<Record<string, boolean>>({});

  // Saving indicator
  let saving = $state(false);

  // New theme form
  let creatingNew = $state(false);
  let newThemeName = $state('');
  let newThemeNameInput = $state<HTMLInputElement | undefined>(undefined);

  // Derived
  const selectedTheme = $derived(
    themeStore.allThemes.find(t => t.id === selectedId) ?? themeStore.allThemes[0],
  );
  const isBuiltIn   = $derived(selectedTheme?.built_in ?? true);
  const isDirty     = $derived(
    !isBuiltIn && JSON.stringify(editVars) !== JSON.stringify(selectedTheme?.vars ?? {}),
  );

  // ── Init ──────────────────────────────────────────────────────────────────

  $effect(() => {
    // When selected theme changes, reset editVars and preview
    if (selectedTheme) {
      editVars  = { ...selectedTheme.vars };
      editName  = selectedTheme.name;
      themeStore.preview(selectedTheme.vars);
    }
  });

  // ── Helpers ───────────────────────────────────────────────────────────────

  function isHex(v: string): boolean {
    return /^#[0-9a-fA-F]{3,8}$/.test(v.trim());
  }

  /** Attempt to extract a hex colour from any CSS value (for the swatch). */
  function swatchColor(v: string): string {
    if (isHex(v)) return v;
    const m = v.match(/#[0-9a-fA-F]{3,8}/);
    if (m) return m[0];
    // rgba / hsla — return as-is; browser will handle it as background
    return v;
  }

  function onVarChange(key: string, newVal: string) {
    editVars = { ...editVars, [key]: newVal };
    // Live preview
    document.documentElement.style.setProperty(key, newVal);
  }

  // Sync text input → color picker (only for pure hex)
  function onTextInput(key: string, e: Event) {
    const val = (e.target as HTMLInputElement).value;
    onVarChange(key, val);
  }

  // Color picker → text input
  function onColorPick(key: string, e: Event) {
    const hex = (e.target as HTMLInputElement).value;
    onVarChange(key, hex);
  }

  function resetVar(key: string) {
    const original = selectedTheme?.vars[key] ?? '';
    onVarChange(key, original);
  }

  function resetAll() {
    editVars = { ...selectedTheme!.vars };
    themeStore.preview(editVars);
  }

  // ── Theme actions ─────────────────────────────────────────────────────────

  async function applyTheme() {
    if (isDirty) await saveEdits();
    await themeStore.setActive(selectedId);
  }

  async function saveEdits() {
    if (!selectedTheme || isBuiltIn) return;
    saving = true;
    try {
      const updated: Theme = {
        ...selectedTheme,
        name: editName.trim() || selectedTheme.name,
        vars: { ...editVars },
      };
      await themeStore.saveCustom(updated);
      uiStore.showToast('Theme saved', 'success');
    } catch (e) {
      uiStore.showToast(`Failed to save theme: ${e}`, 'error');
    } finally {
      saving = false;
    }
  }

  async function cloneTheme(base: Theme) {
    const id   = `custom-${Date.now()}`;
    const copy: Theme = {
      id,
      name:        `${base.name} (copy)`,
      description: base.description,
      built_in:    false,
      vars:        { ...base.vars },
    };
    await themeStore.saveCustom(copy);
    selectedId = id;
    uiStore.showToast('Theme cloned', 'success');
  }

  let pendingDeleteThemeId = $state<string | null>(null);
  function deleteTheme(id: string) { pendingDeleteThemeId = id; }
  async function performDeleteTheme() {
    const id = pendingDeleteThemeId;
    pendingDeleteThemeId = null;
    if (!id) return;
    await themeStore.deleteCustom(id);
    selectedId = themeStore.activeId;
  }

  async function createNewTheme() {
    const name = newThemeName.trim();
    if (!name) return;
    const baseDark = themeStore.builtIn.find(t => t.id === 'dark')!;
    const id = `custom-${name.toLowerCase().replace(/\s+/g, '-')}-${Date.now()}`;
    const theme: Theme = {
      id,
      name,
      built_in: false,
      vars: { ...baseDark.vars },
    };
    await themeStore.saveCustom(theme);
    selectedId    = id;
    creatingNew   = false;
    newThemeName  = '';
  }

  // ── Import / Export ───────────────────────────────────────────────────────

  /** Read each selected JSON file, attempt to import all of them, then report
   *  a per-file summary toast. Successful imports become custom themes with
   *  a fresh `custom-*` id; the last-imported one is selected in the editor. */
  async function handleImportPaths(paths: string[]) {
    showImportPicker = false;
    if (paths.length === 0) return;

    const results: ImportResult[] = [];

    for (const p of paths) {
      const source = p.replace(/\\/g, '/').split('/').pop() ?? p;
      let raw: string;
      try {
        raw = await fsReadTextFile(p);
      } catch (e) {
        results.push({ source, ok: false, error: `Cannot read file: ${e}` });
        continue;
      }
      try {
        const t = await themeStore.importThemeFromJson(raw);
        results.push({ source, ok: true, theme: t });
      } catch (e) {
        results.push({ source, ok: false, error: (e as Error).message });
      }
    }
    const okCount   = results.filter(r => r.ok).length;
    const failCount = results.length - okCount;

    if (okCount > 0) {
      const last = [...results].reverse().find(r => r.ok);
      if (last?.theme) selectedId = last.theme.id;
    }

    if (failCount === 0) {
      uiStore.showToast(
        okCount === 1 ? 'Theme imported' : `${okCount} themes imported`,
        'success',
      );
    } else if (okCount === 0) {
      const first = results.find(r => !r.ok);
      uiStore.showToast(
        `Import failed: ${first?.source ?? '?'} — ${first?.error ?? 'unknown error'}`,
        'error',
      );
    } else {
      uiStore.showToast(
        `${okCount} imported, ${failCount} failed (see console)`,
        'warning',
      );
      // Keep raw error detail in the dev console for the curious.
      for (const r of results) {
        if (!r.ok) console.warn(`[theme import] ${r.source}: ${r.error}`);
      }
    }
  }

  function startExport(theme: Theme) {
    exportTarget     = theme;
    showExportPicker = true;
  }

  async function handleExportPath(path: string) {
    showExportPicker = false;
    const theme = exportTarget;
    exportTarget = null;
    if (!theme) return;
    const fileName = path.split(/[\\/]/).pop() ?? path;
    try {
      const json = themeStore.serializeTheme(theme);
      await fsWriteTextFile(path, json);
      notificationsStore.add('Theme exported', fileName, 'success');
    } catch (e) {
      notificationsStore.add('Theme export failed', String(e), 'error');
    }
  }

  function exportFilenameFor(t: Theme): string {
    const slug = t.id.replace(/[^a-z0-9-_]/gi, '-');
    return `${slug || 'theme'}.json`;
  }

  function handleClose() {
    themeStore.revertPreview();
    onClose();
  }

  $effect(() => {
    if (creatingNew) {
      // Focus the input after the next tick
      setTimeout(() => newThemeNameInput?.focus(), 0);
    }
  });
</script>

<Modal onClose={handleClose} size="full" padBody={false} ariaLabel="Theme Editor">
  {#snippet header()}
    <ModalHeader onClose={handleClose}>
      <Palette size={14} />
      <span class="modal-title">Theme Editor</span>
      {#snippet actions()}
        <!-- Opt-in for the active theme's font preferences. Off by default
             so importing a theme never silently overrides the user's
             preferred UI font; a single click lets curated themes show
             their canonical typography. -->
        <label class="font-toggle" use:tooltip={'When on, themes that declare a UI / code font use it; when off, the global font stack is used.'}>
          <input
            type="checkbox"
            checked={appearanceStore.useThemeFonts}
            onchange={(e) => appearanceStore.setUseThemeFonts(
              (e.currentTarget as HTMLInputElement).checked,
              themeStore.activeTheme.vars,
            )}
          />
          <span>Use theme fonts</span>
        </label>
        <button
          class="btn"
          onclick={() => showImportPicker = true}
          use:tooltip={'Import one or more theme JSON files'}
        >
          <Upload size={13} />
          Import
        </button>
        {#if selectedTheme}
          <button
            class="btn"
            onclick={() => startExport(selectedTheme)}
            use:tooltip={'Export the selected theme to a JSON file'}
          >
            <Download size={13} />
            Export
          </button>
        {/if}
        {#if selectedId !== originalId || isDirty}
          <button
            class="btn btn-primary"
            onclick={applyTheme}
            disabled={saving}
            use:tooltip={'Apply this theme'}
          >
            <Check size={13} />
            {saving ? 'Saving…' : (isDirty ? 'Save & Apply' : 'Apply')}
          </button>
        {/if}
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="te-body">
    <!-- ── Left sidebar: theme list ───────────────────────────────────── -->
    <aside class="theme-list">
      <div class="list-header">Themes</div>

      {#each themeStore.builtIn as theme (theme.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div
          class="theme-item"
          class:active={selectedId === theme.id}
          role="option"
          aria-selected={selectedId === theme.id}
          tabindex="0"
          onclick={() => selectedId = theme.id}
          onkeydown={(e) => e.key === 'Enter' && (selectedId = theme.id)}
        >
          <div class="theme-item-inner">
            <span class="theme-item-name">{theme.name}</span>
            <span class="badge built-in">Built-in</span>
          </div>
          <div class="theme-item-actions">
            {#if selectedId === theme.id && themeStore.activeId === theme.id}
              <span class="active-dot" use:tooltip={'Active theme'}></span>
            {/if}
            <button
              class="list-action-btn"
              onclick={(e) => { e.stopPropagation(); cloneTheme(theme); }}
              use:tooltip={'Clone theme'}
            >
              <Copy size={12} />
            </button>
          </div>
        </div>
      {/each}

      {#if themeStore.custom.length > 0}
        <div class="list-separator">Custom</div>
        {#each themeStore.custom as theme (theme.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
          <div
            class="theme-item"
            class:active={selectedId === theme.id}
            role="option"
            aria-selected={selectedId === theme.id}
            tabindex="0"
            onclick={() => selectedId = theme.id}
            onkeydown={(e) => e.key === 'Enter' && (selectedId = theme.id)}
          >
            <div class="theme-item-inner">
              <span class="theme-item-name">{theme.name}</span>
            </div>
            <div class="theme-item-actions">
              {#if themeStore.activeId === theme.id}
                <span class="active-dot" use:tooltip={'Active theme'}></span>
              {/if}
              <button
                class="list-action-btn"
                onclick={(e) => { e.stopPropagation(); cloneTheme(theme); }}
                use:tooltip={'Clone'}
              >
                <Copy size={12} />
              </button>
              <button
                class="list-action-btn danger"
                onclick={(e) => { e.stopPropagation(); deleteTheme(theme.id); }}
                use:tooltip={'Delete'}
              >
                <Trash2 size={12} />
              </button>
            </div>
          </div>
        {/each}
      {/if}

      <!-- New theme row -->
      {#if creatingNew}
        <form class="new-theme-form" onsubmit={(e) => { e.preventDefault(); createNewTheme(); }}>
          <input
            bind:this={newThemeNameInput}
            bind:value={newThemeName}
            class="new-theme-input"
            placeholder="Theme name…"
            maxlength={40}
          />
          <div class="new-theme-btns">
            <button type="submit" class="btn btn-primary btn-xs" disabled={!newThemeName.trim()}>
              Create
            </button>
            <button type="button" class="btn btn-xs" onclick={() => { creatingNew = false; newThemeName = ''; }}>
              Cancel
            </button>
          </div>
        </form>
      {:else}
        <button class="add-theme-btn" onclick={() => creatingNew = true}>
          <Plus size={13} />
          New theme
        </button>
      {/if}
    </aside>

    <!-- ── Right: variable editor ─────────────────────────────────────── -->
    <div class="editor">
      {#if selectedTheme}
        <!-- Theme name (editable for custom) -->
        <div class="editor-title-row">
          {#if isBuiltIn}
            <div class="editor-theme-name">
              <Lock size={13} class="lock-icon" />
              <span>{selectedTheme.name}</span>
              <span class="badge built-in">Read-only — clone to customise</span>
            </div>
          {:else}
            <div class="editor-name-edit">
              <input
                class="name-input"
                bind:value={editName}
                placeholder="Theme name"
                maxlength={40}
              />
              {#if isDirty}
                <button class="btn btn-xs" onclick={resetAll} use:tooltip={'Discard all changes'}>
                  <RotateCcw size={12} />
                  Reset
                </button>
              {/if}
            </div>
          {/if}
        </div>

        <!-- Variable groups -->
        <div class="var-groups">
          {#each VAR_GROUPS as group}
            <div class="var-group">
              <button
                class="group-header"
                onclick={() => collapsed[group.label] = !collapsed[group.label]}
              >
                {#if collapsed[group.label]}
                  <ChevronRight size={14} />
                {:else}
                  <ChevronDown size={14} />
                {/if}
                <span class="group-label">{group.label}</span>
                <span class="group-count">{group.vars.length}</span>
              </button>

              {#if !collapsed[group.label]}
                <div class="var-rows">
                  {#each group.vars as def}
                    {@const val = editVars[def.key] ?? selectedTheme.vars[def.key] ?? ''}
                    {@const hex = isHex(val)}
                    {@const isColorVal = hex || /^(rgb|rgba|hsl|hsla)\(/i.test(val) || /^[a-f0-9]{6,8}$/i.test(val)}
                    {@const changed = !isBuiltIn && val !== (selectedTheme.vars[def.key] ?? '')}
                    <div class="var-row">
                      <!-- Swatch — acts as color picker trigger for hex values.
                           For non-colour values (lengths, numbers, font stacks)
                           the swatch falls back to a glyph indicator so the
                           empty box doesn't look broken. -->
                      <div
                        class="swatch-trigger"
                        class:clickable={hex && !isBuiltIn}
                        class:non-color={!isColorVal}
                        use:tooltip={val}
                      >
                        {#if isColorVal}
                          <span class="swatch-color" style="background: {swatchColor(val)};"></span>
                        {:else}
                          <span class="swatch-glyph" aria-hidden="true">
                            {#if /px$|rem$|em$|%$/.test(val)}#{:else if /^[\d.]+$/.test(val)}n{:else}T{/if}
                          </span>
                        {/if}
                        {#if hex && !isBuiltIn}
                          <input
                            type="color"
                            class="swatch-input"
                            value={val}
                            oninput={(e) => onColorPick(def.key, e)}
                            tabindex="-1"
                          />
                        {/if}
                      </div>

                      <!-- Label -->
                      <span class="var-label">{def.label}</span>

                      <!-- Text / value input -->
                      <input
                        type="text"
                        class="var-input"
                        class:wide={!hex}
                        class:dirty={changed}
                        value={val}
                        readonly={isBuiltIn}
                        spellcheck={false}
                        oninput={(e) => onTextInput(def.key, e)}
                      />

                      <!-- Reset single var -->
                      {#if changed}
                        <button
                          class="reset-var-btn"
                          onclick={() => resetVar(def.key)}
                          use:tooltip={'Reset to saved'}
                        >
                          <RotateCcw size={11} />
                        </button>
                      {:else}
                        <span class="reset-var-btn-spacer"></span>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</Modal>

{#if pendingDeleteThemeId}
  <ConfirmModal
    title="Delete theme"
    message="Delete this theme?"
    detail="The active theme will fall back to the default if it was the one being deleted."
    variant="danger"
    confirmLabel="Delete"
    onCancel={() => pendingDeleteThemeId = null}
    onConfirm={performDeleteTheme}
  />
{/if}

<!-- ── In-app pickers ──────────────────────────────────────────────────── -->
{#if showImportPicker}
  <FilePickerModal
    mode="file"
    multiple={true}
    extensions={['json']}
    title="Import Theme(s)"
    onConfirmMulti={handleImportPaths}
    onCancel={() => showImportPicker = false}
  />
{/if}

{#if showExportPicker && exportTarget}
  <FilePickerModal
    mode="save"
    extensions={['json']}
    title={`Export Theme — ${exportTarget.name}`}
    initialFilename={exportFilenameFor(exportTarget)}
    onConfirm={handleExportPath}
    onCancel={() => { showExportPicker = false; exportTarget = null; }}
  />
{/if}

<style>
  /* ── Body ────────────────────────────────────────────────────── */
  .te-body {
    display: flex;
    height: 100%;
    overflow: hidden;
    min-height: 0;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }

  /* ── Sidebar ─────────────────────────────────────────────────── */
  /* Plain block container — not a flex column. With many imported
     presets the list grows past the modal's vertical room; flex column
     children default to `flex-shrink: 1` and can compress instead of
     overflowing, suppressing the scrollbar. A vanilla `overflow-y: auto`
     block scrolls naturally regardless of how many themes are listed.
     padding-bottom on the wrapper keeps the last "Add" button clear of
     the rounded corner when scrolled to the end. */
  .theme-list {
    width: 210px;
    flex-shrink: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow-y: auto;
    overflow-x: hidden;
    padding-bottom: 8px;
    /* Custom-themed scrollbar: thinner than the global default and
       padded so the thumb sits inside the rounded card. */
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }
  .theme-list::-webkit-scrollbar       { width: var(--scrollbar-width); }
  .theme-list::-webkit-scrollbar-track { background: transparent; }
  .theme-list::-webkit-scrollbar-thumb {
    background: var(--scrollbar-thumb);
    border-radius: var(--scrollbar-radius);
  }
  .theme-list::-webkit-scrollbar-thumb:hover {
    background: var(--scrollbar-thumb-hover);
  }

  .list-header {
    padding: 10px 12px;
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .list-separator {
    padding: 10px 12px;
    margin: 2px 0 0;
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    border-top: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
  }

  /* Card-list pattern — items live inside a padded flex column between
     the title/separator rows to preserve the sidebar's rounded corners. */
  .theme-list > .theme-item { margin-left: 8px; margin-right: 8px; }
  .theme-list > .list-header + .theme-item,
  .theme-list > .list-separator + .theme-item { margin-top: 6px; }
  .theme-list > .theme-item + .theme-item { margin-top: 4px; }
  .theme-list > .add-theme-btn,
  .theme-list > .new-theme-form { margin-top: 8px; }

  .theme-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 10px;
    min-height: 32px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    transition: background var(--transition-fast), border-color var(--transition-fast),
                box-shadow var(--transition-fast), color var(--transition-fast);
    text-align: left;
    user-select: none;
    box-sizing: border-box;
  }
  .theme-item:hover {
    background: var(--bg-overlay);
    border-color: var(--border);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.15);
    color: var(--text-primary);
  }
  .theme-item.active {
    background: var(--accent-subtle);
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    color: var(--accent);
  }

  .theme-item-inner {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }

  .theme-item-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .theme-item-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    opacity: 0;
    flex-shrink: 0;
    transition: opacity var(--transition-fast);
  }
  .theme-item:hover .theme-item-actions,
  .theme-item.active .theme-item-actions { opacity: 1; }

  .active-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
    margin-right: 2px;
  }

  .list-action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    padding: 0;
  }
  .list-action-btn:hover       { background: var(--bg-overlay); color: var(--text-primary); }
  .list-action-btn.danger:hover{ background: var(--error-subtle); color: var(--error); }

  .badge {
    font-size: 10px;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-weight: 500;
    white-space: nowrap;
  }
  .badge.built-in {
    background: var(--bg-overlay);
    color: var(--text-muted);
  }

  .add-theme-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    margin: 6px 8px 0;
    background: transparent;
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: border-color var(--transition-fast), color var(--transition-fast), background var(--transition-fast);
    width: calc(100% - 16px);
  }
  .add-theme-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-subtle);
  }

  .new-theme-form {
    padding: 8px 8px 4px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .new-theme-input {
    width: 100%;
    padding: 5px 8px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    outline: none;
  }
  .new-theme-input:focus { border-color: var(--border-focus); }
  .new-theme-btns { display: flex; gap: 4px; }

  /* ── Editor ──────────────────────────────────────────────────── */
  .editor {
    flex: 1;
    min-width: 0;
    /* grid with two explicit rows: header (auto) + scrollable area (1fr) */
    display: grid;
    grid-template-rows: auto 1fr;
    overflow: hidden;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }

  .editor-title-row {
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-subtle);
    /* Match the editor card's --bg-base so the rounded top corners of
       the card are actually visible — using --bg-elevated here would blend
       into the modal body wrapper and hide the rounding cue. */
    background: var(--bg-base);
  }

  .editor-theme-name {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-md);
  }
  :global(.lock-icon) { color: var(--text-muted); }

  .editor-name-edit {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .name-input {
    flex: 1;
    max-width: 300px;
    padding: 5px 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-md);
    font-weight: 600;
    outline: none;
  }
  .name-input:focus { border-color: var(--border-focus); }

  .var-groups {
    /* grid child: takes the 1fr row — overflow-y is now well-bounded */
    overflow-y: auto;
    overflow-x: hidden;
    padding: 8px 16px 24px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .var-group {
    flex-shrink: 0;          /* never compress — .var-groups scrolls instead */
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .group-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 8px 12px;
    background: var(--bg-elevated);
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    font-weight: 600;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
    user-select: none;
  }
  .group-header:hover { background: var(--bg-hover); color: var(--text-primary); }

  .group-label { flex: 1; }

  .group-count {
    font-size: 10px;
    background: var(--bg-overlay);
    color: var(--text-muted);
    padding: 1px 5px;
    border-radius: var(--radius-lg);
  }

  .var-rows {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr 1fr;
    gap: 1px;
    background: var(--border-subtle);
    border-top: 1px solid var(--border-subtle);
  }

  .var-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    background: var(--bg-base);
    transition: background var(--transition-fast);
  }
  .var-row:hover { background: var(--bg-hover); }

  /* ── Swatch (color picker trigger) ──────────────────────────── */
  .swatch-trigger {
    position: relative;
    width: 22px;
    height: 22px;
    border-radius: var(--radius-sm);
    border: 1px solid rgba(128, 128, 128, 0.25);
    flex-shrink: 0;
    overflow: hidden;
    cursor: default;
  }
  .swatch-trigger.clickable { cursor: pointer; }
  .swatch-trigger.clickable:hover { border-color: var(--border-focus); }

  .swatch-color {
    display: block;
    width: 100%;
    height: 100%;
    background-clip: padding-box;
  }

  /* Non-colour token swatch: shows a single-letter glyph centred in the box
     so the row still has a visual anchor where the swatch would have been.
     "#" for lengths, "n" for plain numbers, "T" for typography / fallback. */
  .swatch-trigger.non-color { background: var(--bg-overlay); }
  .swatch-glyph {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    font-family: var(--font-code);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: lowercase;
  }

  /* Invisible native color picker overlaid on the swatch */
  .swatch-input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
    padding: 0;
    border: none;
  }

  .var-label {
    flex: 1;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .var-input {
    width: 110px;
    flex-shrink: 0;
    padding: 3px 6px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 11px;
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .var-input.wide  { width: 185px; }
  .var-input.dirty { border-color: var(--warning); }
  .var-input:focus { border-color: var(--border-focus); }
  .var-input[readonly] { opacity: 0.55; cursor: default; }

  .reset-var-btn-spacer {
    width: 20px;
    flex-shrink: 0;
  }

  .reset-var-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0;
    padding: 0;
    transition: opacity var(--transition-fast), color var(--transition-fast), background var(--transition-fast);
    flex-shrink: 0;
  }
  .var-row:hover .reset-var-btn { opacity: 1; }
  .reset-var-btn:hover { color: var(--warning); background: var(--warning-subtle); }

  /* ── Buttons ─────────────────────────────────────────────────── */
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-overlay);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
    white-space: nowrap;
  }
  .btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .btn:disabled { opacity: 0.5; cursor: default; }

  .btn-primary {
    background: var(--accent);
    color: var(--text-on-accent);
    border-color: var(--accent);
  }
  .btn-primary:hover { background: var(--accent-hover); border-color: var(--accent-hover); }

  .btn-xs { padding: 3px 8px; font-size: 11px; }

  /* "Use theme fonts" header toggle — sits between the Theme Editor title
     and the Import / Export buttons. Compact pill-style so it doesn't
     dominate the chrome. */
  .font-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 9px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 11.5px;
    cursor: pointer;
    user-select: none;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .font-toggle:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border); }
  .font-toggle input[type="checkbox"] {
    width: 12px;
    height: 12px;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }
</style>
