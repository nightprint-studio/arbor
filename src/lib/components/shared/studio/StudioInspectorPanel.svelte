<!--
  StudioInspectorPanel — format-agnostic detail card for the studio modal.

  Renders the right-side "Inspector" pane that shows everything we know
  about the currently-selected tree node:

    · Header strip with the kind badge + kind label, child count, and
      action buttons (Copy path, Copy value, Remove from parent).
    · Path display (`$.foo.bar` style).
    · Body, three optional sections:
      1. Schema strip — resolved type / variant picker / missing fields.
         Only shown when the parent supplies the schema-derived data
         (RON today; JSON Schema / TOML / YAML follow with their own
         adapters).
      2. Value — the raw value text plus inline edit + Option toggle.
         The edit input lives inside this panel; focus management
         (microtask + select-all) is self-contained.
      3. Used by — when the selected node is a cross-ref definition,
         lists the usage sites and offers a Rescan button + click-to-
         jump. Wired through the format-agnostic `studioStore.usages`
         API and the parent-supplied `onJumpToUsage` callback.

  What the panel does NOT own:
    · The pane wrapper + slide-in transition + sizing — the parent's
      `<div class="rs-detail-pane" transition:slide>` keeps that role
      so the inspector card animates as the rail toggles between
      "Inspector / Schema / Bindings" right-pane sections.
    · The mutation pipeline (insert, remove, replace, variant pick,
      option toggle, primitive edit). The panel delivers the user's
      intent through callbacks; the parent applies it to the AST.
    · Format-specific helpers (kind badge glyphs, removability rules,
      schema lookups, definition-node detection). Parent passes them
      as small accessor functions — the panel never reaches into RON
      types or schema models directly.
