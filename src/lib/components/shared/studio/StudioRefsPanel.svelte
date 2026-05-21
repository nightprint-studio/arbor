<!--
  StudioRefsPanel — format-agnostic right-side bindings + broken-refs sidecar.

  Two sections in one panel:
    · Configured bindings — every entry in `studioStore.config` (the
      project default, if any, plus each per-glob override). Each row
      expands to list the identifiers ("id"/"name" values) declared
      in the files the binding's scope covers, pulled from the
      cross-ref index. Click a row → opens the definition site via
      `onOpenDefinition` (parent owns the tab + jump flow because the
      workspace store is format-specific).
    · Broken references — every reference field in THIS file whose
      value doesn't resolve to any known id/name in the project.
      Empty state is the happy path; the project-wide totals live in
      the Studio sidebar.

  The two sections share state (`expandedBindings` Set, header,
  Rescan button, scan lifecycle) tightly enough that splitting into
  two panels would force lifting them out or duplicating them.
  Same rationale as `StudioDiffPane` (one pane, two sub-views).

  All state — bindingEntries derived, expandedBindings Set, helpers,
  cross/broken-refs lazy load — is panel-owned. The parent only
  injects the format identifier, sourcePath (to filter broken refs
  to the current doc), an `onOpenDefinition` callback, and an
  optional `emptyState` snippet for format-specific copy.

  Note: `studioStore` itself is shared across all formats (it lives
  in `$lib/stores/studio.svelte.ts`, not RON-specific), so the panel
  can read `studioStore.config`, `studioStore.crossRefs`,
  `studioStore.brokenRefs`, etc. directly.
-->
<script lang="ts" module>
  import type { Snippet } from 'svelte';
  import type { StudioBackend, StudioFormat } from '$lib/ipc/studio-format';
  import type { CrossRefDef } from '$lib/ipc/studio';

  export interface StudioRefsPanelProps<TKind extends string = string> {
    /** Identifies the format (used only for the default empty-state
     *  fallback copy). */
    formatId: StudioFormat;
    /** Pre-bound backend. The panel does not currently call it — the
     *  refs index lives in the shared `studioStore`. Kept on the
     *  interface for parity with the other shared panels and as an
     *  escape hatch for future per-format needs. */
    backend: StudioBackend<TKind>;

    /** Absolute path of the current document — used to filter
     *  `studioStore.brokenRefs` to the file the user is editing. */
    sourcePath: string | null;

    /** Opens a definition site as a tab + jumps to it. Parent owns
     *  this flow: each format has its own workspace store (RON's
     *  `ronStudioWorkspaceStore`, JSON's own, …) that decides how a
     *  file becomes a tab. */
    onOpenDefinition: (d: CrossRefDef) => Promise<void> | void;

    /** Replaces the empty-state copy ("No schema bindings configured
     *  for this project. Use the Studio sidebar's right-click menu
     *  on a `.{ext}` file or folder to bind it to a Rust source.").
     *  Provide it when the format's binding source isn't a Rust
     *  source (e.g. JSON Schema for JSON). */
    emptyState?: Snippet;
  }
</script>

