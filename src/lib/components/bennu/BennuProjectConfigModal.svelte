<script lang="ts">
  /**
   * Bennu Project Configuration — per-project settings for the open Java project.
   *
   * Lets the user OVERRIDE the BE-resolved project facts (JDK language level,
   * source encoding, source/output roots, excluded dirs) and inspect the
   * read-only facts (modules). Overrides route through
   * `bennuProjectConfigStore`, the SEAM that will map onto a future `[bennu]`
   * section in the per-repo `<repo>/.arbor/bennu/config.toml` (CLAUDE.md rule #11).
   *
   * Structure/scaffold phase: the config store is in-memory (MOCK). Every mock is
   * marked inline. Dependencies live in their own left tool window
   * (BennuDependenciesPanel), not here.
   *
   * Keyboard-first: first field auto-focused by <Modal>, Tab cycles fields in
   * logical order, Esc cancels (handled by <Modal>), Ctrl/Cmd+Enter applies.
   */
  import { untrack } from 'svelte';
  import { Settings2, Coffee, FileType, Boxes, FolderTree, SpellCheck } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuSpellStore } from '$lib/stores/bennu/spell.svelte';
  import { bennuNamingStore } from '$lib/stores/bennu/naming.svelte';
  import BennuNamingSettings from './BennuNamingSettings.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import {
    bennuProjectConfigStore,
    defaultConfig,
    AUTO,
    type BennuProjectConfig,
  } from '$lib/stores/bennu/project-config.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const project = $derived(projectStore.project);
  const root = $derived(project?.root ?? null);

  // Resolved facts from the BE (what "Auto" defers to).
  const resolvedJdk = $derived(project?.jdk?.version ?? null);
  const resolvedJdkSource = $derived(project?.jdk?.source ?? null);
  const resolvedEncoding = $derived(projectStore.activeEncoding ?? 'UTF-8');
  const modules = $derived(project?.modules ?? []);

  // Spelling (opt-in per project). Enable is gated on dictionaries being installed.
  const spellOn = $derived(root ? bennuSpellStore.enabledFor(root) : false);

  // ── Local editable draft ──────────────────────────────────────────────────
  // Seeded from the store on open; applied back on Ctrl+Enter / Apply. Editing a
  // local copy (not the store directly) keeps Cancel a true no-op.
  let draft = $state<BennuProjectConfig>(
    root ? { ...bennuProjectConfigStore.get(root) } : defaultConfig(),
  );

  // ── Select options ─────────────────────────────────────────────────────────
  // "Auto" carries the resolved value in its label so the user sees what it maps
  // to without leaving the modal.
  const jdkOptions = $derived([
    { value: AUTO, label: resolvedJdk ? `Auto — from pom (${resolvedJdk})` : 'Auto — from pom' },
    { value: '1.8', label: 'Java 8 (1.8)' },
    { value: '11',  label: 'Java 11' },
    { value: '17',  label: 'Java 17' },
    { value: '21',  label: 'Java 21' },
  ]);

  const encodingOptions = $derived([
    { value: AUTO,      label: `Auto — resolved (${resolvedEncoding})` },
    { value: 'UTF-8',   label: 'UTF-8' },
    { value: 'Cp1252',  label: 'Cp1252 (Windows-1252)' },
    { value: 'ISO-8859-1', label: 'ISO-8859-1 (Latin-1)' },
    { value: 'US-ASCII', label: 'US-ASCII' },
  ]);

  // ── Actions ────────────────────────────────────────────────────────────────
  // The naming section edits its own store's draft (it is a real per-repo section on disk, not
  // part of the in-memory `draft` above), so it is loaded when the modal opens on a project and
  // written by the same Apply.
  $effect(() => {
    const r = root;
    // Untracked: both calls READ the store state they then write (the cached catalog, the loaded
    // section), and reading it here would make this effect depend on its own result — one extra
    // round-trip per open, for nothing. The project root is the only real dependency.
    untrack(() => {
      void bennuNamingStore.loadCatalog(r);
      if (r) void bennuNamingStore.load(r);
    });
  });

  function apply() {
    // MOCK — persists only to the in-memory store; wire to per-project
    // `<repo>/.arbor/bennu/config.toml` when the BE lands.
    if (root) bennuProjectConfigStore.apply(root, draft);
    // Naming IS on disk already; only written when it actually changed, so opening and closing the
    // modal never rewrites the file (and never invalidates the BE's cached copy for nothing).
    if (root && bennuNamingStore.dirty) {
      void bennuNamingStore.apply().then((ok) => {
        if (!ok) toastStore.show("Couldn't save the naming conventions", 'error');
      });
    }
    onClose();
  }

  function resetToDefaults() {
    draft = defaultConfig();
    // "Reset" must mean the whole modal, not the half of it that happens to live in a local. The
    // naming draft goes back to what is on disk — resetting it to *empty* would silently offer to
    // delete a convention the project deliberately adopted.
    bennuNamingStore.revert();
  }

  // Ctrl/Cmd+Enter submits from anywhere in the modal body.
  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      apply();
    }
  }
</script>

<Modal
  {onClose}
  width="680px"
  height="620px"
  ariaLabel="Bennu Project Configuration"