-->
<script lang="ts" module>
  import type { Snippet } from 'svelte';
  import type { StudioBackend, StudioFormat } from '$lib/ipc/studio-format';
  import type { StudioTreeNodeBase } from './StudioTreePane.svelte';

  /** Resolved-type info the schema strip surfaces at the top of the
   *  body. `isUnknown` / `isExternal` drive the colour palette so the
   *  user can tell at a glance whether the schema resolved the slot or
   *  punted on it. */
  export interface InspectorSchemaTypeInfo {
    label: string;
    isUnknown: boolean;
    isExternal: boolean;
  }

  /** Data driving the "Variant" picker section — shown for enum
   *  variants where the schema knows the alternatives. */
  export interface InspectorVariantPickerInfo {
    /** Enum type name displayed next to the section title. */
    enumName: string;
    /** Currently-selected variant tag (empty string when unset). */
    currentTag: string;
    /** All variants the schema knows. `suffix` is the visual hint
     *  shown next to the variant name (e.g. `(…)` for tuples,
     *  ` { … }` for structs, empty for units). */
    variants: { name: string; suffix: string }[];
  }

  /** One row in the "Missing fields" section — schema fields the live
   *  data is missing. Clicking inserts the field with a schema default. */
  export interface InspectorMissingField {
    name: string;
    typeLabel: string;
    hasDefault: boolean;
  }

  /** Usage entry (cross-ref definition site → all sites that reference
   *  the value). Format-agnostic: `studioStore.readUsages` already
   *  works in these terms for every format with cross-refs. */
  export interface InspectorUsageEntry {
    absolute_path: string;
    field_path: string[];
    key_name: string;
    file_name: string;
  }

  /** Imperative surface exposed via `bind:this`. Empty today — focus
   *  management is internal and triggered by prop changes. Reserved
   *  for future shortcuts (e.g. `focusValue()` to scroll the body to
   *  the value section). */
  export interface StudioInspectorPanelController {
    /** Force-focus the detail-location edit input. Parent calls this
     *  from its `startEdit('detail')` so the focus dance survives the
     *  microtask delay even if the panel was unmounted at the moment
     *  the user fired the action. */
    focusEditInput(): void;
  }

  export interface StudioInspectorPanelProps<
    TKind extends string = string,
    TNode extends StudioTreeNodeBase<TKind> = StudioTreeNodeBase<TKind>,
  > {
    formatId: StudioFormat;
    /** Pre-bound backend. Unused inside the panel today — kept on
     *  the interface for symmetry with the other studio panels and
     *  as an escape hatch for future per-pane fetches. */
    backend: StudioBackend<TKind>;

    // ── Selection state (parent-owned, read-only) ──────────────────
    selectedNode: TNode | null;
    valueText: string | null;
    valueLoading: boolean;

    // ── Inline-edit state (parent-owned trigger; bindable buffers) ──
    editingPid: string | null;
    editLocation: 'tree' | 'detail';
    editBuf: string;       // $bindable — panel inputs write here
    editError: string | null;
    editBannerVisible: boolean;

    // ── Format-specific accessors ──────────────────────────────────
    /** Single-glyph badge for the given kind. Default returns the
     *  kind name itself so even an un-wired wrapper renders. */
    kindBadge?: (kind: TKind) => string;
    /** Is this node removable from its parent? Drives the trash
     *  action in the head strip. */
    isRemovable: (node: TNode) => boolean;
    /** True when the kind supports primitive inline edit (string /
     *  number / bool / char and so on). Drives the Edit pencil and
     *  the input vs. <select> branch below. */
    isEditablePrimitive: (kind: TKind) => boolean;
    /** True for kinds that should render a `<select>` edit input
     *  instead of `<input type="text">` (RON's `bool`). When omitted
     *  the panel always renders a text input. */
    isBoolKind?: (kind: TKind) => boolean;
    /** True for kinds whose edit input wants a "single character"
     *  placeholder (RON's `char`). Cosmetic. */
    isCharKind?: (kind: TKind) => boolean;
    /** True for kinds that expose a Some/None toggle button in the
     *  Value section header (RON's `option`). When omitted the
     *  toggle is never shown. */
    isOptionKind?: (kind: TKind) => boolean;

    /** True when the kind is a container (struct/map/list/object/
     *  array/…). Drives the first-level "Contents" preview section.
     *  When omitted the section is never rendered (graceful fallback
     *  for older wrappers). */
    isContainerKind?: (kind: TKind) => boolean;

    /** Is this node a cross-ref definition? Drives the Used-by
     *  section. */
    isDefinitionNode: (node: TNode) => boolean;
    /** Extract the actual definition value (e.g. the unquoted
     *  string content). Returns null when the node doesn't carry
     *  a definition value (defensive — parent generally filters
     *  with `isDefinitionNode` first). */
    definitionValue: (node: TNode) => string | null;

    /** Schema-derived type info for the selected node. Pass
     *  `() => null` (or omit) when the format has no schema. */
    schemaTypeInfo?: (node: TNode) => InspectorSchemaTypeInfo | null;
    /** Variant picker data; `null` when the node isn't an
     *  enum-variant or no enum schema is bound. */
    variantPickerInfo?: (node: TNode) => InspectorVariantPickerInfo | null;
    /** Missing-fields list (empty array = nothing to show). */
    missingFields?: (node: TNode) => InspectorMissingField[];

    // ── Action callbacks ───────────────────────────────────────────
    onCopyPath: (node: TNode) => void | Promise<void>;
    onCopyValue: () => void | Promise<void>;
    onRemove: () => void | Promise<void>;
    onStartEdit: (location: 'tree' | 'detail') => void;
    onCommitEdit: () => void | Promise<void>;
    onCancelEdit: () => void;
    onPickVariant: (name: string) => void | Promise<void>;
    onAddField: (parent: TNode, fieldName: string) => void | Promise<void>;
    onToggleOption: () => void | Promise<void>;
    onDismissEditBanner: () => void;
    onJumpToUsage: (u: InspectorUsageEntry) => void | Promise<void>;
    /** Optional click handler for a first-level Contents preview row.
     *  Wrapper typically forwards to `treePane.selectNode(child)` so
     *  the tree jumps to the chosen child. When omitted, preview rows
     *  are still rendered but not clickable. */
    onSelectChild?: (child: TNode) => void | Promise<void>;

    // ── Optional content overrides ─────────────────────────────────
    /** Empty-state body. Default renders a generic prompt. */
    emptyState?: Snippet;
  }
</script>

<script
  lang="ts"
  generics="TKind extends string, TNode extends StudioTreeNodeBase<TKind>"
