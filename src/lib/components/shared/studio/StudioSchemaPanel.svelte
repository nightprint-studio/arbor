<!--
  StudioSchemaPanel — format-agnostic right-side schema sidecar.

  The strip renders:
    · Empty state: a format-flavored intro + a "pick schema source"
      button that opens an internal `FilePickerModal`.
    · Loaded state: file/crate/root metadata, a root-type Dropdown
      backed by the latest `CrateProbe`, action buttons (Load schema /
      Change file), an inline error banner, coverage stats (three
      cards: resolved / external / unknown), and a filterable list of
      indexed types. Clicking a type row fires `onOpenViewSource` —
      the parent shows the "View implementation" modal because the
      same flow is also triggered from the tree's context menu.

  Schema STATE (`Schema` object, paths, selections, loading flag,
  error string) is parent-owned: the rest of the modal (tree
  decoration, mutation defaults, inspector, autoLoadSchemaFromHint)
  reads it directly. The panel takes it as read props + action
  callbacks. The only state OWNED by the panel is the file-picker
  open flag and the indexed-types filter.

  Format-specific copy flows through props/snippet — RON wraps with
  `pickerTitle="Pick Rust source for schema"`, `pickerExtensions=['rs']`,
  and a custom `intro` snippet ("Load a Rust source file from your
  crate…"). Per-type ref-count chip is RON-specific and surfaced via
  the optional `refCountForType` callback prop; future formats can
  omit it.
-->
<script lang="ts" module>
  import type { Snippet } from 'svelte';
  import { untrack } from 'svelte';
  import type {
    StudioBackend, StudioFormat,
    CrateProbe, Schema, TypeDef,
  } from '$lib/ipc/studio-format';

  export interface StudioSchemaPanelProps<TKind extends string = string> {
    /** Identifies the format (used only for the picker tooltip + the
     *  default `intro` copy fallback). */
    formatId: StudioFormat;
    /** Pre-bound backend. The panel does NOT run probe/load itself —
     *  the parent owns those calls so the resulting `Schema` lives
     *  next to every consumer (tree, inspector, mutation defaults).
     *  Still kept on the interface so future per-pane needs can hit
     *  the backend without prop drilling. */
    backend: StudioBackend<TKind>;

    // ── Schema state (parent-owned; read-only here) ────────────────
    schema:        Schema | null;
    schemaProbe:   CrateProbe | null;
    schemaRsPath:  string | null;
    schemaRootSel: string | null;
    schemaLoading: boolean;
    schemaError:   string | null;

    // ── Action callbacks ───────────────────────────────────────────
    /** Fires with the newly picked source path. Parent runs
     *  `backend.schemaProbe(path)` + commits the result + auto-picks
     *  the first root candidate. Async so the panel can await its
     *  completion if needed (today the call site is fire-and-forget). */
    onProbe:          (rsPath: string) => Promise<void> | void;
    /** Commits the user's root-type pick into `schemaRootSel`. */
    onSelectRoot:     (canonical: string) => void;
    /** Fires "Load schema" — parent runs `backend.schemaLoad(...)` +
     *  persists the choice via the workspace store. */
    onLoad:           () => Promise<void> | void;
    /** Clears every schema-side state (parent resets schema, probe,
     *  rsPath, rootSel, error). */
    onClear:          () => void;
    /** Opens the "View implementation" modal for a canonical path.
     *  Parent owns the modal because the tree's context menu opens
     *  the same flow. */
    onOpenViewSource: (canonical: string) => void;

    // ── Format-specific copy ───────────────────────────────────────
    /** Title for the file picker modal. Default: 'Pick schema source'. */
    pickerTitle?:       string;
    /** Extension filter passed to `FilePickerModal`. Default: `['rs']`. */
    pickerExtensions?:  string[];
    /** Button label used in both the empty-state CTA and the
     *  Change-file action. Default: 'Pick file'. */
    pickerButtonLabel?: string;

    // ── Snippets ───────────────────────────────────────────────────
    /** Renders the explanatory paragraph in the empty state.
     *  Default: a short generic line; RON injects its full Rust-crate
     *  walkthrough copy. */
    intro?: Snippet;

    /** RON-only callback for the small ref-count chip on type rows
     *  (counts fields whose name matches the active ref-pattern set).
     *  Other formats omit this — the chip stays hidden. */
    refCountForType?: (def: TypeDef) => number;
  }
</script>

<script lang="ts" generics="TKind extends string">
  import { BookOpen, FolderOpen, ChevronDown, Wand2, X, Link as LinkIcon } from 'lucide-svelte';
  import Spinner from '../ui/Spinner.svelte';
  import Alert from '../ui/Alert.svelte';
  import Dropdown, { type DropdownItem } from '../ui/Dropdown.svelte';
  import FilePickerModal from '../FilePickerModal.svelte';
  import PanelShell from '../ui/PanelShell.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    formatId,
    backend: _backend,
    schema,
    schemaProbe,
    schemaRsPath,
    schemaRootSel,
    schemaLoading,
    schemaError,
    onProbe,
    onSelectRoot,
    onLoad,
    onClear,
    onOpenViewSource,
    pickerTitle = 'Pick schema source',
    pickerExtensions = ['rs'],
    pickerButtonLabel = 'Pick file',
    intro,
    refCountForType,
  }: StudioSchemaPanelProps<TKind> = $props();

  // Silence "unused" for the parameter — kept on the interface so a
  // future format that needs direct backend access can grow into it.
  void untrack(() => _backend);

  // ── Local state ──────────────────────────────────────────────────
  /** File-picker modal open flag. Owned here because the picker
   *  is initiated from this panel's buttons and the resulting path
   *  is forwarded to the parent via `onProbe`. */
  let pickerOpen = $state(false);
  /** Indexed-types filter input. Pure local UI state. */
  let typeFilter = $state('');

  // ── Derived ──────────────────────────────────────────────────────
  /** Dropdown items for the root-type picker — derived from the
   *  parent's probe candidates. Click handlers commit the selection
   *  via the `onSelectRoot` callback. */
  const rootTypeItems = $derived.by<DropdownItem[]>(() => {
    if (!schemaProbe) return [];
    return schemaProbe.root_candidates.map(c => ({
      kind:    'item',
      id:      c.canonical_path,
      label:   c.name,
      meta:    c.kind,
      active:  schemaRootSel === c.canonical_path,
      onclick: () => onSelectRoot(c.canonical_path),
    } satisfies DropdownItem));
  });

  /** Case-insensitive substring match against the canonical path so
   *  searching "ability" matches `crate::job::ability::ActionAbility`
   *  too. */
  const filteredTypes = $derived.by<[string, TypeDef][]>(() => {
    if (!schema) return [];
    const q = typeFilter.trim().toLowerCase();
    const all = Object.entries(schema.types) as [string, TypeDef][];
    if (!q) return all;
    return all.filter(([path, def]) =>
      path.toLowerCase().includes(q) || def.name.toLowerCase().includes(q));
  });

  // ── Helpers ──────────────────────────────────────────────────────
  function basename(p: string | null): string {
    if (!p) return '';
    const norm = p.replace(/\\/g, '/');
    const slash = norm.lastIndexOf('/');
    return slash === -1 ? norm : norm.slice(slash + 1);
  }

  function openPicker()  { pickerOpen = true; }
  function cancelPick()  { pickerOpen = false; }
  async function onPicked(path: string) {
    pickerOpen = false;
    await onProbe(path);
  }