<script lang="ts" generics="TKind extends string">
  import { untrack } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import {
    Layers, RotateCcw, ChevronDown, ChevronRight, AlertCircle, Check,
  } from 'lucide-svelte';
  import Spinner from '../ui/Spinner.svelte';
  import Button from '../ui/Button.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { animStore } from '$lib/stores/animations.svelte';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import type { StudioFileKind } from '$lib/ipc/studio';

  let {
    formatId,
    backend: _backend,
    sourcePath,
    onOpenDefinition,
    emptyState,
  }: StudioRefsPanelProps<TKind> = $props();

  // Silence "unused" — kept on the interface for future per-format hooks.
  void untrack(() => _backend);

  // Map the FE format id onto the host's `StudioFileKind`. Each
  // format ships its own cross-ref namespace (RON 2B-3, JSON 3.c.a,
  // TOML 4.c.a, YAML 5.c, .properties Phase 6).
  const refsKind: StudioFileKind = $derived(
    formatId === 'json'       ? 'json' :
    formatId === 'toml'       ? 'toml' :
    formatId === 'yaml'       ? 'yaml' :
    formatId === 'properties' ? 'properties' :
    'ron'
  );

  const kindCrossRefs   = $derived(studioStore.crossRefsForKind(refsKind));
  const kindBrokenRefs  = $derived(studioStore.brokenRefsForKind(refsKind));
  const kindCrossRefsLoading  = $derived(studioStore.crossRefsLoadingForKind(refsKind));
  const kindBrokenRefsLoading = $derived(studioStore.brokenRefsLoadingForKind(refsKind));
  const kindBrokenRefsTabId   = $derived(studioStore.brokenRefsTabIdForKind(refsKind));

  // ── Bindings ────────────────────────────────────────────────────

  /** Flat list of every schema binding configured for the active
   *  repo. Default binding (when set) lands at the top; per-glob
   *  overrides follow in file order. */
  type BindingEntry = {
    scope:    string;
    kind:     'default' | 'override';
    rsFile:   string;
    rootType: string;
  };
  const bindingEntries = $derived.by<BindingEntry[]>(() => {
    const out: BindingEntry[] = [];
    const cfg = studioStore.config;
    if (cfg.default) {
      out.push({
        scope:    '*',
        kind:     'default',
        rsFile:   cfg.default.rs_file,
        rootType: cfg.default.root_type,
      });
    }
    for (const o of cfg.overrides ?? []) {
      out.push({
        scope:    o.glob,
        kind:     'override',
        rsFile:   o.rs_file,
        rootType: o.root_type,
      });
    }
    return out;
  });

  /** Tracks which bindings the user has expanded. Keyed by
   *  `kind\x00scope` so the default entry doesn't collide with an
   *  override sharing the literal pattern "*". Also stores
   *  `broken\x00value` keys for the broken-ref section's grouped
   *  rows — same expansion mechanism, different ID space. */
  let expandedBindings = $state<Set<string>>(new Set());

  function bindingKey(b: BindingEntry): string {
    return b.kind + '\x00' + b.scope;
  }
  function toggleBinding(b: BindingEntry) {
    const k = bindingKey(b);
    const next = new Set(expandedBindings);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    expandedBindings = next;
    // Lazy-load the cross-ref index the first time a binding is
    // expanded — it's needed to enumerate the identifiers declared
    // in files matching the binding's scope. Idempotent; the store
    // dedupes via `crossRefsTabId`.
    if (next.has(k)) {
      const tid = tabsStore.activeTabId;
      if (tid) untrack(() => { void studioStore.loadCrossRefsForKind(tid, refsKind); });
    }
  }

  /** Compute the cross-reference definitions whose source file is
   *  covered by a binding's scope. For overrides this is a direct
   *  glob match on the file's `relative_path`; for the default
   *  binding it's every def NOT already claimed by some override
   *  (matches the host-side priority order: overrides win first).
   *  Sorted by id_value so the expanded list reads alphabetically. */
  function defsForBinding(b: BindingEntry): CrossRefDef[] {
    const overrides = studioStore.config.overrides ?? [];
    const out: CrossRefDef[] = [];
    for (const defs of kindCrossRefs.values()) {
      for (const d of defs) {
        if (b.kind === 'default') {
          const claimed = overrides.some(o => studioStore.globMatch(o.glob, d.relative_path));
          if (!claimed) out.push(d);
        } else {
          if (studioStore.globMatch(b.scope, d.relative_path)) out.push(d);
        }
      }
    }
    out.sort((a, b) => a.id_value.localeCompare(b.id_value));
    return out;
  }

  function toggleBrokenGroup(value: string) {
    const k = 'broken\x00' + value;
    const next = new Set(expandedBindings);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    expandedBindings = next;
  }

  // ── Broken refs (filtered to current doc) ─────────────────────────

  /** Broken refs filtered to the currently open document. The panel
   *  is a per-doc view — surfacing every project-wide orphan here
   *  would bury the relevant ones under noise from files the user
   *  hasn't touched. The Studio sidebar shows the full project-wide
   *  list. Path-normalise on both sides so Windows backslashes don't
   *  miss a match. */
  const docBrokenRefs = $derived.by(() => {
    if (!sourcePath) return [] as typeof kindBrokenRefs;
    const norm = sourcePath.replace(/\\/g, '/');
    return kindBrokenRefs.filter(r =>
      r.absolute_path.replace(/\\/g, '/') === norm,
    );
  });

  /** Group the per-doc broken-ref list by `value` so the same
   *  missing target collapses into one card with all its offending
   *  sites listed beneath. */
  const groupedBroken = $derived.by(() => {
    const m = new Map<string, typeof kindBrokenRefs>();
    for (const r of docBrokenRefs) {
      const arr = m.get(r.value);
      if (arr) arr.push(r);
      else m.set(r.value, [r]);
    }
    return [...m.entries()];
  });

  // ── Lifecycle: lazy-load scan on mount ────────────────────────────

  /** The panel is conditionally rendered (only when the right-rail
   *  button is active), so this effect fires once on open. The store
   *  dedupes server-side via `crossRefsTabId` / `brokenRefsTabId`
   *  → cheap if the indices are already populated for this tab. */
  $effect(() => {
    const tid = tabsStore.activeTabId;
    if (!tid) return;
    const kind = refsKind;
    untrack(() => {
      void studioStore.loadCrossRefsForKind(tid, kind);
      void studioStore.loadBrokenRefsForKind(tid, kind);
    });
  });

  function rescan() {
    const tid = tabsStore.activeTabId;
    if (!tid) return;
    void studioStore.loadCrossRefsForKind(tid, refsKind, true);
    void studioStore.loadBrokenRefsForKind(tid, refsKind, true);
  }

  function basename(p: string | null): string {
    if (!p) return '';
    const norm = p.replace(/\\/g, '/');
    const slash = norm.lastIndexOf('/');
    return slash === -1 ? norm : norm.slice(slash + 1);
  }