>
  import { untrack } from 'svelte';
  import {
    ScanSearch, Link as LinkIcon, Copy, Trash2, Replace, Plus,
    Pencil, Check, X, ToggleLeft, ToggleRight, Info,
    ArrowUpRight, RotateCcw, FileCode,
  } from 'lucide-svelte';
  import Button from '../ui/Button.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';

  let {
    formatId,
    backend: _backend,
    selectedNode,
    valueText,
    valueLoading,
    editingPid,
    editLocation,
    editBuf = $bindable(),
    editError,
    editBannerVisible,
    kindBadge,
    isRemovable,
    isEditablePrimitive,
    isBoolKind,
    isCharKind,
    isOptionKind,
    isContainerKind,
    isDefinitionNode,
    definitionValue,
    schemaTypeInfo,
    variantPickerInfo,
    missingFields,
    onCopyPath,
    onCopyValue,
    onRemove,
    onStartEdit,
    onCommitEdit,
    onCancelEdit,
    onPickVariant,
    onAddField,
    onToggleOption,
    onDismissEditBanner,
    onJumpToUsage,
    onSelectChild,
    emptyState,
  }: StudioInspectorPanelProps<TKind, TNode> = $props();

  // Kept on the interface but unused in the panel body for now.
  void untrack(() => _backend);

  /** Edit-input refs — panel owns its own input nodes for the
   *  detail-location inline edit. Tree-location inputs live in the
   *  parent's row snippet and remain parent-managed. */
  let editInputEl:  HTMLInputElement  | undefined = $state();
  let editSelectEl: HTMLSelectElement | undefined = $state();

  /** Focus + select-all when the parent enters detail-location edit
   *  on the currently-selected node. Two `queueMicrotask` hops
   *  because the input is rendered inside a Svelte `{#if}` branch
   *  that only mounts AFTER the editingPid state writes propagate. */
  function focusActiveInput(): void {
    if (!selectedNode) return;
    if (editingPid !== selectedNode.pid) return;
    if (editLocation !== 'detail') return;
    queueMicrotask(() => queueMicrotask(() => {
      const isBool = isBoolKind?.(selectedNode.kind) ?? false;
      const el = isBool ? editSelectEl : editInputEl;
      el?.focus();
      if (el instanceof HTMLInputElement) el.select();
    }));
  }

  $effect(() => {
    // Track editingPid + editLocation; selectedNode is referenced
    // inside focusActiveInput via `untrack` so a selection change
    // alone doesn't re-fire focus (we only want the transition into
    // edit mode to grab focus, not the steady-state).
    const _pid = editingPid;
    const _loc = editLocation;
    void _pid; void _loc;
    untrack(() => focusActiveInput());
  });

  /** Imperative escape hatch — parent's `startEdit('detail')` can
   *  call this after toggling `editingPid` if the natural effect
   *  hasn't fired yet (rare; survives microtask scheduling edge
   *  cases). */
  export function focusEditInput(): void {
    focusActiveInput();
  }

  /** Lazy-load usages when the selection lands on a definition node
   *  the parent has flagged. Same logic that used to live in the
   *  modal — `studioStore.loadUsages` dedupes + caches, so re-
   *  selecting the same node is free. */
  /** Map FE format id onto a host file-kind so the cross-ref +
   *  usage lookups hit the right per-kind slice of the store
   *  (Phase 3.c onwards — TOML 4.c.a, YAML 5.c, .properties Phase 6). */
  const usagesKind = $derived<'ron' | 'json' | 'toml' | 'yaml' | 'properties'>(
    formatId === 'json'       ? 'json' :
    formatId === 'toml'       ? 'toml' :
    formatId === 'yaml'       ? 'yaml' :
    formatId === 'properties' ? 'properties' :
    'ron'
  );

  $effect(() => {
    const node = selectedNode;
    if (!node || !isDefinitionNode(node)) return;
    const value = definitionValue(node);
    const tabId = tabsStore.activeTabId;
    if (!value || !tabId) return;
    const kind = usagesKind;
    untrack(() => { void studioStore.loadUsagesForKind(tabId, value, kind); });
  });

  /** Cancel-on-Esc / commit-on-Enter for the detail-location edit
   *  input. `stopPropagation` keeps the keys from bubbling up to
   *  the tree's row keydown handler (which would re-select / re-
   *  expand the current row). */
  function onEditKey(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      void onCommitEdit();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onCancelEdit();
    }
  }

  /** Render-time helpers — all read-only, no mutation. */
  function pathLabel(node: TNode): string {
    return node.path.length === 0 ? '$' : '$.' + node.path.join('.');
  }
  function badgeText(kind: TKind): string {
    return (kindBadge ? kindBadge(kind) : String(kind));
  }

  // ── Usages reactive lookups ────────────────────────────────────────
  // Derived directly off the store so a `loadUsages` from elsewhere
  // (e.g. a context-menu Rescan) flows in.
  const usagesData = $derived.by<{ items: InspectorUsageEntry[] | null; loading: boolean } | null>(() => {
    if (!selectedNode || !isDefinitionNode(selectedNode)) return null;
    const value = definitionValue(selectedNode);
    const tabId = tabsStore.activeTabId;
    if (!value || !tabId) return null;
    return {
      items:   studioStore.readUsagesForKind(tabId, value, usagesKind) as InspectorUsageEntry[] | null,
      loading: studioStore.isUsagesLoadingForKind(tabId, value, usagesKind),
    };
  });

  function rescanUsages(): void {
    if (!selectedNode || !isDefinitionNode(selectedNode)) return;
    const value = definitionValue(selectedNode);
    const tabId = tabsStore.activeTabId;
    if (!value || !tabId) return;
    void studioStore.loadUsagesForKind(tabId, value, usagesKind);
  }