</script>

<PanelShell title="Schema" class="ssp-shell">
  {#snippet icon()}<BookOpen size={13} />{/snippet}
  <div class="ssp-body">
    {#if !schemaRsPath}
      {#if intro}
        {@render intro()}
      {:else}
        <p class="ssp-hint">No schema loaded. Pick a source file to index types for the {formatId.toUpperCase()} format.</p>
      {/if}
      <button class="ssp-btn" onclick={openPicker}>
        <FolderOpen size={12} /> {pickerButtonLabel}
      </button>
    {:else}
      <div class="ssp-meta">
        <div class="ssp-meta-row">
          <span>File</span>
          <span class="ssp-meta-val" use:tooltip={schemaRsPath}>{basename(schemaRsPath)}</span>
        </div>
        {#if schemaProbe}
          <div class="ssp-meta-row"><span>Crate</span><span class="ssp-meta-val">{schemaProbe.crate_name}</span></div>
        {/if}
        {#if schema}
          <div class="ssp-meta-row ssp-meta-root">
            <span>Root</span>
            <span class="ssp-meta-val" use:tooltip={schema.root_type}>{schema.root_name}</span>
          </div>
        {/if}
      </div>

      {#if schemaProbe && schemaProbe.root_candidates.length > 0}
        {@const selected = schemaProbe.root_candidates.find(c => c.canonical_path === schemaRootSel)}
        <span class="ssp-label">Root type</span>
        <!-- Shared Dropdown — same widget the rest of the app uses
             for value pickers. Each candidate becomes an item with
             `active` mirroring schemaRootSel, click commits the
             selection via the `onSelectRoot` callback. Searchable
             so a crate with many top-level structs/enums stays
             navigable; menu width follows the trigger. -->
        <Dropdown
          items={rootTypeItems}
          position="fixed"
          matchTriggerWidth={true}
          searchable={rootTypeItems.length > 8}
          searchPlaceholder="Filter types…"
        >
          {#snippet trigger({ toggle, open })}
            <button
              type="button"
              class="ssp-root-trigger"
              class:ssp-root-trigger-open={open}
              onclick={toggle}
              aria-haspopup="listbox"
              aria-expanded={open}
            >
              <span class="ssp-root-trigger-label">
                {#if selected}
                  <span class="ssp-root-trigger-name">{selected.name}</span>
                  <span class="ssp-root-trigger-kind">{selected.kind}</span>
                {:else}
                  <span class="ssp-root-trigger-empty">Pick a root type…</span>
                {/if}
              </span>
              <ChevronDown size={12} class="ssp-root-trigger-caret" />
            </button>
          {/snippet}
        </Dropdown>
        <div class="ssp-actions">
          <button class="ssp-btn ssp-btn-primary" onclick={() => void onLoad()} disabled={schemaLoading || !schemaRootSel}>
            {#if schemaLoading}<Spinner size="xs" color="currentColor" />{:else}<Wand2 size={11} />{/if}
            Load schema
          </button>
          <button class="ssp-btn" onclick={openPicker}>Change file</button>
        </div>
      {/if}

      {#if schemaError}
        <div class="ssp-banner-wrap"><Alert variant="error" compact text={schemaError} /></div>
      {/if}

      {#if schema}
        <!-- Coverage summary — three inline cards with kind-coded
             numbers (green resolved / blue external / muted unknown). -->
        <div class="ssp-stats">
          <div class="ssp-stat">
            <span class="ssp-stat-num ssp-stat-num-resolved">{schema.stats.resolved}</span>
            <span class="ssp-stat-label">resolved</span>
          </div>
          <div class="ssp-stat">
            <span class="ssp-stat-num ssp-stat-num-external">{schema.stats.external}</span>
            <span class="ssp-stat-label">external</span>
          </div>
          <div class="ssp-stat">
            <span class="ssp-stat-num ssp-stat-num-unknown">{schema.stats.unknown}</span>
            <span class="ssp-stat-label">unknown</span>
          </div>
        </div>

        <!-- Section label — same small-caps rhythm as the
             FilePicker sidebar's `LOCATIONS / RECENTS / …`. -->
        <div class="ssp-section">
          <span class="ssp-section-text">Indexed types</span>
          <span class="ssp-section-count">{Object.keys(schema.types).length}</span>
          <input
            class="ssp-filter"
            bind:value={typeFilter}
            placeholder="Filter…"
            spellcheck="false"
          />
        </div>
        <div class="ssp-types">
          {#each filteredTypes as [path, def] (path)}
            {@const refs = refCountForType ? refCountForType(def) : 0}
            <button
              type="button"
              class="ssp-type-row"
              onclick={() => onOpenViewSource(path)}
              use:tooltip={refs > 0
                ? `${path}\n${refs} reference field${refs === 1 ? '' : 's'}`
                : path}
            >
              <span class="ssp-type-kind ssp-type-kind-{def.kind}">
                {def.kind === 'enum' ? 'E' : def.kind === 'struct' ? 'S' : 'A'}
              </span>
              <span class="ssp-type-name">{def.name}</span>
              {#if refs > 0}
                <!-- Ref-field count badge — single click target for
                     "this type contains N fields that point at other
                     entities by id". Clicking the row opens the source,
                     where the actual fields are highlighted. -->
                <span class="ssp-type-refs"
                      use:tooltip={`${refs} reference field${refs === 1 ? '' : 's'}`}>
                  <LinkIcon size={9} />
                  <span>{refs}</span>
                </span>
              {/if}
              <span class="ssp-type-meta">
                {#if def.kind === 'struct'}{def.fields.length} fields
                {:else if def.kind === 'enum'}{def.variants.length} variants
                {:else}alias{/if}
              </span>
            </button>
          {/each}
          {#if filteredTypes.length === 0}
            <div class="ssp-types-empty">No matches for "{typeFilter}".</div>
          {/if}
        </div>

        <button class="ssp-btn ssp-btn-quiet" onclick={onClear}>
          <X size={11} /> Clear schema
        </button>
      {/if}
    {/if}
  </div>
</PanelShell>

{#if pickerOpen}
  <FilePickerModal
    mode="file"
    title={pickerTitle}
    extensions={pickerExtensions}
    initialPath={schemaRsPath ?? undefined}
    onConfirm={onPicked}
    onCancel={cancelPick}
  />
{/if}

<style>
  .ssp-body {
    padding: 12px; overflow: auto;
    display: flex; flex-direction: column;
    gap: 12px; flex: 1;
  }

  .ssp-hint { color: var(--text-muted); font-size: 11px; line-height: 1.6; }

  .ssp-meta {
    display: flex; flex-direction: column; gap: 4px;
    padding: 8px;
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
  }
  .ssp-meta-row {
    display: flex; justify-content: space-between; gap: 8px;
    font-size: 11px;
  }
  .ssp-meta-row > span:first-child { color: var(--text-muted); }
  .ssp-meta-val {
    font-family: var(--font-code);
    max-width: 200px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ssp-meta-root .ssp-meta-val {
    font-family: var(--font-code);
    color: var(--syntax-type, var(--accent));
  }

  .ssp-label {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  /* Root-type picker trigger — wide pill the user clicks to open the
     shared Dropdown. Label is split into "Name" (code font) + "kind"
     pill so a long candidate list still has a tight visual rhythm. */
  .ssp-root-trigger {
    display: flex; align-items: center; gap: 6px;
    width: 100%;
    padding: 5px 8px;
    background: var(--bg-overlay);
    color: var(--text-primary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .ssp-root-trigger:hover { border-color: var(--accent); }
  .ssp-root-trigger-open {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, var(--bg-overlay));
  }
  .ssp-root-trigger-label {
    flex: 1; min-width: 0;
    display: flex; align-items: center; gap: 6px;
    overflow: hidden;
  }
  .ssp-root-trigger-name {
    font-family: var(--font-code);
    color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ssp-root-trigger-kind {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--syntax-type, var(--accent));
    background: color-mix(in srgb, var(--syntax-type, var(--accent)) 14%, transparent);
    padding: 1px 5px;
    border-radius: 3px;
    flex-shrink: 0;
  }
  .ssp-root-trigger-empty { color: var(--text-muted); font-style: italic; }
  :global(.ssp-root-trigger-caret) { color: var(--text-muted); flex-shrink: 0; }
  .ssp-root-trigger:hover :global(.ssp-root-trigger-caret),
  .ssp-root-trigger-open :global(.ssp-root-trigger-caret) { color: var(--accent); }

  .ssp-actions { display: flex; gap: 6px; }

  .ssp-btn {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 6px 12px;
    background: var(--bg-overlay);
    color: var(--text-primary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 11px; cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .ssp-btn:hover:not(:disabled) { background: var(--bg-base); border-color: var(--accent); }
  .ssp-btn:disabled { opacity: 0.45; cursor: not-allowed; }
  .ssp-btn-primary {
    background: var(--accent);
    color: var(--bg-base);
    border-color: var(--accent);
  }
  .ssp-btn-primary:hover:not(:disabled) {
    background: var(--accent-strong, var(--accent));
    filter: brightness(1.08);
  }
  /* Quiet variant — borderless, transparent. Used for low-stakes
     terminal actions like "Clear schema" that shouldn't compete
     with the primary buttons above for attention. */
  .ssp-btn-quiet {
    background: transparent;
    border-color: transparent;
    color: var(--text-muted);
  }
  .ssp-btn-quiet:hover:not(:disabled) {
    background: var(--bg-overlay);
    border-color: transparent;
    color: var(--text-primary);
  }

  .ssp-banner-wrap { margin: 6px 8px 0; }

  /* Coverage stats — three big inline cards with kind-coded numbers
     (resolved=green / external=blue / unknown=muted). */
  .ssp-stats { display: flex; gap: 6px; }
  .ssp-stat {
    flex: 1;
    padding: 8px 6px;
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
    text-align: center;
  }
  .ssp-stat-num {
    display: block;
    font-family: var(--font-code);
    font-size: 18px; font-weight: 700;
    line-height: 1.1;
    color: var(--text-primary);
  }
  .ssp-stat-num-resolved { color: var(--success, #98c379); }
  .ssp-stat-num-external { color: var(--syntax-type, #61afef); }
  .ssp-stat-num-unknown  { color: var(--text-muted); }
  .ssp-stat-label {
    display: block;
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-top: 3px;
  }

  /* ── Indexed-types browser ───────────────────────────────────────
     File-picker-flavoured: small-caps section label, transparent row
     surface, count chip on the left, filter input flushed right. */
  .ssp-section {
    display: flex; align-items: center; gap: 6px;
    margin: 6px 0 2px;
    padding: 0 2px;
  }
  .ssp-section-text {
    font-family: var(--font-ui-sans);
    font-size: 10px; font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-disabled);
  }
  .ssp-section-count {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 0 5px;
    border-radius: 8px;
    line-height: 14px;
  }
  .ssp-filter {
    margin-left: auto;
    width: 110px;
    font-size: 10px;
    padding: 2px 6px;
    background: var(--bg-base);
    color: var(--text-primary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    outline: none;
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .ssp-filter:focus {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 4%, var(--bg-base));
  }
  .ssp-types {
    display: flex; flex-direction: column;
    gap: 3px;
    max-height: 320px;
    overflow-y: auto;
    background: transparent;
    padding: 2px 0;
  }
  .ssp-type-row {
    display: flex; align-items: center; gap: 9px;
    width: 100%;
    height: 30px;
    padding: 0 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: inherit;
    cursor: pointer;
    text-align: left;
    font-size: 12px;
    transition: background var(--transition-fast);
  }
  .ssp-type-row:hover { background: var(--bg-hover); }
  .ssp-type-kind {
    display: inline-flex; align-items: center; justify-content: center;
    width: 18px; height: 18px;
    border-radius: 3px;
    font-family: var(--font-code);
    font-size: 10px; font-weight: 700;
    color: #fff;
    flex-shrink: 0;
  }
  .ssp-type-kind-struct { background: var(--syntax-type, #61afef); }
  /* Pinned magenta/purple for enums — distinct from the yellow
     `--syntax-function` some themes ship; reads cleanly against
     both struct (blue) and alias (muted). */
  .ssp-type-kind-enum   { background: #c678dd; }
  .ssp-type-kind-alias  { background: var(--text-muted); }
  .ssp-type-name {
    font-family: var(--font-code); font-size: 11px;
    color: var(--text-primary);
    flex: 1;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ssp-type-meta {
    font-size: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  /* Ref-count badge — small accent pill, visible only when the type
     has at least one field flagged as a reference. Same accent
     rhythm as the "↗ N hits" chip in the query toolbar. */
  .ssp-type-refs {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 0 5px;
    height: 14px;
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: 7px;
    font-family: var(--font-code);
    font-size: 9.5px;
    font-weight: 600;
    flex-shrink: 0;
  }
  :global(.ssp-type-refs svg) { color: inherit; }
  .ssp-types-empty {
    padding: 10px;
    text-align: center;
    color: var(--text-muted);
    font-size: 11px;
  }
</style>
