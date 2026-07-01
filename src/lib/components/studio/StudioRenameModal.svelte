<!--
  StudioRenameModal — F12 cross-reference rename refactor (FROZEN F12).

  Format-agnostic preview/apply UI shared across every studio format
  whose backend declares `descriptor.supports_rename_reference = true`.
  RON is the prototype consumer (Phase 2B-3); JSON/TOML/YAML/.properties
  inherit the modal as-is once their backends ship the capability.

  Wiring contract — the format wrapper supplies:
    · `backend`     — `studioBackend(formatId)` (provides
                      `renamePreview` / `renameApply`)
    · `tabId`       — active repo tab id (BE resolves repo root from it)
    · `formatLabel` — short display string ("RON" / "JSON" / …)
    · `oldValue`    — the value the user right-clicked on
    · `openDocs`    — every open doc snapshot (for dirty-blocker check)
    · `onClose`     — closed without applying
    · `onApplied`   — apply succeeded; carries `{ written_files,
                      failed_files }` so the caller can refresh open
                      tabs whose source matches the touched paths

  Behaviour:
    · Preview runs at mount + on `newValue` debounce (collisions update
      live as the user types). Errors caught and surfaced inline.
    · Per-site skip checkboxes; per-file aggregate checkbox toggles
      every nested site at once.
    · Dirty-doc blocker — apply button disabled until the user saves /
      discards the listed open docs (FROZEN F12 — refactor blocks
      until the offending docs are clean).
    · Collisions — sticky warning banner; Apply still allowed
      (FROZEN F12 — "Continue anyway?" is the implicit answer once
      the user confirms).
    · Apply is best-effort sequential with rollback PRE-flush on the
      BE. Per-file flush failures surface in the success toast.
