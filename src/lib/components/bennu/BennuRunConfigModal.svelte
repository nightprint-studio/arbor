<script lang="ts">
  /**
   * Bennu Run Configurations — the IntelliJ-style run-configuration EDITOR.
   *
   * Two panes:
   *   • LEFT  — the list of NAMED configs for the open project. Create (+),
   *     duplicate, delete; arrow-key navigation; a ● marks the ACTIVE config
   *     (what the titlebar ▶ / Shift+F10 launches). Right-click for the same
   *     actions via the shared context menu.
   *   • RIGHT — the form for the selected config: name, main class (free-text
   *     for now — discovery is a BE follow-up), program args, VM args, working
   *     directory, and environment variables (key/value rows).
   *
   * Every edit funnels straight into {@link bennuRunConfigStore} (the SEAM for a
   * future per-repo `[bennu.run]` config), so there's no separate "Apply" — the
   * list and the store are always in sync. "Run" builds then launches the
   * SELECTED config; the ● (Set active) button makes it the default target.
   *
   * Keyboard-first: the config list auto-focuses when non-empty; ↑/↓ move the
   * selection, Enter runs it; Tab cycles the form fields; Ctrl/Cmd+Enter saves &
   * closes (everything is already saved — this just dismisses); Esc cancels
   * (handled by <Modal>).
   */
  import {
    Play, Plus, Copy, Trash2, SlidersHorizontal, CircleDot, Circle, X,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import {
    bennuRunConfigStore, splitArgs, type RunConfig, type EnvVar,
  } from '$lib/stores/bennu/run-config.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const project = $derived(projectStore.project);
  const root = $derived(project?.root ?? null);

  // ── Selection ───────────────────────────────────────────────────────────────
  // Selected = which config the form edits (distinct from ACTIVE = what ▶ Run
  // launches). Seed from the active config so opening lands on the run target.
  let selectedId = $state<string | null>(null);

  const configs = $derived(root ? bennuRunConfigStore.configsFor(root) : []);
  const activeId = $derived(root ? bennuRunConfigStore.activeIdFor(root) : null);
  const selected = $derived<RunConfig | null>(
    configs.find((c) => c.id === selectedId) ?? null,
  );

  // Keep the selection valid as the list mutates (create/delete): fall back to the
  // active config, then the first, then null.
  $effect(() => {
    if (!root) return;
    if (selectedId && configs.some((c) => c.id === selectedId)) return;
    selectedId = activeId ?? configs[0]?.id ?? null;
  });

  // ── List actions ────────────────────────────────────────────────────────────
  function createConfig() {
    if (!root) return;
    selectedId = bennuRunConfigStore.create(root);
    focusNameSoon();
  }
  function duplicateConfig(id: string) {
    if (!root) return;
    const nid = bennuRunConfigStore.duplicate(root, id);
    if (nid) { selectedId = nid; focusNameSoon(); }
  }
  function deleteConfig(id: string) {
    if (!root) return;
    selectedId = bennuRunConfigStore.remove(root, id);
  }
  function setActive(id: string) {
    if (!root) return;
    bennuRunConfigStore.setActive(root, id);
  }

  // ── Form edits — every change persists straight into the store ───────────────
  function patch(p: Partial<Omit<RunConfig, 'id'>>) {
    if (root && selectedId) bennuRunConfigStore.update(root, selectedId, p);
  }
  function addEnv() {
    if (!selected) return;
    patch({ env: [...selected.env, { key: '', value: '' }] });
  }
  function updateEnv(idx: number, next: Partial<EnvVar>) {
    if (!selected) return;
    const env = selected.env.map((e, i) => (i === idx ? { ...e, ...next } : e));
    patch({ env });
  }
  function removeEnv(idx: number) {
    if (!selected) return;
    patch({ env: selected.env.filter((_, i) => i !== idx) });
  }

  // ── Run the SELECTED config (build then launch) ──────────────────────────────
  const canRun = $derived(
    !!root && !!selected && selected.mainClass.trim().length > 0 && !bennuRunStore.active,
  );
  function runSelected() {
    if (!root || !selected || !canRun) return;
    const cls = selected.mainClass.trim();
    const args = splitArgs(selected.programArgs);
    // Make the config we're launching the active one, so the titlebar ▶ keeps
    // running the same target next time.
    bennuRunConfigStore.setActive(root, selected.id);
    onClose();
    void bennuRunStore.run(root, cls, args);
  }

  // ── Keyboard nav on the config list ──────────────────────────────────────────
  let listEl = $state<HTMLUListElement | undefined>();
  let nameEl = $state<HTMLInputElement | undefined>();

  function focusNameSoon() {
    queueMicrotask(() => nameEl?.focus());
  }

  function onListKeydown(e: KeyboardEvent) {
    if (!configs.length) return;
    const idx = configs.findIndex((c) => c.id === selectedId);
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedId = configs[Math.min(idx + 1, configs.length - 1)]?.id ?? selectedId;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedId = configs[Math.max(idx - 1, 0)]?.id ?? selectedId;
    } else if (e.key === 'Home') {
      e.preventDefault();
      selectedId = configs[0]?.id ?? selectedId;
    } else if (e.key === 'End') {
      e.preventDefault();
      selectedId = configs[configs.length - 1]?.id ?? selectedId;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      runSelected();
    } else if (e.key === 'Delete' && selectedId) {
      e.preventDefault();
      deleteConfig(selectedId);
    }
  }

  function openRowMenu(e: MouseEvent, cfg: RunConfig) {
    e.preventDefault();
    const items: MenuItem[] = [
      { id: 'active', label: 'Set as active', icon: CircleDot, disabled: cfg.id === activeId },
      { id: 'duplicate', label: 'Duplicate', icon: Copy },
      { id: 'delete', label: 'Delete', icon: Trash2, danger: true },
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      if (id === 'active') setActive(cfg.id);
      else if (id === 'duplicate') duplicateConfig(cfg.id);
      else if (id === 'delete') deleteConfig(cfg.id);
    });
  }

  // Ctrl/Cmd+Enter = save & close (everything is live-saved; this just dismisses).
  function onBodyKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      onClose();
    }
  }

  // Auto-focus the list ONCE, the first time it mounts with configs, so ↑/↓ work
  // immediately. Guarded so later list mutations (create → focus the name field)
  // don't yank focus back to the list.
  let listFocused = false;
  $effect(() => {
    if (!listFocused && configs.length && listEl) {
      listFocused = true;
      const el = listEl;
      queueMicrotask(() => el.focus());
    }
  });

  const cmdPreview = $derived.by(() => {
    if (!selected) return '';
    const vm = selected.vmArgs.trim();
    const prog = selected.programArgs.trim();
    const cls = selected.mainClass.trim() || 'com.example.Main';
    return `java ${vm ? vm + ' ' : ''}-cp target/classes:<deps> ${cls}${prog ? ' ' + prog : ''}`;
  });