</script>

<div class="sip-pane">
  <!-- Standard panel header — matches PanelShell ps-header. The
       activity rail owns the open/close toggle, so no redundant X
       button here. -->
  <div class="sip-head">
    <ScanSearch size={13} />
    <span class="sip-title">Inspector</span>
    <span class="sip-spacer"></span>
  </div>

  {#if selectedNode}
    <div class="sip-head-row">
      <span class="sip-kind">
        <span class="sip-row-badge sip-row-badge-{selectedNode.kind}">{badgeText(selectedNode.kind)}</span>
        <span>{selectedNode.kind}</span>
      </span>
      {#if selectedNode.child_count > 0}
        <span class="sip-count">{selectedNode.child_count} children</span>
      {/if}
      <span class="sip-spacer"></span>
      <Button variant="icon" size="md" tooltip="Copy path" ariaLabel="Copy path"
              onclick={() => void onCopyPath(selectedNode!)}>
        {#snippet iconStart()}<LinkIcon size={11} />{/snippet}
      </Button>
      {#if valueText != null}
        <Button variant="icon" size="md" tooltip="Copy value" ariaLabel="Copy value"
                onclick={() => void onCopyValue()}>
          {#snippet iconStart()}<Copy size={11} />{/snippet}
        </Button>
      {/if}
      {#if isRemovable(selectedNode)}
        <Button variant="icon" size="md" tooltip="Remove from parent" ariaLabel="Remove"
                color="var(--error)"
                onclick={() => void onRemove()}>
          {#snippet iconStart()}<Trash2 size={11} />{/snippet}
        </Button>
      {/if}
    </div>

    <div class="sip-path" use:tooltip={'Path from document root'}>
      {pathLabel(selectedNode)}
    </div>

    <div class="sip-body">
      <!-- ── Schema strip ─────────────────────────────────────────── -->
      {#if schemaTypeInfo}
        {@const ty = schemaTypeInfo(selectedNode)}
        {#if ty}
          <div>
            <div class="sip-section-title">Schema type</div>
            <div class="sip-type">
              <span class="sip-type-value" class:sip-type-unknown={ty.isUnknown} class:sip-type-external={ty.isExternal}>
                {ty.label}
              </span>
            </div>
          </div>
        {/if}
      {/if}

      {#if variantPickerInfo}
        {@const vp = variantPickerInfo(selectedNode)}
        {#if vp}
          <div>
            <div class="sip-section-title sip-value-title">
              <Replace size={11} />
              <span>Variant · {vp.enumName}</span>
            </div>
            <select class="sip-variant-picker"
                    value={vp.currentTag}
                    onchange={(e) => void onPickVariant((e.target as HTMLSelectElement).value)}
                    use:tooltip={'Switch enum variant (replaces payload with schema-defaulted values)'}>
              {#if !vp.currentTag}
                <option value="" disabled>— pick variant —</option>
              {/if}
              {#each vp.variants as v (v.name)}
                <option value={v.name}>{v.name}{v.suffix}</option>
              {/each}
            </select>
          </div>
        {/if}
      {/if}

      {#if missingFields}
        {@const missing = missingFields(selectedNode)}
        {#if missing.length > 0}
          <div>
            <div class="sip-section-title">Missing fields · {missing.length}</div>
            <div class="sip-missing">
              {#each missing as f (f.name)}
                <button
                  type="button"
                  class="sip-missing-row sip-missing-row-btn"
                  onclick={() => void onAddField(selectedNode!, f.name)}
                  use:tooltip={`Add ${f.name} with a schema-default value`}
                >
                  <Plus size={11} class="sip-missing-plus" />
                  <span class="sip-missing-name">{f.name}</span>
                  <span class="sip-missing-type">{f.typeLabel}</span>
                  {#if f.hasDefault}<span class="sip-missing-pill">default</span>{/if}
                </button>
              {/each}
            </div>
          </div>
        {/if}
      {/if}

      <!-- ── Value section ───────────────────────────────────────────── -->
      {#if !valueLoading && valueText != null}
        <div>
          <div class="sip-section-title sip-value-title">
            <span>Value</span>
            {#if isEditablePrimitive(selectedNode.kind) && editingPid !== selectedNode.pid}
              <button type="button" class="sip-edit-btn"
                      onclick={() => onStartEdit('detail')}
                      use:tooltip={'Edit value (regenerates document text)'}
                      aria-label="Edit value">
                <Pencil size={11} />
                <span>Edit</span>
              </button>
            {/if}
            {#if isOptionKind?.(selectedNode.kind)}
              <button type="button" class="sip-edit-btn"
                      onclick={() => void onToggleOption()}
                      use:tooltip={'Toggle Some / None'}
                      aria-label="Toggle option">
                {#if valueText === 'None'}
                  <ToggleLeft size={12} />
                  <span>Set Some</span>
                {:else}
                  <ToggleRight size={12} />
                  <span>Set None</span>
                {/if}
              </button>
            {/if}
          </div>

          {#if editingPid === selectedNode.pid && editLocation === 'detail' && isEditablePrimitive(selectedNode.kind)}
            {#if editBannerVisible}
              <div class="sip-edit-banner">
                <Info size={12} />
                <span>Tree edits regenerate the document text — comments and original formatting are normalised.</span>
                <button class="sip-edit-banner-dismiss" onclick={onDismissEditBanner} aria-label="Dismiss notice">Got it</button>
              </div>
            {/if}
            {#if isBoolKind?.(selectedNode.kind)}
              <div class="sip-edit-row">
                <select class="sip-edit-input"
                        bind:this={editSelectEl}
                        bind:value={editBuf}
                        onkeydown={onEditKey}>
                  <option value="true">true</option>
                  <option value="false">false</option>
                </select>
                <button class="sip-edit-commit" onclick={() => void onCommitEdit()} use:tooltip={'Apply'} aria-label="Apply">
                  <Check size={12} />
                </button>
                <button class="sip-edit-cancel" onclick={onCancelEdit} use:tooltip={'Cancel (Esc)'} aria-label="Cancel">
                  <X size={12} />
                </button>
              </div>
            {:else}
              <div class="sip-edit-row">
                <input class="sip-edit-input"
                       bind:this={editInputEl}
                       bind:value={editBuf}
                       onkeydown={onEditKey}
                       type="text"
                       placeholder={isCharKind?.(selectedNode.kind) ? 'single character' : ''}
                       spellcheck="false" />
                <button class="sip-edit-commit" onclick={() => void onCommitEdit()} use:tooltip={'Apply (Enter)'} aria-label="Apply">
                  <Check size={12} />
                </button>
                <button class="sip-edit-cancel" onclick={onCancelEdit} use:tooltip={'Cancel (Esc)'} aria-label="Cancel">
                  <X size={12} />
                </button>
              </div>
            {/if}
            {#if editError}
              <div class="sip-edit-error">{editError}</div>
            {/if}
          {:else}
            <pre class="sip-pre">{valueText}</pre>
          {/if}
        </div>
      {/if}

      <!-- ── Contents preview ────────────────────────────────────────
           First-level children for containers. Mirrors the JSON-studio
           behaviour where selecting a parent row shows its immediate
           children as a quick preview — saves the user from having to
           expand the row in the tree just to peek at structure.

           Children are lazily loaded by the tree pane on container
           selection (see <StudioTreePane>.selectNode), so we either
           render the list immediately or a lightweight "loading…"
           hint while the fetch is in flight. -->
      {#if isContainerKind?.(selectedNode.kind)}
        {@const kids = (selectedNode.children ?? null) as TNode[] | null}
        {@const total = selectedNode.child_count}
        {@const MAX_PREVIEW = 50}
        <div>
          <div class="sip-section-title">
            Contents{#if total > 0} · {total}{/if}
          </div>
          {#if total === 0}
            <div class="sip-preview-empty">No children.</div>
          {:else if kids == null || selectedNode.loading}
            <div class="sip-preview-empty">Loading children…</div>
          {:else}
            {@const shown = kids.slice(0, MAX_PREVIEW)}
            <div class="sip-preview-list">
              {#each shown as c (c.pid)}
                <button
                  type="button"
                  class="sip-preview-row"
                  disabled={!onSelectChild}
                  onclick={() => { if (onSelectChild) void onSelectChild(c); }}
                  use:tooltip={onSelectChild ? 'Jump to this child' : ''}
                >
                  <span class="sip-row-badge sip-row-badge-{c.kind}">{badgeText(c.kind)}</span>
                  <span class="sip-preview-key">{c.key}</span>
                  <span class="sip-preview-value">{c.preview}</span>
                </button>
              {/each}
            </div>
            {#if kids.length > MAX_PREVIEW}
              <div class="sip-preview-more">+ {kids.length - MAX_PREVIEW} more — expand the row in the tree to see them all</div>
            {/if}
          {/if}
        </div>
      {/if}

      <!-- ── Used by section ─────────────────────────────────────────── -->
      {#if isDefinitionNode(selectedNode) && usagesData}
        {@const items     = usagesData.items}
        {@const isLoading = usagesData.loading}
        <div>
          <div class="sip-section-title sip-value-title">
            <ArrowUpRight size={11} />
            <span>
              Used by
              {#if items == null}…{:else}{items.length}{/if}
            </span>
            <button class="sip-edit-btn"
                    onclick={rescanUsages}
                    use:tooltip={'Rescan references'}
                    aria-label="Rescan references">
              <RotateCcw size={11} />
            </button>
          </div>
          {#if isLoading && (items == null || items.length === 0)}
            <div class="sip-usages-empty">Scanning project…</div>
          {:else if items != null && items.length === 0}
            <div class="sip-usages-empty">No references found in this repo.</div>
          {:else if items != null && items.length > 0}
            <div class="sip-usages-list">
              {#each items as u, i (i)}
                <button class="sip-usages-row"
                        onclick={() => void onJumpToUsage(u)}
                        use:tooltip={u.absolute_path}>
                  <span class="sip-usages-key">{u.key_name}</span>
                  <span class="sip-usages-file">{u.file_name}</span>
                  <span class="sip-usages-path">$.{u.field_path.join('.')}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {:else}
    <div class="sip-empty">
      {#if emptyState}
        {@render emptyState()}
      {:else}
        <FileCode size={28} />
        <span>Select a node in the tree to inspect its value.</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* The card chrome (width / slide animation / background) lives in
     the parent's `.rs-detail-pane` wrapper — the panel only fills it.
     min-height:0 + overflow:hidden keep the body scroll container
     bounded inside the flex column. */
  .sip-pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  /* Standard panel header — same recipe as RonStudioModal's
     `.rs-panel-head` so the inspector matches the schema / refs /
     stage / etc. panels at a glance. */
  .sip-head {
    display: flex; align-items: center; gap: 6px;
    padding: 0 8px 0 12px;
    height: 34px;
    min-height: 34px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .sip-title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
    text-transform: uppercase;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Title-icon tint — matches PanelShell.ps-icon. */
  .sip-head > :global(svg:first-child) {
    color: var(--accent);
    flex-shrink: 0;
  }
  .sip-spacer { flex: 1; }

  /* Action strip directly under the panel head. */
  .sip-head-row {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .sip-kind {
    display: inline-flex; align-items: center; gap: 4px;
    font-family: var(--font-code); font-size: 11px; font-weight: 600;
    padding: 2px 8px;
    background: var(--bg-overlay);
    border-radius: 10px;
  }
  .sip-count { color: var(--text-muted); font-size: 11px; }

  /* Kind badge — same palette as the tree-pane row badges, kept in
     sync via the `kind` data attribute. The wrapper styles are
     duplicated rather than `:global`-leaked so the panel renders
     correctly even when extracted from RonStudioModal's CSS scope. */
  .sip-row-badge {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 22px; height: 16px; padding: 0 4px;
    border-radius: 3px;
    font-family: var(--font-code); font-size: 9px; font-weight: 600; line-height: 1;
    flex-shrink: 0;
  }
  .sip-row-badge-struct,
  .sip-row-badge-map,
  .sip-row-badge-tuple,
  .sip-row-badge-list {
    background: color-mix(in srgb, var(--syntax-keyword) 18%, transparent);
    color: var(--syntax-keyword);
  }
  .sip-row-badge-string,
  .sip-row-badge-char {
    background: color-mix(in srgb, var(--syntax-string) 18%, transparent);
    color: var(--syntax-string);
  }
  .sip-row-badge-number {
    background: color-mix(in srgb, var(--syntax-number) 18%, transparent);
    color: var(--syntax-number);
  }
  .sip-row-badge-bool,
  .sip-row-badge-option {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .sip-row-badge-unit { background: var(--bg-overlay); color: var(--text-muted); }
  .sip-row-badge-named_struct,
  .sip-row-badge-named_tuple,
  .sip-row-badge-unit_variant {
    background: color-mix(in srgb, var(--syntax-type, var(--accent)) 18%, transparent);
    color: var(--syntax-type, var(--accent));
  }
  .sip-row-badge-none {
    background: var(--bg-overlay);
    color: var(--text-disabled);
  }

  .sip-path {
    color: var(--accent); font-family: var(--font-code); font-size: 11px;
    padding: 5px 10px;
    border-bottom: 1px solid var(--border-subtle);
    word-break: break-all; flex-shrink: 0;
    background: var(--bg-base);
  }

  .sip-body {
    padding: 8px 10px;
    overflow: auto;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sip-section-title {
    font-size: 10px; color: var(--text-muted);
    text-transform: uppercase; letter-spacing: 0.06em;
    margin-bottom: 4px;
  }
  .sip-type {
    display: flex; gap: 6px; align-items: center;
    font-size: 11px;
    padding: 6px 8px;
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
  }
  .sip-type-value { font-family: var(--font-code); color: var(--text-primary); }
  .sip-type-value.sip-type-unknown  { color: var(--warning, #d19a66); }
  .sip-type-value.sip-type-external { color: var(--text-disabled); }

  .sip-pre {
    font-family: var(--font-code); font-size: 11px;
    white-space: pre-wrap; margin: 0;
    padding: 8px;
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
    max-height: 240px; overflow: auto;
  }

  .sip-empty {
    color: var(--text-muted);
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 10px; flex: 1;
    padding: 32px;
    font-size: 12px;
  }

  /* ── Inline edit (detail-location) ─────────────────────────────── */
  .sip-value-title {
    display: flex; align-items: center; gap: 8px;
  }
  .sip-edit-btn {
    margin-left: auto;
    display: inline-flex; align-items: center; gap: 3px;
    font-size: 10px;
    padding: 2px 6px;
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }
  .sip-edit-btn:hover { background: var(--bg-elevated); color: var(--text-primary); }
  .sip-edit-banner {
    display: flex; align-items: center; gap: 6px;
    font-size: 10px;
    padding: 4px 6px 4px 8px;
    margin-bottom: 6px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    line-height: 1.3;
  }
  .sip-edit-banner-dismiss {
    margin-left: auto;
    font-size: 10px; padding: 1px 6px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .sip-edit-banner-dismiss:hover { color: var(--text-primary); border-color: var(--accent); }
  .sip-edit-row {
    display: flex; align-items: center; gap: 4px;
  }
  .sip-edit-input {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: 11px;
    padding: 5px 8px;
    background: var(--bg-base);
    border: 1px solid var(--accent);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    outline: none;
  }
  .sip-edit-input:focus {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent);
  }
  .sip-edit-commit,
  .sip-edit-cancel {
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-secondary);
    cursor: pointer;
    flex-shrink: 0;
  }
  .sip-edit-commit {
    color: var(--success, #98c379);
    border-color: color-mix(in srgb, var(--success, #98c379) 35%, var(--border-subtle));
  }
  .sip-edit-commit:hover { background: color-mix(in srgb, var(--success, #98c379) 15%, transparent); }
  .sip-edit-cancel:hover { background: var(--bg-elevated); color: var(--text-primary); }
  .sip-edit-error {
    margin-top: 4px;
    font-size: 10px;
    color: var(--error, #e06c75);
  }

  /* ── Variant picker ───────────────────────────────────────────── */
  .sip-variant-picker {
    width: 100%;
    font-family: var(--font-code); font-size: 11px;
    padding: 5px 8px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    cursor: pointer;
  }
  .sip-variant-picker:hover { border-color: var(--accent); }
  .sip-variant-picker:focus {
    outline: none;
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent);
  }

  /* ── Missing fields ───────────────────────────────────────────── */
  .sip-missing {
    padding: 6px 8px;
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
  }
  .sip-missing-row { display: flex; gap: 8px; align-items: center; font-size: 11px; padding: 2px 0; }
  .sip-missing-row-btn {
    width: 100%;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    padding: 3px 6px;
    text-align: left;
    cursor: pointer;
    color: inherit;
  }
  .sip-missing-row-btn:hover { background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .sip-missing-row-btn:hover :global(.sip-missing-plus) { color: var(--accent); }
  :global(.sip-missing-plus) { color: var(--text-muted); flex-shrink: 0; }
  .sip-missing-name { font-family: var(--font-code); color: var(--text-primary); }
  .sip-missing-type { color: var(--text-muted); font-family: var(--font-code); font-size: 10px; }
  .sip-missing-pill {
    background: color-mix(in srgb, var(--success, #98c379) 20%, transparent);
    color: var(--success, #98c379);
    font-size: 9px; padding: 1px 6px; border-radius: 8px; margin-left: auto;
  }

  /* ── Contents preview (first-level children) ─────────────────── */
  .sip-preview-empty {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
    font-style: italic;
  }
  .sip-preview-list {
    display: flex; flex-direction: column;
    gap: 1px;
    padding: 2px;
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
    max-height: 320px;
    overflow: auto;
  }
  .sip-preview-row {
    display: grid;
    grid-template-columns: auto auto 1fr;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 3px 6px;
    background: transparent;
    color: inherit;
    border: none;
    border-radius: 3px;
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast);
    min-width: 0;
  }
  .sip-preview-row:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .sip-preview-row:disabled {
    cursor: default;
  }
  .sip-preview-key {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .sip-preview-value {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    min-width: 0;
  }
  .sip-preview-more {
    margin-top: 4px;
    padding: 4px 6px;
    font-size: 10px;
    color: var(--text-muted);
    text-align: center;
    font-style: italic;
  }

  /* ── Used by list ─────────────────────────────────────────────── */
  .sip-usages-empty {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
    font-style: italic;
  }
  .sip-usages-list {
    display: flex; flex-direction: column;
    gap: 1px;
    padding: 2px;
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
  }
  .sip-usages-row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 6px;
    background: transparent;
    color: inherit;
    border: none;
    border-radius: 3px;
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast);
  }
  .sip-usages-row:hover { background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .sip-usages-key {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    padding: 1px 5px;
    border-radius: 8px;
    flex-shrink: 0;
  }
  .sip-usages-file {
    font-family: var(--font-ui-sans);
    font-size: 11.5px;
    color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .sip-usages-path {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    max-width: 160px;
  }
</style>