-->
<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    AlertCircle, AlertTriangle, ChevronDown, ChevronRight, FileText,
    Replace, Sigma, X as XIcon,
  } from 'lucide-svelte';

  import Modal from '$lib/components/shared/Modal.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Tree, { type RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';

  import type { StudioBackend } from '$lib/ipc/studio/studio-format';
  import type {
    RenameOpenDoc,
    RenamePreview,
    RenameResult,
    RenameSite,
  } from '$lib/types/studio/studio-format';

  // ── Props ──────────────────────────────────────────────────────────

  interface Props {
    backend:     StudioBackend;
    tabId:       string;
    formatLabel: string;
    oldValue:    string;
    openDocs:    RenameOpenDoc[];
    onClose:     () => void;
    onApplied:   (result: RenameResult) => void;
  }

  let { backend, tabId, formatLabel, oldValue, openDocs, onClose, onApplied }: Props = $props();

  // ── Local state ────────────────────────────────────────────────────

  let newValue          = $state(untrack(() => oldValue));
  let preview           = $state<RenamePreview | null>(null);
  let previewing        = $state(true);
  let previewError      = $state<string | null>(null);
  let applying          = $state(false);
  let applyError        = $state<string | null>(null);

  /** Site keys the user has chosen to skip. The site list is the
   *  source of truth; this set just records exclusions. */
  let skipped = $state<Set<string>>(new Set());

  function siteKey(s: RenameSite): string {
    return `${s.absolute_path}${s.field_path.join('/')}`;
  }

  // ── Preview lifecycle ──────────────────────────────────────────────

  let previewTok = 0;

  async function loadPreview(hint: string | null): Promise<void> {
    const token = ++previewTok;
    previewing = true;
    previewError = null;
    try {
      const p = await backend.renamePreview({
        tabId,
        oldValue,
        newValueHint: hint,
        openDocs,
      });
      if (token !== previewTok) return;
      preview = p;
      // Reset skip set on first load only — user-edited skips survive
      // subsequent re-previews triggered by `newValue` typing (the
      // site list shape doesn't change with the new value).
      if (skipped.size === 0) {
        skipped = new Set();
      } else {
        // Drop any keys whose sites no longer appear (defensive — site
        // list shape is stable today, but we don't want stale keys to
        // accumulate if it ever changes).
        const live = new Set(p.sites.map(siteKey));
        const next = new Set<string>();
        for (const k of skipped) if (live.has(k)) next.add(k);
        skipped = next;
      }
    } catch (e) {
      if (token !== previewTok) return;
      previewError = String(e);
      preview = null;
    } finally {
      if (token === previewTok) previewing = false;
    }
  }

  // Initial preview — fires once on mount with the old value as the
  // (no-collision) hint, so the first paint already shows the site list.
  onMount(() => { void loadPreview(null); });

  // Re-fire preview when `newValue` changes — debounced so live typing
  // doesn't thrash the BE. The BE call is cheap (index-cached) but we
  // still gate it.
  let typeTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const v = newValue.trim();
    if (typeTimer) clearTimeout(typeTimer);
    typeTimer = setTimeout(() => {
      // Only ask for collisions when the new value differs from the old.
      const hint = v.length === 0 || v === oldValue ? null : v;
      void loadPreview(hint);
    }, 200);
  });

  // ── Derived UI state ───────────────────────────────────────────────

  /** Group sites by file for the tree view. Stable order = whatever
   *  the BE returned (already sorted by file then path). */
  type FileGroup = {
    id:            string;
    absolute_path: string;
    relative_path: string;
    file_name:     string;
    sites:         RenameSite[];
  };
  type Node =
    | { kind: 'file'; id: string; group: FileGroup; children: Node[] }
    | { kind: 'site'; id: string; site: RenameSite };

  const groups = $derived.by<FileGroup[]>(() => {
    const sites = preview?.sites ?? [];
    const map = new Map<string, FileGroup>();
    const order: string[] = [];
    for (const s of sites) {
      const key = s.absolute_path;
      let g = map.get(key);
      if (!g) {
        g = {
          id:            `f:${key}`,
          absolute_path: s.absolute_path,
          relative_path: s.relative_path,
          file_name:     s.file_name,
          sites:         [],
        };
        map.set(key, g);
        order.push(key);
      }
      g.sites.push(s);
    }
    return order.map(k => map.get(k)!);
  });

  const treeNodes = $derived.by<Node[]>(() =>
    groups.map<Node>(g => ({
      kind:     'file',
      id:       g.id,
      group:    g,
      children: g.sites.map<Node>(s => ({
        kind: 'site',
        id:   `s:${siteKey(s)}`,
        site: s,
      })),
    })),
  );

  const totalSites      = $derived(preview?.sites.length ?? 0);
  const totalFiles      = $derived(groups.length);
  const skippedCount    = $derived(skipped.size);
  const activeSites     = $derived.by<RenameSite[]>(
    () => (preview?.sites ?? []).filter(s => !skipped.has(siteKey(s))),
  );
  const activeSiteCount = $derived(activeSites.length);
  const activeFileCount = $derived.by<number>(
    () => new Set(activeSites.map(s => s.absolute_path)).size,
  );

  const hasDirtyBlockers   = $derived((preview?.dirty_blockers.length ?? 0) > 0);
  const hasCollisions      = $derived((preview?.collisions.length ?? 0) > 0);

  const trimmedNew         = $derived(newValue.trim());
  const newValueValid      = $derived(
    trimmedNew.length > 0 && trimmedNew !== oldValue,
  );
  const canApply           = $derived(
    !applying
    && !previewing
    && newValueValid
    && activeSiteCount > 0
    && !hasDirtyBlockers,
  );

  // ── Skip toggles ───────────────────────────────────────────────────

  function toggleSite(s: RenameSite): void {
    const key = siteKey(s);
    const next = new Set(skipped);
    if (next.has(key)) next.delete(key); else next.add(key);
    skipped = next;
  }

  function fileState(g: FileGroup): 'all' | 'none' | 'some' {
    let on = 0;
    for (const s of g.sites) if (!skipped.has(siteKey(s))) on++;
    if (on === 0)              return 'none';
    if (on === g.sites.length) return 'all';
    return 'some';
  }

  function toggleFile(g: FileGroup): void {
    const next = new Set(skipped);
    const state = fileState(g);
    if (state === 'all') {
      // currently all included → skip everything in this file
      for (const s of g.sites) next.add(siteKey(s));
    } else {
      // include everything (some / none → all)
      for (const s of g.sites) next.delete(siteKey(s));
    }
    skipped = next;
  }

  // ── Apply ──────────────────────────────────────────────────────────

  async function runApply(): Promise<void> {
    if (!canApply) return;
    applying = true;
    applyError = null;
    try {
      const result = await backend.renameApply({
        tabId,
        oldValue,
        newValue: trimmedNew,
        sites:    activeSites,
        openDocs,
      });
      onApplied(result);
    } catch (e) {
      applyError = String(e);
    } finally {
      applying = false;
    }
  }

  function onKeydown(e: KeyboardEvent): void {
    // Escape is handled by Modal.svelte's own keydown handler; we only
    // intercept Ctrl/Cmd+Enter as the apply shortcut.
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void runApply();
    }
  }

  // Default expanded state — every file expanded so the user can see
  // sites at a glance. Recomputed when the file list changes, so a
  // re-preview after typing keeps the surface visible without forcing
  // the user to re-expand. Tree's `expandedIds` controlled mode would
  // pin state across renders; uncontrolled `defaultExpanded` re-seeds
  // only when ids appear for the first time, which matches the UX we
  // want here.
  const defaultExpandedIds = $derived.by<string[]>(() => groups.map(g => g.id));