</script>

<div class="srp-root">
  <div class="srp-head">
    <Layers size={13} />
    <span class="srp-title">Bindings</span>
    <span class="srp-spacer"></span>
    <Button variant="icon" size="md"
            onclick={rescan}
            disabled={!tabsStore.activeTabId || kindBrokenRefsLoading || kindCrossRefsLoading}
            tooltip="Re-scan references — picks up file changes since the panel was last opened"
            ariaLabel="Rescan references">
      {#snippet iconStart()}<RotateCcw size={11} class={kindBrokenRefsLoading || kindCrossRefsLoading ? 'spin' : ''} />{/snippet}
    </Button>
  </div>
  <div class="srp-body">
    {#if bindingEntries.length === 0}
      {#if emptyState}
        {@render emptyState()}
      {:else}
        <p class="srp-empty">
          No schema bindings configured for this project. Use the
          <strong>Studio</strong> sidebar's right-click menu on a
          <code>.{formatId}</code> file or folder to bind it to a
          schema source.
        </p>
      {/if}
    {:else}
      <div class="srp-section">
        <span class="srp-section-text">Configured bindings</span>
        <span class="srp-section-count">{bindingEntries.length}</span>
      </div>
      <div class="srp-list">
        {#each bindingEntries as b (bindingKey(b))}
          {@const isOpen = expandedBindings.has(bindingKey(b))}
          {@const defs = isOpen ? defsForBinding(b) : []}
          <div class="srp-block">
            <button
              type="button"
              class="srp-row"
              class:expanded={isOpen}
              onclick={() => toggleBinding(b)}
              use:tooltip={b.rsFile}
              aria-expanded={isOpen}
            >
              <span class="srp-caret" aria-hidden="true">
                {#if isOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
              </span>
              <div class="srp-text">
                <div class="srp-scope">
                  {#if b.kind === 'default'}
                    <span class="srp-scope-tag srp-scope-default">default</span>
                    <span class="srp-scope-hint">(repo-wide)</span>
                  {:else}
                    <span class="srp-scope-glob">{b.scope}</span>
                  {/if}
                </div>
                <div class="srp-target">
                  <span class="srp-file">{basename(b.rsFile)}</span>
                  <span class="srp-sep">·</span>
                  <span class="srp-type">{b.rootType}</span>
                </div>
              </div>
            </button>
            {#if isOpen}
              <div class="srp-defs-wrap"
                   transition:slide={{ axis: 'y', duration: animStore.dPanel, easing: cubicOut }}>
                {#if kindCrossRefsLoading && defs.length === 0}
                  <div class="srp-defs-loading">
                    <Spinner size="xs" /> <span>Loading identifiers…</span>
                  </div>
                {:else if defs.length === 0}
                  <div class="srp-defs-empty">
                    No identifiers declared in files matching this scope yet.
                  </div>
                {:else}
                  <div class="srp-defs">
                    {#each defs as d (d.absolute_path + '\x00' + d.def_path.join('/'))}
                      <!-- Mini-card: id_value on its own line so
                           long values stay readable; field tag +
                           file name below as secondary metadata. -->
                      <button
                        type="button"
                        class="srp-def"
                        onclick={() => void onOpenDefinition(d)}
                        use:tooltip={d.relative_path}
                      >
                        <span class="srp-def-id">{d.id_value}</span>
                        <div class="srp-def-meta">
                          <span class="srp-def-field">{d.def_field}</span>
                          <span class="srp-def-sep">·</span>
                          <span class="srp-def-file">{d.file_name}</span>
                        </div>
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <!-- Broken-references section — every reference field whose
         value doesn't match any known id/name in the project.
         Loaded lazily on panel open; rendered as a separate section
         so the user reads it as a validation result, not as a
         binding entry. Empty state is the happy path: no orphans,
         nothing shown. -->
    {#if kindBrokenRefsLoading && docBrokenRefs.length === 0}
      <div class="srp-section srp-section-loading">
        <span class="srp-section-text">Broken references in this file</span>
        <Spinner size="xs" />
      </div>
    {:else if kindBrokenRefsTabId !== null && docBrokenRefs.length === 0}
      <!-- Scanned, nothing dangling in THIS file — confirmatory
           rather than a silent empty so the user knows the check
           actually ran. Project-wide totals live in the Studio
           sidebar's broken-refs section. -->
      <div class="srp-allgood">
        <Check size={11} />
        <span>
          All references in this file resolve{#if kindBrokenRefs.length > 0}
            <span class="srp-allgood-aside"> · {kindBrokenRefs.length} broken elsewhere</span>
          {/if}.
        </span>
      </div>
    {:else if docBrokenRefs.length > 0}
      <div class="srp-section srp-section-broken">
        <span class="srp-section-text">Broken references in this file</span>
        <span class="srp-section-count srp-section-count-warn">
          {docBrokenRefs.length}
        </span>
        {#if kindBrokenRefs.length > docBrokenRefs.length}
          <span class="srp-section-aside">
            · {kindBrokenRefs.length - docBrokenRefs.length} elsewhere
          </span>
        {/if}
      </div>
      <div class="srp-list">
        {#each groupedBroken as [value, sites] (value)}
          {@const brokenKey = 'broken\x00' + value}
          {@const isBrokenOpen = expandedBindings.has(brokenKey)}
          <div class="srp-block">
            <button
              type="button"
              class="srp-row srp-row-broken"
              class:expanded={isBrokenOpen}
              onclick={() => toggleBrokenGroup(value)}
              aria-expanded={isBrokenOpen}
            >
              <span class="srp-caret" aria-hidden="true">
                {#if isBrokenOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
              </span>
              <div class="srp-text">
                <div class="srp-scope">
                  <AlertCircle size={11} class="srp-broken-icon" />
                  <span class="srp-scope-glob srp-broken-value">{value}</span>
                </div>
                <div class="srp-target">
                  <span class="srp-broken-hint">
                    {sites.length} ref{sites.length === 1 ? '' : 's'} · no matching id/name in project
                  </span>
                </div>
              </div>
            </button>
            {#if isBrokenOpen}
              <div class="srp-defs-wrap"
                   transition:slide={{ axis: 'y', duration: animStore.dPanel, easing: cubicOut }}>
                <div class="srp-defs">
                  {#each sites as s (s.absolute_path + '\x00' + s.field_path.join('/'))}
                    <button
                      type="button"
                      class="srp-def srp-def-broken"
                      onclick={() => void onOpenDefinition({
                        id_value:      s.value,
                        absolute_path: s.absolute_path,
                        relative_path: s.relative_path,
                        file_name:     s.file_name,
                        kind:          refsKind,
                        def_path:      s.field_path,
                        def_field:     s.key_name,
                      })}
                      use:tooltip={s.relative_path}
                    >
                      <span class="srp-def-id srp-def-id-broken">{s.value}</span>
                      <div class="srp-def-meta">
                        <span class="srp-def-field srp-def-field-broken">{s.key_name}</span>
                        <span class="srp-def-sep">·</span>
                        <span class="srp-def-file">{s.file_name}</span>
                      </div>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  /* Outer flex column — sits inside the modal's right-rail card slot.
     Same shape as `<StudioSchemaPanel>` so swapping rails feels
     identical. */
  .srp-root {
    display: flex; flex-direction: column;
    min-height: 0;
    flex: 1;
  }

  /* Header — matches the parent's `.rs-panel-head` chrome (also used
     by inspector + query) so the row reads as part of the same
     family even though the class names differ. */
  .srp-head {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    font-size: 11px; font-weight: 600;
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .srp-title { font-weight: 600; color: var(--text-primary); }
  .srp-spacer { flex: 1; }
  .srp-head > :global(svg:first-child) { color: var(--accent); flex-shrink: 0; }

  .srp-body {
    padding: 10px 8px 8px;
    overflow: auto;
    display: flex; flex-direction: column;
    gap: 10px;
    flex: 1;
  }

  .srp-empty {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.5;
    padding: 4px 4px 0;
  }
  .srp-empty :global(strong) { color: var(--text-primary); font-weight: 600; }
  .srp-empty :global(code) {
    font-family: var(--font-code);
    font-size: 10.5px;
    background: var(--bg-overlay);
    padding: 0 4px;
    border-radius: 3px;
  }

  /* Section header (Configured bindings · Broken references) —
     uppercase mini-label + count chip + optional aside. */
  .srp-section {
    display: flex; align-items: center; gap: 6px;
    margin: 6px 0 2px;
    padding: 0 2px;
  }
  .srp-section-text {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-disabled);
  }
  .srp-section-count {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 0 5px;
    border-radius: 8px;
    line-height: 14px;
  }

  /* Bindings list — flat rows, no card chrome. Each block has a
     clickable summary row + an optional expanded list of declared
     identifiers below. */
  .srp-list {
    display: flex; flex-direction: column;
    gap: 4px;
  }
  .srp-block {
    display: flex; flex-direction: column;
    border-radius: var(--radius-sm);
  }
  .srp-row {
    /* Clickable header — chevron + text block laid out as a row.
       `min-width: 0` on both is key to letting long canonical paths
       ellipsise instead of overflowing the panel. */
    display: flex; align-items: flex-start; gap: 6px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    text-align: left;
    cursor: pointer;
    color: inherit;
    transition: background var(--transition-fast);
    min-width: 0;
  }
  .srp-row:hover { background: var(--bg-hover); }
  .srp-caret {
    display: flex; align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
    padding-top: 1px;
  }
  .srp-row.expanded .srp-caret { color: var(--accent); }
  .srp-text {
    display: flex; flex-direction: column;
    gap: 3px;
    flex: 1;
    min-width: 0;
  }

  .srp-scope {
    display: flex; align-items: baseline; gap: 6px;
    font-size: 11.5px;
    min-width: 0;
  }
  .srp-scope-glob {
    font-family: var(--font-code);
    color: var(--text-primary);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .srp-scope-tag {
    display: inline-flex; align-items: center;
    font-family: var(--font-code);
    font-size: 9.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 6px;
    border-radius: 3px;
    flex-shrink: 0;
  }
  .srp-scope-default {
    color: var(--success, #98c379);
    background: color-mix(in srgb, var(--success, #98c379) 14%, transparent);
  }
  .srp-scope-hint {
    color: var(--text-disabled);
    font-size: 10px;
    flex-shrink: 0;
  }

  .srp-target {
    display: flex; align-items: baseline; gap: 4px;
    font-size: 10.5px;
    color: var(--text-muted);
    padding-left: 2px;
    min-width: 0;
  }
  .srp-file {
    font-family: var(--font-code);
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .srp-sep { color: var(--text-disabled); flex-shrink: 0; }
  .srp-type {
    font-family: var(--font-code);
    color: var(--syntax-type, var(--accent));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  /* Wrap node that holds the slide-transition. Critical to ensure
     `transition:slide` measures a node whose height is set by its
     children. */
  .srp-defs-wrap {
    overflow: hidden;
  }
  /* Expanded list of declared identifiers — one mini-card per
     cross-ref def matched by this binding's scope. */
  .srp-defs {
    display: flex; flex-direction: column;
    gap: 3px;
    padding: 4px 4px 8px 22px;        /* indent past the chevron */
    max-height: 320px;
    overflow-y: auto;
  }
  .srp-def {
    display: flex; flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 6px 8px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: inherit;
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    min-width: 0;
  }
  .srp-def:hover {
    background: var(--bg-hover);
    border-color: color-mix(in srgb, var(--accent) 32%, var(--border-subtle));
  }
  .srp-def-id {
    font-family: var(--font-code);
    font-size: 11px;
    font-weight: 500;
    color: var(--accent);
    overflow-wrap: anywhere;
    line-height: 1.3;
  }
  .srp-def-meta {
    display: flex; align-items: baseline; gap: 5px;
    font-size: 10px;
    color: var(--text-muted);
    min-width: 0;
  }
  .srp-def-field {
    display: inline-flex; align-items: center;
    font-family: var(--font-code);
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 1px 5px;
    border-radius: 3px;
    color: var(--syntax-keyword, var(--accent));
    background: color-mix(in srgb, var(--syntax-keyword, var(--accent)) 14%, transparent);
    flex-shrink: 0;
  }
  .srp-def-sep { color: var(--text-disabled); flex-shrink: 0; }
  .srp-def-file {
    font-family: var(--font-code);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .srp-defs-loading,
  .srp-defs-empty {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 8px 10px 22px;
    font-size: 10.5px;
    color: var(--text-muted);
    font-style: italic;
  }

  /* ── Broken references section ─────────────────────────────────
     Same structural rhythm as the configured-bindings list above
     but coloured to read as a warning. */
  .srp-section-broken { margin-top: 14px; }
  .srp-section-loading {
    margin-top: 14px;
    opacity: 0.8;
  }
  .srp-allgood {
    display: flex; align-items: center; gap: 6px;
    margin-top: 14px;
    padding: 6px 8px;
    font-size: 11px;
    color: var(--success, #98c379);
    background: color-mix(in srgb, var(--success, #98c379) 10%, transparent);
    border-radius: var(--radius-sm);
  }
  .srp-allgood-aside {
    color: var(--warning, #e5c07b);
    font-style: italic;
  }
  .srp-section-aside {
    margin-left: auto;
    font-size: 9.5px;
    font-style: italic;
    color: var(--text-disabled);
  }
  .srp-section-count-warn {
    color: var(--warning, #e5c07b);
    background: color-mix(in srgb, var(--warning, #e5c07b) 18%, var(--bg-overlay));
  }
  .srp-row-broken.expanded .srp-caret { color: var(--warning, #e5c07b); }
  :global(.srp-broken-icon) {
    color: var(--warning, #e5c07b);
    flex-shrink: 0;
  }
  .srp-broken-value {
    color: var(--warning, #e5c07b) !important;
  }
  .srp-broken-hint {
    color: var(--text-muted);
    font-style: italic;
    font-size: 10px;
  }
  .srp-def-broken {
    border-color: color-mix(in srgb, var(--warning, #e5c07b) 20%, var(--border-subtle));
  }
  .srp-def-broken:hover {
    border-color: var(--warning, #e5c07b);
  }
  .srp-def-id-broken { color: var(--warning, #e5c07b); }
  .srp-def-field-broken {
    color: var(--warning, #e5c07b);
    background: color-mix(in srgb, var(--warning, #e5c07b) 14%, transparent);
  }
</style>