>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings2 size={14} />
      <span class="modal-title">Project Configuration</span>
      {#if project}
        <span class="hdr-name">{project.name}</span>
      {/if}
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={handleKeydown}>
    {#if !project}
      <EmptyState message="Open a project to configure it." />
    {:else}
      <!-- JDK ─────────────────────────────────────────────────────────────── -->
      <section class="cfg-section">
        <div class="sec-head">
          <Coffee size={13} />
          <h3>JDK</h3>
        </div>
        <FormField
          label="Language level"
          hint={resolvedJdkSource
            ? `Resolved from ${resolvedJdkSource}. Override only to pin a different level.`
            : 'The Java language level Bennu resolves the classpath against.'}
        >
          <Select bind:value={draft.jdkOverride} options={jdkOptions} />
        </FormField>
      </section>

      <!-- Encoding ────────────────────────────────────────────────────────── -->
      <section class="cfg-section">
        <div class="sec-head">
          <FileType size={13} />
          <h3>Encoding</h3>
        </div>
        <FormField
          label="Source encoding"
          hint="How file source is decoded. Legacy projects often declare Cp1252 in the pom."
        >
          <Select bind:value={draft.encodingOverride} options={encodingOptions} />
        </FormField>
      </section>

      <!-- Spelling ──────────────────────────────────────────────────────── -->
      <section class="cfg-section">
        <div class="sec-head">
          <SpellCheck size={13} />
          <h3>Spelling</h3>
        </div>
        <FormField
          label="Spell-check identifiers &amp; comments"
          hint="Checks declared names (split by camelCase / snake_case / kebab-case) and comments against English + Italian dictionaries. Misspellings show as hints with an 'Add to dictionary' quick-fix."
        >
          <Toggle
            checked={spellOn}
            disabled={!bennuSpellStore.installed || !root}
            onchange={(v) => { if (root) bennuSpellStore.setEnabled(root, v); }}
            label={bennuSpellStore.installed ? (spellOn ? 'On' : 'Off') : 'Download the dictionaries first'}
          />
        </FormField>
        <div class="spell-dicts">
          {#if bennuSpellStore.installed}
            <span class="spell-status ok">Installed: {bennuSpellStore.status?.languages.join(', ')}</span>
          {:else}
            <span class="spell-status">No dictionaries installed yet.</span>
          {/if}
          <Button
            variant="secondary"
            size="sm"
            onclick={() => void bennuSpellStore.download()}
            disabled={bennuSpellStore.downloading}
          >
            {bennuSpellStore.downloading ? 'Downloading…' : bennuSpellStore.installed ? 'Re-download' : 'Download EN + IT'}
          </Button>
        </div>
        {#if bennuSpellStore.downloading && bennuSpellStore.progress}
          <div class="spell-prog">{bennuSpellStore.progress}</div>
        {/if}
      </section>

      <!-- Naming conventions ─────────────────────────────────────────────── -->
      <BennuNamingSettings />

      <!-- Source / output roots ──────────────────────────────────────────── -->
      <section class="cfg-section">
        <div class="sec-head">
          <FolderTree size={13} />
          <h3>Roots</h3>
        </div>
        <div class="two-col">
          <FormField label="Source root">
            <Input bind:value={draft.sourceRoot} placeholder="src/main/java" />
          </FormField>
          <FormField label="Output root">
            <Input bind:value={draft.outputRoot} placeholder="target/classes" />
          </FormField>
        </div>
        <FormField
          label="Excluded directories"
          hint="Comma-separated folder names skipped by indexing and search."
        >
          <Input bind:value={draft.excludedDirs} placeholder="target, .git, .idea" />
        </FormField>
      </section>

      <!-- Modules (read-only) ────────────────────────────────────────────── -->
      <section class="cfg-section">
        <div class="sec-head">
          <Boxes size={13} />
          <h3>Modules</h3>
          {#if modules.length}<span class="sec-count">{modules.length}</span>{/if}
        </div>
        {#if modules.length === 0}
          <EmptyState message="Single-module project — no child modules declared." compact />
        {:else}
          <ul class="ro-list">
            {#each modules as m (m)}
              <li class="ro-row">
                <Boxes size={12} />
                <span class="ro-primary">{m}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </section>

    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" size="sm" onclick={resetToDefaults} disabled={!project}>
        Reset to defaults
      </Button>
      <div class="footer-actions">
        <Button variant="secondary" size="sm" onclick={onClose}>Cancel</Button>
        <Button
          variant="primary"
          size="sm"
          onclick={apply}
          disabled={!project}
          tooltip={{ content: 'Apply', shortcut: 'Ctrl+Enter' }}
        >
          Apply
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-name {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .cfg-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .sec-head {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--text-secondary);
  }
  .sec-head h3 {
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-primary);
  }
  .sec-count {
    font-size: var(--font-size-2xs);
    font-weight: 600;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    padding: 0 6px;
    line-height: 15px;
  }
  .sec-note {
    margin: -2px 0 2px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    line-height: 1.45;
  }

  .two-col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .ro-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .ro-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    border-top: 1px solid var(--border-subtle);
  }
  .ro-row:first-child { border-top: none; }
  .ro-row :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  .ro-primary {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .footer-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .spell-dicts {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .spell-status { font-size: var(--font-size-xs); color: var(--text-muted); }
  .spell-status.ok { color: var(--success); }
  .spell-prog { font-size: var(--font-size-2xs); color: var(--text-muted); font-family: var(--font-code); }
</style>