</script>

<Modal
  {onClose}
  width="820px"
  height="600px"
  padBody={false}
  ariaLabel="Bennu Run Configurations"
>
  {#snippet header()}
    <ModalHeader {onClose}>
      <SlidersHorizontal size={14} />
      <span class="modal-title">Run Configurations</span>
      {#if project}<span class="hdr-name">{project.name}</span>{/if}
    </ModalHeader>
  {/snippet}

  {#if !project}
    <div class="empty-wrap">
      <EmptyState message="Open a project to configure a run." />
    </div>
  {:else}
    <div class="split">
      <!-- LEFT — config list ─────────────────────────────────────────────── -->
      <aside class="list-pane">
        <div class="list-toolbar">
          <span class="list-title">Configurations</span>
          <div class="list-tools">
            <button
              class="icon-btn"
              onclick={createConfig}
              use:tooltip={'New configuration'}
              aria-label="New configuration"
            >
              <Plus size={14} />
            </button>
            <button
              class="icon-btn"
              onclick={() => selectedId && duplicateConfig(selectedId)}
              disabled={!selectedId}
              use:tooltip={'Duplicate'}
              aria-label="Duplicate configuration"
            >
              <Copy size={14} />
            </button>
            <button
              class="icon-btn"
              onclick={() => selectedId && deleteConfig(selectedId)}
              disabled={!selectedId}
              use:tooltip={'Delete'}
              aria-label="Delete configuration"
            >
              <Trash2 size={14} />
            </button>
          </div>
        </div>

        {#if configs.length === 0}
          <div class="list-empty">
            <EmptyState message="No run configurations yet." compact />
            <Button variant="secondary" size="sm" onclick={createConfig}>
              {#snippet iconStart()}<Plus size={13} />{/snippet}
              Add configuration
            </Button>
          </div>
        {:else}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <ul
            class="cfg-list"
            role="listbox"
            aria-label="Run configurations"
            tabindex="0"
            bind:this={listEl}
            onkeydown={onListKeydown}
          >
            {#each configs as cfg (cfg.id)}
              <li role="option" aria-selected={cfg.id === selectedId}>
                <button
                  class="cfg-row"
                  class:selected={cfg.id === selectedId}
                  onclick={() => (selectedId = cfg.id)}
                  ondblclick={runSelected}
                  oncontextmenu={(e) => openRowMenu(e, cfg)}
                  title={cfg.id === activeId ? 'Active configuration' : ''}
                >
                  <span class="cfg-mark" class:active={cfg.id === activeId}>
                    {#if cfg.id === activeId}
                      <CircleDot size={13} />
                    {:else}
                      <Circle size={13} />
                    {/if}
                  </span>
                  <span class="cfg-name">{cfg.name || 'Unnamed'}</span>
                  {#if cfg.id === activeId}<span class="cfg-badge">active</span>{/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </aside>

      <!-- RIGHT — form for the selected config ────────────────────────────── -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <section class="form-pane" onkeydown={onBodyKeydown}>
        {#if !selected}
          <EmptyState message="Select or create a configuration to edit it." />
        {:else}
          <div class="form-head">
            <FormField label="Name">
              <Input
                value={selected.name}
                bind:element={nameEl}
                placeholder="Application"
                oninput={(v) => patch({ name: v })}
              />
            </FormField>
            <Button
              variant={selected.id === activeId ? 'tonal' : 'secondary'}
              size="sm"
              onclick={() => setActive(selected!.id)}
              disabled={selected.id === activeId}
              tooltip={{ content: 'Make this the ▶ Run target' }}
            >
              {#snippet iconStart()}<CircleDot size={13} />{/snippet}
              {selected.id === activeId ? 'Active' : 'Set active'}
            </Button>
          </div>

          <FormField
            label="Main class"
            hint="Fully qualified class with a public static void main. Discovery is coming — type it for now."
          >
            <Input
              value={selected.mainClass}
              placeholder="com.example.Main"
              oninput={(v) => patch({ mainClass: v })}
            />
          </FormField>

          <FormField label="Program arguments" hint="Passed to the program after the main class.">
            <Input
              value={selected.programArgs}
              placeholder="--port 8080 input.txt"
              oninput={(v) => patch({ programArgs: v })}
            />
          </FormField>

          <FormField label="VM arguments" hint="JVM options (-Xmx…, -D…). Not yet forwarded to the runner — coming with the run BE.">
            <Input
              value={selected.vmArgs}
              placeholder="-Xmx512m -Dfile.encoding=UTF-8"
              oninput={(v) => patch({ vmArgs: v })}
            />
          </FormField>

          <FormField label="Working directory" hint="Empty = project root.">
            <Input
              value={selected.workingDir}
              placeholder="$PROJECT_DIR$"
              oninput={(v) => patch({ workingDir: v })}
            />
          </FormField>

          <FormField label="Environment variables">
            {#snippet actions()}
              <button
                class="icon-btn"
                onclick={addEnv}
                use:tooltip={'Add variable'}
                aria-label="Add environment variable"
              >
                <Plus size={13} />
              </button>
            {/snippet}
            {#if selected.env.length === 0}
              <div class="env-empty">No environment variables.</div>
            {:else}
              <div class="env-rows">
                {#each selected.env as row, i (i)}
                  <div class="env-row">
                    <Input
                      value={row.key}
                      placeholder="NAME"
                      ariaLabel="Variable name"
                      oninput={(v) => updateEnv(i, { key: v })}
                    />
                    <span class="env-eq">=</span>
                    <Input
                      value={row.value}
                      placeholder="value"
                      ariaLabel="Variable value"
                      oninput={(v) => updateEnv(i, { value: v })}
                    />
                    <button
                      class="icon-btn"
                      onclick={() => removeEnv(i)}
                      use:tooltip={'Remove'}
                      aria-label="Remove variable"
                    >
                      <X size={13} />
                    </button>
                  </div>
                {/each}
              </div>
            {/if}
          </FormField>

          <p class="cmd-preview"><span class="cmd">{cmdPreview}</span></p>
        {/if}
      </section>
    </div>
  {/if}

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="foot-hint">Changes save as you type.</span>
      <div class="footer-actions">
        <Button variant="secondary" size="sm" onclick={onClose}>Close</Button>
        <Button
          variant="primary"
          size="sm"
          onclick={runSelected}
          disabled={!canRun}
          tooltip={{ content: 'Build & run this configuration', shortcut: 'Enter' }}
        >
          {#snippet iconStart()}<Play size={13} />{/snippet}
          Run
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .hdr-name {
    font-size: 11px; color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .empty-wrap { display: flex; align-items: center; justify-content: center; height: 100%; }

  /* Two-pane split — edge-to-edge (padBody={false}). Left list on --bg-base with
     a divider, right form scrolls. */
  .split {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100%;
    min-height: 0;
  }

  /* ── Left: list pane ──────────────────────────────────────────────────── */
  .list-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border-subtle);
    background: var(--bg-base);
  }
  .list-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .list-title {
    font-size: 11px; font-weight: 600; letter-spacing: 0.02em;
    color: var(--text-secondary); text-transform: uppercase;
  }
  .list-tools { display: flex; align-items: center; gap: 2px; }

  .list-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 24px 12px;
  }

  .cfg-list {
    list-style: none;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .cfg-list:focus-visible { outline: none; }
  .cfg-list:focus-visible .cfg-row.selected {
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .cfg-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .cfg-row:hover { background: var(--bg-hover); }
  .cfg-row.selected { background: var(--bg-selected, var(--bg-hover)); }

  .cfg-mark { display: inline-flex; color: var(--text-muted); flex-shrink: 0; }
  .cfg-mark.active { color: var(--success); }
  .cfg-name {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .cfg-badge {
    font-size: 9px; text-transform: uppercase; letter-spacing: 0.4px; font-weight: 700;
    color: var(--success);
    background: color-mix(in srgb, var(--success) 16%, transparent);
    border-radius: var(--radius-sm); padding: 1px 5px; flex-shrink: 0;
  }

  /* ── Right: form pane ─────────────────────────────────────────────────── */
  .form-pane {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 18px;
    overflow-y: auto;
    min-height: 0;
  }
  .form-head {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
    gap: 12px;
  }

  .env-empty { font-size: 11px; color: var(--text-muted); padding: 2px 0; }
  .env-rows { display: flex; flex-direction: column; gap: 6px; }
  .env-row {
    display: grid;
    grid-template-columns: 1fr auto 1.4fr auto;
    align-items: center;
    gap: 6px;
  }
  .env-eq { color: var(--text-muted); font-family: var(--font-code); }

  /* Shared small icon button — matches the list toolbar + field actions. */
  .icon-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .icon-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn:disabled { opacity: 0.4; cursor: default; }

  .cmd-preview { margin: 2px 0 0; }
  .cmd {
    font-family: var(--font-code); font-size: 11px; color: var(--text-muted);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 6px 9px; display: block;
    overflow-x: auto; white-space: nowrap;
  }

  .foot-hint { font-size: 11px; color: var(--text-muted); }
  .footer-actions { display: flex; align-items: center; gap: 8px; }
</style>