</script>

<svelte:window onkeydown={onKeydown} />

<Modal
  {onClose}
  width="min(820px, 95vw)"
  height="min(720px, 90vh)"
  padBody={false}
  ariaLabel="Rename across project"
>
  {#snippet header()}
    <div class="rm-header">
      <div class="rm-title">
        <Replace size={16} />
        <span>Rename across project</span>
        <span class="rm-format-tag">{formatLabel}</span>
      </div>
      <button class="rm-close" type="button" aria-label="Close" onclick={onClose}>
        <XIcon size={14} />
      </button>
    </div>
  {/snippet}

  <div class="rm-body">
    <!-- New value input + summary -->
    <div class="rm-form">
      <div class="rm-row">
        <span class="rm-row-label">From</span>
        <code class="rm-old">{oldValue}</code>
      </div>
      <div class="rm-row">
        <span class="rm-row-label">To</span>
        <Input
          bind:value={newValue}
          placeholder="New value"
          autofocus
          size="md"
          error={
            !newValueValid
              ? (trimmedNew.length === 0
                  ? 'Enter a new value'
                  : 'Must differ from the old value')
              : null
          }
        />
      </div>

      <div class="rm-summary">
        <Sigma size={13} />
        {#if previewing && !preview}
          <span>Scanning project…</span>
        {:else if previewError}
          <span class="rm-summary-err">{previewError}</span>
        {:else if totalSites === 0}
          <span>No occurrences of <code>{oldValue}</code> in this project.</span>
        {:else}
          <span>
            Renaming <code>{oldValue}</code> → <code>{trimmedNew || '…'}</code>
            in <strong>{activeSiteCount}</strong>
            {activeSiteCount === 1 ? 'site' : 'sites'}
            across <strong>{activeFileCount}</strong>
            {activeFileCount === 1 ? 'file' : 'files'}
            {#if skippedCount > 0}
              <span class="rm-skip-tag">({skippedCount} skipped)</span>
            {/if}
          </span>
        {/if}
      </div>
    </div>

    <!-- Dirty-doc blocker -->
    {#if hasDirtyBlockers}
      <div class="rm-banner rm-banner-error">
        <AlertCircle size={14} />
        <div class="rm-banner-body">
          <strong>Unsaved changes block this refactor.</strong>
          Save or discard the following open
          {(preview?.dirty_blockers.length ?? 0) === 1 ? 'doc' : 'docs'}, then re-open this dialog:
          <ul class="rm-blocker-list">
            {#each preview?.dirty_blockers ?? [] as d (d.doc_id)}
              <li><code>{d.source_path ?? '(untitled doc)'}</code></li>
            {/each}
          </ul>
        </div>
      </div>
    {/if}

    <!-- Collision warning -->
    {#if hasCollisions}
      <div class="rm-banner rm-banner-warn">
        <AlertTriangle size={14} />
        <div class="rm-banner-body">
          <strong>Target <code>{trimmedNew}</code> already exists</strong>
          in {(preview?.collisions.length ?? 0) === 1 ? '1 site' : `${preview?.collisions.length ?? 0} sites`}.
          Continuing will merge the namespaces — every reference to the
          old value will resolve to the existing definition(s).
          <details class="rm-collision-details">
            <summary>Show {(preview?.collisions.length ?? 0) === 1 ? 'site' : 'sites'}</summary>
            <ul class="rm-collision-list">
              {#each preview?.collisions ?? [] as c}
                <li>
                  <code>{c.relative_path}</code>
                  <span class="rm-collision-path">→ {c.field_path.join('.')}</span>
                </li>
              {/each}
            </ul>
          </details>
        </div>
      </div>
    {/if}

    <!-- Apply error (per-call failure, not a partial flush) -->
    {#if applyError}
      <div class="rm-banner rm-banner-error">
        <AlertCircle size={14} />
        <div class="rm-banner-body">
          <strong>Apply failed.</strong>
          {applyError}
        </div>
      </div>
    {/if}

    <!-- Site preview (virtualised) -->
    <div class="rm-list">
      {#if previewing && !preview}
        <StateBlock tone="loading" label="Scanning project…">
          {#snippet spinner()}<Spinner size={14} />{/snippet}
        </StateBlock>
      {:else if previewError}
        <StateBlock tone="error" label={previewError} />
      {:else if totalSites === 0}
        <StateBlock tone="neutral" label={`No occurrences of "${oldValue}" found.`} />
      {:else}
        <Tree
          nodes={treeNodes}
          getId={(n) => n.id}
          getChildren={(n) => n.kind === 'file' ? n.children : null}
          defaultExpanded={defaultExpandedIds}
          rowHeight={26}
          indentSize={16}
          basePadding={8}
          showChevron={false}
          ariaLabel="Sites affected by rename"
        >
          {#snippet row({ node, expanded, toggle }: RowSnippetCtx<Node>)}
            {#if node.kind === 'file'}
              {@const fst = fileState(node.group)}
              <button
                type="button"
                class="rm-chev"
                aria-label={expanded ? 'Collapse file' : 'Expand file'}
                onclick={(e) => { e.stopPropagation(); toggle(); }}
              >
                {#if expanded}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
              </button>
              <input
                type="checkbox"
                class="rm-check"
                aria-label="Toggle all sites in this file"
                checked={fst === 'all'}
                indeterminate={fst === 'some'}
                onclick={(e) => e.stopPropagation()}
                onchange={() => toggleFile(node.group)}
              />
              <FileText size={13} class="rm-file-icon" />
              <span class="rm-file-name">{node.group.relative_path}</span>
              <span class="rm-file-count">
                {node.group.sites.length}
                {node.group.sites.length === 1 ? 'site' : 'sites'}
              </span>
            {:else}
              {@const skip = skipped.has(siteKey(node.site))}
              <span class="rm-chev rm-chev-spacer"></span>
              <input
                type="checkbox"
                class="rm-check"
                aria-label={skip ? 'Include this site' : 'Skip this site'}
                checked={!skip}
                onclick={(e) => e.stopPropagation()}
                onchange={() => toggleSite(node.site)}
              />
              <span class={`rm-scope rm-scope-${node.site.scope}`}>
                {node.site.scope === 'definition' ? 'def' : node.site.scope === 'reference' ? 'ref' : 'key'}
              </span>
              <code class="rm-site-key">{node.site.key_name}</code>
              <span class="rm-site-path">@ {node.site.field_path.join('.')}</span>
              {#if node.site.preview}
                <span class="rm-site-preview" title={node.site.preview}>{node.site.preview}</span>
              {/if}
            {/if}
          {/snippet}
        </Tree>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <div class="rm-footer">
      <span class="rm-footer-hint">
        {#if hasDirtyBlockers}
          <AlertCircle size={12} /> Save or discard the listed docs to enable Apply.
        {:else if !newValueValid && trimmedNew.length > 0}
          New value must differ from the old value.
        {:else if activeSiteCount === 0 && totalSites > 0}
          All sites skipped — nothing to apply.
        {:else}
          <kbd>Ctrl</kbd>+<kbd>Enter</kbd> to apply · <kbd>Esc</kbd> to cancel
        {/if}
      </span>
      <div class="rm-footer-actions">
        <Button variant="ghost" onclick={onClose} disabled={applying}>
          Cancel
        </Button>
        <Button variant="primary" onclick={() => void runApply()} disabled={!canApply} loading={applying}>
          Rename {activeSiteCount > 0 ? `(${activeSiteCount})` : ''}
        </Button>
      </div>
    </div>
  {/snippet}
</Modal>

<style>
  /* Header / chrome — matches the RonStudioModal mac-close-btn pattern. */
  .rm-header {
    display: flex; align-items: center; justify-content: space-between;
    width: 100%;
    padding: 0;
  }
  .rm-title {
    display: flex; align-items: center; gap: 8px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .rm-format-tag {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--bg-elevated);
    color: var(--text-secondary);
    font-weight: 500;
  }
  .rm-close {
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
    border: none; background: transparent; cursor: pointer;
    border-radius: 4px;
    color: var(--text-secondary);
  }
  .rm-close:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* Body layout */
  .rm-body {
    display: flex; flex-direction: column;
    gap: 10px;
    height: 100%;
    padding: 14px 16px 8px;
    overflow: hidden;
  }
  .rm-form {
    display: flex; flex-direction: column; gap: 8px;
    flex-shrink: 0;
  }
  .rm-row {
    display: flex; align-items: center; gap: 10px;
  }
  .rm-row-label {
    width: 36px;
    font-size: 11px; text-transform: uppercase;
    color: var(--text-secondary);
    letter-spacing: 0.05em;
  }
  .rm-old {
    flex: 1;
    padding: 4px 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-primary);
    word-break: break-all;
  }

  .rm-summary {
    display: flex; align-items: center; gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
    padding: 4px 0 0;
  }
  .rm-summary code {
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-elevated);
    color: var(--text-primary);
  }
  .rm-summary strong { color: var(--text-primary); }
  .rm-summary-err   { color: var(--error); }
  .rm-skip-tag      { color: var(--warning); margin-left: 4px; }

  /* Banners */
  .rm-banner {
    display: flex; align-items: flex-start; gap: 8px;
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid;
    font-size: 12px;
    line-height: 1.45;
    flex-shrink: 0;
  }
  .rm-banner-body { flex: 1; min-width: 0; }
  .rm-banner-error {
    background: color-mix(in srgb, var(--error) 8%, transparent);
    border-color: color-mix(in srgb, var(--error) 30%, transparent);
    color: var(--text-primary);
  }
  .rm-banner-error :global(svg) { color: var(--error); flex-shrink: 0; margin-top: 2px; }
  .rm-banner-warn {
    background: color-mix(in srgb, var(--warning) 8%, transparent);
    border-color: color-mix(in srgb, var(--warning) 30%, transparent);
    color: var(--text-primary);
  }
  .rm-banner-warn :global(svg) { color: var(--warning); flex-shrink: 0; margin-top: 2px; }

  .rm-blocker-list, .rm-collision-list {
    margin: 4px 0 0;
    padding: 0 0 0 18px;
    color: var(--text-secondary);
    font-size: 11px;
  }
  .rm-blocker-list code, .rm-collision-list code {
    font-family: var(--font-mono);
    color: var(--text-primary);
  }
  .rm-collision-details summary {
    cursor: pointer;
    color: var(--text-secondary);
    font-size: 11px;
    margin-top: 4px;
  }
  .rm-collision-path {
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: 11px;
    margin-left: 4px;
  }

  /* Site list (Tree) */
  .rm-list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
  }
  .rm-list :global(.tree-row) {
    font-size: 12px;
    align-items: center;
    gap: 6px;
  }
  .rm-list :global(.tree-row:hover) { background: var(--bg-hover); }

  .rm-chev {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px;
    background: transparent; border: none; cursor: pointer;
    color: var(--text-secondary);
    padding: 0;
    flex-shrink: 0;
  }
  .rm-chev:hover { color: var(--text-primary); }
  .rm-chev-spacer { cursor: default; pointer-events: none; }

  .rm-check {
    margin: 0;
    flex-shrink: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .rm-file-icon { color: var(--text-secondary); flex-shrink: 0; }
  .rm-file-name {
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 1;
    min-width: 0;
  }
  .rm-file-count {
    margin-left: auto;
    color: var(--text-secondary);
    font-size: 11px;
    flex-shrink: 0;
  }

  .rm-scope {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 5px;
    border-radius: 3px;
    font-weight: 600;
    flex-shrink: 0;
  }
  .rm-scope-definition {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .rm-scope-reference {
    background: color-mix(in srgb, var(--info) 18%, transparent);
    color: var(--info);
  }
  .rm-scope-key {
    background: color-mix(in srgb, var(--warning) 18%, transparent);
    color: var(--warning);
  }

  .rm-site-key {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-primary);
    flex-shrink: 0;
  }
  .rm-site-path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .rm-site-preview {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-disabled);
    margin-left: 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }

  /* Footer */
  .rm-footer {
    display: flex; align-items: center; justify-content: space-between;
    width: 100%;
    gap: 12px;
  }
  .rm-footer-hint {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: 11px;
    color: var(--text-secondary);
    flex: 1; min-width: 0;
  }
  .rm-footer-hint kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 1px 4px;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    background: var(--bg-elevated);
    color: var(--text-secondary);
  }
  .rm-footer-actions {
    display: flex; align-items: center; gap: 8px;
    flex-shrink: 0;
  }
</style>
