<!--
  StudioBulkEditModal — F13 query-driven bulk edit (FROZEN F13).

  Format-agnostic preview/apply UI shared across every studio format
  whose backend declares `descriptor.supports_bulk_edit = true`. RON
  is the prototype consumer (Phase 2B-4); JSON/TOML/YAML/.properties
  inherit the modal as-is once their backends ship the capability.

  Wiring contract — the wrapper supplies:
    · `backend`     — `studioBackend(formatId)` (provides
                      `bulkEditPreview` / `bulkEditApply`)
    · `tabId`       — active repo tab id (BE resolves repo root from it)
    · `docId`       — active doc id (used in `active_doc` scope)
    · `formatLabel` — short display string ("RON" / "JSON" / …)
    · `query`       — JSONPath the user typed in the query bar
    · `nullPolicy`  — descriptor's `null_handling` (drives literal-null
                      preview warnings + UI affordances)
    · `openDocs`    — every open doc snapshot (dirty-blocker check)
    · `onClose`     — closed without applying
    · `onApplied`   — apply succeeded; carries the full BulkEditResult

  Behaviour:
    · Preview runs at mount + debounced on every action/value-source/
      scope change so the user sees the result of typing as they go.
    · Per-site skip checkboxes; per-file aggregate checkbox toggles
      every nested site at once. Sites marked `will_skip` by the BE
      cannot be re-enabled (the warning chip explains why).
    · Dirty-doc blocker — same as F12. Apply disabled until clean.
    · Expression compile errors land in a sticky banner.
    · Apply is best-effort sequential with rollback PRE-flush
      (project_wide) or a single history entry (active_doc).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    AlertCircle, AlertTriangle, ChevronDown, ChevronRight, FileText,
    PencilRuler, X as XIcon, Type as TypeIcon, Hash, ToggleLeft, Slash,
    PenLine, Trash2, FileCode, FolderOpen, Code2,
  } from 'lucide-svelte';

  import Modal from '$lib/components/shared/Modal.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Tree, { type RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';

  import type { NullPolicy, StudioBackend } from '$lib/ipc/studio-format';
  import type {
    BulkEditAction,
    BulkEditLiteral,
    BulkEditOpenDoc,
    BulkEditPreview,
    BulkEditResult,
    BulkEditScope,
    BulkEditSite,
    BulkEditValueSource,
  } from '$lib/types/studio-format';

  // ── Props ──────────────────────────────────────────────────────────

  interface Props {
    backend:     StudioBackend;
    tabId:       string;
    docId:       string;
    formatLabel: string;
    query:       string;
    nullPolicy:  NullPolicy;
    openDocs:    BulkEditOpenDoc[];
    onClose:     () => void;
    onApplied:   (result: BulkEditResult) => void;
  }

  let {
    backend, tabId, docId, formatLabel, query, nullPolicy, openDocs,
    onClose, onApplied,
  }: Props = $props();

  // ── Local state ────────────────────────────────────────────────────

  let action      = $state<BulkEditAction>('set');
  let scope       = $state<BulkEditScope>('active_doc');

  type LiteralKind = 'string' | 'number' | 'bool' | 'null';
  let valueMode   = $state<'literal' | 'expression'>('literal');
  let literalKind = $state<LiteralKind>('string');
  let literalStr  = $state('');
  let literalNum  = $state('');
  let literalBool = $state(false);
  let exprSrc     = $state('old');

  let preview      = $state<BulkEditPreview | null>(null);
  let previewing   = $state(true);
  let previewError = $state<string | null>(null);
  let applying     = $state(false);
  let applyError   = $state<string | null>(null);

  /** Site keys the user has chosen to skip (in addition to BE-flagged
   *  skips, which are always excluded regardless of this set). */
  let skipped = $state<Set<string>>(new Set());

  function siteKey(s: BulkEditSite): string {
    return `${s.absolute_path} ${s.field_path.join('/')}`;
  }

  // ── Value source builder ───────────────────────────────────────────

  /** Compose the wire-shape `BulkEditValueSource` from the local
   *  form state. `null` when `action === 'delete'` (BE doesn't need
   *  one and we keep the payload tight). */
  const valueSource = $derived.by<BulkEditValueSource | null>(() => {
    if (action === 'delete') return null;
    if (valueMode === 'expression') {
      return { kind: 'expression', source: exprSrc };
    }
    let literal: BulkEditLiteral;
    switch (literalKind) {
      case 'string': literal = { type: 'string', value: literalStr }; break;
      case 'bool':   literal = { type: 'bool',   value: literalBool }; break;
      case 'null':   literal = { type: 'null' };                       break;
      case 'number': {
        const n = literalStr === '' ? Number.NaN : Number(literalStr);
        // Surface NaN to the BE — the per-site preview will skip it
        // with a clear reason if applicable. (Numbers fully-typed by
        // the literalNum buffer would lose precision on f64 paths.)
        literal = { type: 'number', value: Number.isFinite(n) ? n : 0 };
        break;
      }
    }
    return { kind: 'literal', literal };
  });

  /** Surface the literal-number text validation inline so the user
   *  knows why the BE keeps reporting "0" hits on numeric mode. */
  const literalNumError = $derived.by<string | null>(() => {
    if (action !== 'set' || valueMode !== 'literal' || literalKind !== 'number') return null;
    if (literalStr === '') return 'Enter a number';
    const n = Number(literalStr);
    if (!Number.isFinite(n)) return 'Not a valid number';
    return null;
  });

  // ── Preview lifecycle ──────────────────────────────────────────────

  let previewTok = 0;
  let previewTimer: ReturnType<typeof setTimeout> | null = null;

  async function loadPreview(): Promise<void> {
    const token = ++previewTok;
    previewing = true;
    previewError = null;
    try {
      const p = await backend.bulkEditPreview({
        tabId,
        docId,
        scope,
        query,
        action,
        valueSource,
        openDocs,
      });
      if (token !== previewTok) return;
      preview = p;
      // Drop any skip keys whose sites no longer appear (scope or
      // query changed since last preview).
      const live = new Set(p.sites.map(siteKey));
      const next = new Set<string>();
      for (const k of skipped) if (live.has(k)) next.add(k);
      skipped = next;
    } catch (e) {
      if (token !== previewTok) return;
      previewError = String(e);
      preview = null;
    } finally {
      if (token === previewTok) previewing = false;
    }
  }

  function schedulePreview() {
    if (previewTimer) clearTimeout(previewTimer);
    previewTimer = setTimeout(() => void loadPreview(), 220);
  }

  onMount(() => { void loadPreview(); });

  // Re-fire preview on every input that affects what the BE computes.
  // Includes scope (different file set), action / value mode + kind /
  // value literal / expression. The debounce covers fast typing.
  let prevSnapshot = '';
  $effect(() => {
    const snap = JSON.stringify({
      action, scope, valueMode, literalKind, literalStr, literalBool,
      exprSrc,
    });
    if (snap === prevSnapshot) return;
    prevSnapshot = snap;
    schedulePreview();
  });

  // ── Derived UI state ───────────────────────────────────────────────

  type FileGroup = {
    id:            string;
    absolute_path: string;
    relative_path: string;
    file_name:     string;
    sites:         BulkEditSite[];
  };
  type Node =
    | { kind: 'file'; id: string; group: FileGroup; children: Node[] }
    | { kind: 'site'; id: string; site: BulkEditSite };

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

  const totalSites = $derived(preview?.sites.length ?? 0);
  const totalFiles = $derived(groups.length);

  /** A site is "active" when the BE didn't pre-skip it AND the user
   *  hasn't checked it off in the preview list. */
  const activeSites = $derived.by<BulkEditSite[]>(
    () => (preview?.sites ?? []).filter(s =>
      !s.will_skip && !skipped.has(siteKey(s))),
  );
  const activeSiteCount = $derived(activeSites.length);
  const activeFileCount = $derived.by<number>(
    () => new Set(activeSites.map(s => s.absolute_path)).size,
  );
  const beSkippedCount = $derived.by<number>(
    () => (preview?.sites ?? []).filter(s => s.will_skip).length,
  );
  const userSkippedCount = $derived(skipped.size);

  const hasDirtyBlockers   = $derived((preview?.dirty_blockers.length ?? 0) > 0);
  const hasExpressionError = $derived(!!preview?.expression_error);
  const isExpression       = $derived(valueMode === 'expression' && action === 'set');

  const valueInputValid = $derived.by<boolean>(() => {
    if (action === 'delete')              return true;
    if (valueMode === 'expression')       return exprSrc.trim().length > 0;
    if (literalKind === 'number')         return literalNumError === null;
    // string / bool / null are always "valid" (empty string is OK)
    return true;
  });

  const canApply = $derived(
    !applying
    && !previewing
    && !hasDirtyBlockers
    && !hasExpressionError
    && valueInputValid
    && activeSiteCount > 0,
  );

  // ── Skip toggles ───────────────────────────────────────────────────

  function toggleSite(s: BulkEditSite): void {
    if (s.will_skip) return;
    const key = siteKey(s);
    const next = new Set(skipped);
    if (next.has(key)) next.delete(key); else next.add(key);
    skipped = next;
  }

  type FileTriState = 'all' | 'none' | 'some' | 'forced_skip';
  function fileState(g: FileGroup): FileTriState {
    const enabledSites = g.sites.filter(s => !s.will_skip);
    if (enabledSites.length === 0) return 'forced_skip';
    let on = 0;
    for (const s of enabledSites) if (!skipped.has(siteKey(s))) on++;
    if (on === 0)                          return 'none';
    if (on === enabledSites.length)        return 'all';
    return 'some';
  }

  function toggleFile(g: FileGroup): void {
    const next = new Set(skipped);
    const state = fileState(g);
    const enabled = g.sites.filter(s => !s.will_skip);
    if (state === 'all') {
      for (const s of enabled) next.add(siteKey(s));
    } else if (state === 'forced_skip') {
      // Nothing to toggle.
    } else {
      for (const s of enabled) next.delete(siteKey(s));
    }
    skipped = next;
  }

  // ── Apply ──────────────────────────────────────────────────────────

  async function runApply(): Promise<void> {
    if (!canApply) return;
    applying = true;
    applyError = null;
    try {
      const result = await backend.bulkEditApply({
        tabId,
        docId,
        scope,
        action,
        valueSource,
        sites: activeSites,
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
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void runApply();
    }
  }

  const defaultExpandedIds = $derived.by<string[]>(() => groups.map(g => g.id));

  // ── Helpers ────────────────────────────────────────────────────────

  function scopeLabel(s: BulkEditScope): string {
    return s === 'active_doc' ? 'Active doc only' : 'Project-wide';
  }

  function literalKindIcon(k: LiteralKind) {
    return k === 'string' ? TypeIcon
         : k === 'number' ? Hash
         : k === 'bool'   ? ToggleLeft
         :                  Slash;
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Modal
  {onClose}
  width="min(880px, 95vw)"
  height="min(760px, 92vh)"
  padBody={false}
  ariaLabel="Bulk edit by query"
>
  {#snippet header()}
    <div class="be-header">
      <div class="be-title">
        <PencilRuler size={16} />
        <span>Bulk transform</span>
        <span class="be-format-tag">{formatLabel}</span>
      </div>
      <button class="be-close" type="button" aria-label="Close" onclick={onClose}>
        <XIcon size={14} />
      </button>
    </div>
  {/snippet}

  <div class="be-body">
    <!-- Query summary -->
    <div class="be-query-summary">
      <span class="be-query-label">Query</span>
      <code class="be-query-code">{query}</code>
      <span class="be-query-hits">
        {#if previewing && !preview}
          <Spinner size={11} /> previewing…
        {:else}
          {totalSites} match{totalSites === 1 ? '' : 'es'}
          {#if totalFiles > 1}· {totalFiles} files{/if}
        {/if}
      </span>
    </div>

    <!-- Form row 1 — Action -->
    <div class="be-row">
      <span class="be-row-label">Action</span>
      <div class="be-radio-group" role="radiogroup" aria-label="Action">
        <label class="be-radio" class:active={action === 'set'}>
          <input type="radio" name="bulk-action" value="set" bind:group={action} />
          <PenLine size={13} /> <span>Set</span>
        </label>
        <label class="be-radio" class:active={action === 'delete'}>
          <input type="radio" name="bulk-action" value="delete" bind:group={action} />
          <Trash2 size={13} /> <span>Delete</span>
        </label>
      </div>
    </div>

    <!-- Form row 2 — Value source (Set only) -->
    {#if action === 'set'}
      <div class="be-row">
        <span class="be-row-label">Value</span>
        <div class="be-value-block">
          <div class="be-tabs" role="tablist" aria-label="Value source">
            <button
              role="tab"
              type="button"
              class="be-tab"
              class:active={valueMode === 'literal'}
              aria-selected={valueMode === 'literal'}
              onclick={() => valueMode = 'literal'}
            >Literal</button>
            <button
              role="tab"
              type="button"
              class="be-tab"
              class:active={valueMode === 'expression'}
              aria-selected={valueMode === 'expression'}
              onclick={() => valueMode = 'expression'}
            ><Code2 size={11} /> Expression</button>
          </div>

          {#if valueMode === 'literal'}
            <div class="be-literal">
              <div class="be-kind-picker" role="radiogroup" aria-label="Literal kind">
                {#each ['string', 'number', 'bool', 'null'] as const as k}
                  {@const Icon = literalKindIcon(k)}
                  <button
                    type="button"
                    class="be-kind-chip"
                    class:active={literalKind === k}
                    onclick={() => literalKind = k}
                    aria-pressed={literalKind === k}
                  ><Icon size={11} /> {k}</button>
                {/each}
              </div>
              <div class="be-literal-input">
                {#if literalKind === 'string'}
                  <Input bind:value={literalStr} placeholder="String value" size="md" />
                {:else if literalKind === 'number'}
                  <Input
                    bind:value={literalStr}
                    placeholder="Number (e.g. 42, 3.14, -7)"
                    size="md"
                    error={literalNumError}
                  />
                {:else if literalKind === 'bool'}
                  <label class="be-bool-toggle">
                    <input type="checkbox" bind:checked={literalBool} />
                    <span>{literalBool ? 'true' : 'false'}</span>
                  </label>
                {:else if literalKind === 'null'}
                  <div class="be-null-hint">
                    {#if nullPolicy === 'not_supported'}
                      <AlertTriangle size={12} />
                      This format has no null. RON sites will skip with a warning unless the target is an Option.
                    {:else if nullPolicy === 'as_delete'}
                      <AlertTriangle size={12} />
                      Setting null on this format deletes the key.
                    {:else if nullPolicy === 'ask_user'}
                      Setting null prompts for: clear value or remove key.
                    {:else}
                      Native null — sites get `null` as their new value.
                    {/if}
                  </div>
                {/if}
              </div>
            </div>
          {:else}
            <div class="be-expr">
              <textarea
                class="be-expr-input"
                rows="3"
                placeholder={"old.upper()\n`${old}_legacy`\nold > 10 ? `big` : `small`\nold ?? \"default\""}
                bind:value={exprSrc}
                spellcheck="false"
                autocomplete="off"
              ></textarea>
              <details class="be-expr-help">
                <summary>Mini-expression language — fluent on `old`</summary>
                <div class="be-expr-help-body">
                  <p><strong>Variable.</strong> <code>old</code> is the current value at each hit.</p>
                  <p><strong>Method chains.</strong></p>
                  <ul>
                    <li><code>old.upper()</code>, <code>old.lower()</code>, <code>old.trim()</code></li>
                    <li><code>old.replace("from", "to")</code> · <code>old.substr(0, 5)</code></li>
                    <li><code>old.pad_start(4, "0")</code> · <code>old.length()</code></li>
                    <li><code>old.starts_with("v")</code> · <code>old.contains("x")</code></li>
                    <li><code>old.abs()</code>, <code>old.round(2)</code>, <code>old.clamp(0, 100)</code></li>
                    <li><code>old.to_string()</code> · <code>old.to_number()</code> · <code>old.to_bool()</code></li>
                  </ul>
                  <p><strong>Template strings.</strong> <code>`prefix_${'$'}{'{'}old{'}'}_v2`</code></p>
                  <p><strong>Operators.</strong> <code>+ - * / %</code> (strict types) · <code>== != &lt; &gt; &lt;= &gt;=</code> · <code>&amp;&amp; ||  !</code> · <code>??</code> (null coalesce) · <code>? :</code> (ternary)</p>
                  <p><strong>Null-safety.</strong> Methods on null skip the site. Guard with <code>(old ?? "").upper()</code>.</p>
                </div>
              </details>
            </div>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Form row 3 — Scope -->
    <div class="be-row">
      <span class="be-row-label">Scope</span>
      <div class="be-radio-group" role="radiogroup" aria-label="Scope">
        <label class="be-radio" class:active={scope === 'active_doc'}>
          <input type="radio" name="bulk-scope" value="active_doc" bind:group={scope} />
          <FileCode size={13} /> <span>{scopeLabel('active_doc')}</span>
        </label>
        <label class="be-radio" class:active={scope === 'project_wide'}>
          <input type="radio" name="bulk-scope" value="project_wide" bind:group={scope} />
          <FolderOpen size={13} /> <span>{scopeLabel('project_wide')}</span>
        </label>
      </div>
    </div>

    <!-- Banners -->
    {#if hasExpressionError}
      <div class="be-banner be-banner-error">
        <AlertCircle size={14} />
        <div class="be-banner-body">
          <strong>Expression error.</strong>
          <code class="be-banner-code">{preview?.expression_error}</code>
        </div>
      </div>
    {/if}

    {#if hasDirtyBlockers}
      <div class="be-banner be-banner-error">
        <AlertCircle size={14} />
        <div class="be-banner-body">
          <strong>Unsaved changes block this edit.</strong>
          Save or discard the following open
          {(preview?.dirty_blockers.length ?? 0) === 1 ? 'doc' : 'docs'}, then re-open this dialog:
          <ul class="be-blocker-list">
            {#each preview?.dirty_blockers ?? [] as d (d.doc_id)}
              <li><code>{d.source_path ?? '(untitled doc)'}</code></li>
            {/each}
          </ul>
        </div>
      </div>
    {/if}

    {#if applyError}
      <div class="be-banner be-banner-error">
        <AlertCircle size={14} />
        <div class="be-banner-body">
          <strong>Apply failed.</strong>
          {applyError}
        </div>
      </div>
    {/if}

    <!-- Site preview -->
    <div class="be-list-wrap">
      <div class="be-list-header">
        <span>
          <strong>{activeSiteCount}</strong>
          {activeSiteCount === 1 ? 'site' : 'sites'} ready
          {#if activeFileCount > 1}across <strong>{activeFileCount}</strong> files{/if}
          {#if beSkippedCount > 0}
            <span class="be-skip-tag" title="Skipped by the engine (eval error / type mismatch / container hit)">
              · {beSkippedCount} skipped
            </span>
          {/if}
          {#if userSkippedCount > 0}
            <span class="be-skip-tag-user" title="Skipped by you via the checkbox">
              · {userSkippedCount} excluded
            </span>
          {/if}
        </span>
      </div>
      <div class="be-list">
        {#if previewing && !preview}
          <StateBlock tone="loading" label="Scanning…">
            {#snippet spinner()}<Spinner size={14} />{/snippet}
          </StateBlock>
        {:else if previewError}
          <StateBlock tone="error" label={previewError} />
        {:else if totalSites === 0}
          <StateBlock tone="neutral" label="Query returned no matches." />
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
            ariaLabel="Bulk edit preview"
          >
            {#snippet row({ node, expanded, toggle }: RowSnippetCtx<Node>)}
              {#if node.kind === 'file'}
                {@const fst = fileState(node.group)}
                <button
                  type="button"
                  class="be-chev"
                  aria-label={expanded ? 'Collapse file' : 'Expand file'}
                  onclick={(e) => { e.stopPropagation(); toggle(); }}
                >
                  {#if expanded}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
                </button>
                <input
                  type="checkbox"
                  class="be-check"
                  aria-label="Toggle all sites in this file"
                  checked={fst === 'all'}
                  indeterminate={fst === 'some'}
                  disabled={fst === 'forced_skip'}
                  onclick={(e) => e.stopPropagation()}
                  onchange={() => toggleFile(node.group)}
                />
                <FileText size={13} class="be-file-icon" />
                <span class="be-file-name">{node.group.relative_path}</span>
                <span class="be-file-count">
                  {node.group.sites.length}
                  {node.group.sites.length === 1 ? 'site' : 'sites'}
                </span>
              {:else}
                {@const s = node.site}
                {@const skip = skipped.has(siteKey(s))}
                <span class="be-chev be-chev-spacer"></span>
                <input
                  type="checkbox"
                  class="be-check"
                  aria-label={skip ? 'Include this site' : 'Skip this site'}
                  checked={!skip && !s.will_skip}
                  disabled={s.will_skip}
                  onclick={(e) => e.stopPropagation()}
                  onchange={() => toggleSite(s)}
                />
                <span class="be-kind-chip-sm">{s.kind}</span>
                <span class="be-site-path">@ {s.field_path.length === 0 ? '$' : s.field_path.join('.')}</span>
                <span class="be-old">{s.old_preview}</span>
                <span class="be-arrow">→</span>
                {#if s.will_skip}
                  <span class="be-skip-chip" title={s.skip_reason}>
                    <AlertTriangle size={10} /> skip
                  </span>
                  {#if s.skip_reason}
                    <span class="be-skip-reason">{s.skip_reason}</span>
                  {/if}
                {:else}
                  <span class="be-new">{s.new_preview}</span>
                {/if}
              {/if}
            {/snippet}
          </Tree>
        {/if}
      </div>
    </div>
  </div>

  {#snippet footer()}
    <div class="be-footer">
      <span class="be-footer-hint">
        {#if hasExpressionError}
          <AlertCircle size={12} /> Fix the expression to enable Apply.
        {:else if hasDirtyBlockers}
          <AlertCircle size={12} /> Save or discard the listed docs to enable Apply.
        {:else if !valueInputValid}
          Fix the value above to enable Apply.
        {:else if activeSiteCount === 0 && totalSites > 0}
          All matching sites are skipped — nothing to apply.
        {:else}
          <kbd>Ctrl</kbd>+<kbd>Enter</kbd> to apply · <kbd>Esc</kbd> to cancel
        {/if}
      </span>
      <div class="be-footer-actions">
        <Button variant="ghost" onclick={onClose} disabled={applying}>Cancel</Button>
        <Button
          variant="primary"
          onclick={() => void runApply()}
          disabled={!canApply}
          loading={applying}
        >
          Apply {activeSiteCount > 0 ? `(${activeSiteCount})` : ''}
        </Button>
      </div>
    </div>
  {/snippet}
</Modal>

<style>
  /* Header / chrome — same pattern as StudioRenameModal. */
  .be-header {
    display: flex; align-items: center; justify-content: space-between;
    width: 100%; padding: 0;
  }
  .be-title {
    display: flex; align-items: center; gap: 8px;
    font-weight: 600; color: var(--text-primary);
  }
  .be-format-tag {
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em;
    padding: 2px 6px; border-radius: 4px;
    background: var(--bg-elevated); color: var(--text-secondary);
    font-weight: 500;
  }
  .be-close {
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
    border: none; background: transparent; cursor: pointer;
    border-radius: 4px; color: var(--text-secondary);
  }
  .be-close:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* Body layout */
  .be-body {
    display: flex; flex-direction: column;
    gap: 10px; height: 100%; padding: 12px 16px 8px;
    overflow: hidden;
  }

  .be-query-summary {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    flex-shrink: 0;
    min-width: 0;
  }
  .be-query-label {
    font-size: 10px; text-transform: uppercase;
    color: var(--text-secondary);
    letter-spacing: 0.06em;
    flex-shrink: 0;
  }
  .be-query-code {
    flex: 1;
    font-family: var(--font-mono); font-size: 12px;
    color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    min-width: 0;
  }
  .be-query-hits {
    font-size: 11px;
    color: var(--text-secondary);
    flex-shrink: 0;
    display: inline-flex; align-items: center; gap: 4px;
  }

  /* Form rows */
  .be-row {
    display: flex; align-items: flex-start; gap: 12px;
    flex-shrink: 0;
  }
  .be-row-label {
    width: 60px;
    padding-top: 5px;
    font-size: 10px; text-transform: uppercase;
    color: var(--text-secondary); letter-spacing: 0.06em;
    flex-shrink: 0;
  }
  .be-radio-group {
    display: inline-flex; gap: 4px;
    padding: 3px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
  }
  .be-radio {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 4px 10px;
    font-size: 12px;
    color: var(--text-secondary);
    border-radius: 4px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .be-radio input { display: none; }
  .be-radio:hover { color: var(--text-primary); }
  .be-radio.active {
    background: var(--bg-base);
    color: var(--text-primary);
    box-shadow: 0 0 0 1px var(--accent) inset;
  }

  /* Value block */
  .be-value-block {
    flex: 1;
    display: flex; flex-direction: column; gap: 6px;
    min-width: 0;
  }
  .be-tabs {
    display: inline-flex; gap: 2px;
    align-self: flex-start;
  }
  .be-tab {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 4px 9px;
    font-size: 11px;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .be-tab:hover { color: var(--text-primary); }
  .be-tab.active {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }

  .be-literal {
    display: flex; flex-direction: column; gap: 6px;
  }
  .be-kind-picker {
    display: inline-flex; gap: 4px;
  }
  .be-kind-chip {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 3px 8px;
    font-size: 11px;
    background: var(--bg-base);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .be-kind-chip:hover { color: var(--text-primary); }
  .be-kind-chip.active {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .be-literal-input { min-width: 0; }
  .be-bool-toggle {
    display: inline-flex; align-items: center; gap: 6px;
    font-family: var(--font-mono); font-size: 12px;
    color: var(--text-primary);
  }
  .be-null-hint {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: 11.5px;
    color: var(--text-secondary);
    background: color-mix(in srgb, var(--warning) 8%, transparent);
    padding: 4px 8px;
    border-radius: 4px;
    border: 1px solid color-mix(in srgb, var(--warning) 25%, transparent);
  }
  .be-null-hint :global(svg) { color: var(--warning); flex-shrink: 0; }

  .be-expr {
    display: flex; flex-direction: column; gap: 4px;
  }
  .be-expr-input {
    font-family: var(--font-mono); font-size: 12.5px;
    color: var(--text-primary);
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    padding: 6px 8px;
    resize: vertical;
    min-height: 50px;
    line-height: 1.45;
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .be-expr-input:focus { border-color: var(--accent); }
  .be-expr-help summary {
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
    margin-top: 2px;
  }
  .be-expr-help-body {
    font-size: 11.5px;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    padding: 8px 12px;
    margin-top: 6px;
    line-height: 1.55;
  }
  .be-expr-help-body p { margin: 4px 0; }
  .be-expr-help-body ul { margin: 4px 0 4px 16px; padding: 0; }
  .be-expr-help-body li { margin: 2px 0; }
  .be-expr-help-body code {
    font-family: var(--font-mono); font-size: 11px;
    padding: 1px 4px; border-radius: 3px;
    background: var(--bg-base); color: var(--text-primary);
  }

  /* Banners */
  .be-banner {
    display: flex; align-items: flex-start; gap: 8px;
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid;
    font-size: 12px; line-height: 1.45;
    flex-shrink: 0;
  }
  .be-banner-body { flex: 1; min-width: 0; }
  .be-banner-error {
    background: color-mix(in srgb, var(--error) 8%, transparent);
    border-color: color-mix(in srgb, var(--error) 30%, transparent);
    color: var(--text-primary);
  }
  .be-banner-error :global(svg) { color: var(--error); flex-shrink: 0; margin-top: 2px; }
  .be-banner-code {
    display: inline-block;
    font-family: var(--font-mono); font-size: 11px;
    padding: 1px 6px; border-radius: 3px;
    background: var(--bg-base); color: var(--text-primary);
    margin-left: 4px;
  }
  .be-blocker-list {
    margin: 4px 0 0;
    padding: 0 0 0 18px;
    color: var(--text-secondary);
    font-size: 11px;
  }
  .be-blocker-list code { font-family: var(--font-mono); color: var(--text-primary); }

  /* Site list (Tree) */
  .be-list-wrap {
    flex: 1;
    display: flex; flex-direction: column;
    min-height: 0;
  }
  .be-list-header {
    font-size: 11.5px;
    color: var(--text-secondary);
    padding: 4px 4px 6px;
    display: flex; align-items: center; justify-content: space-between;
  }
  .be-list-header strong { color: var(--text-primary); }
  .be-skip-tag      { color: var(--warning); margin-left: 4px; }
  .be-skip-tag-user { color: var(--text-secondary); margin-left: 4px; }

  .be-list {
    flex: 1; min-height: 0;
    overflow: auto;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
  }
  .be-list :global(.tree-row) {
    font-size: 12px;
    align-items: center;
    gap: 6px;
  }
  .be-list :global(.tree-row:hover) { background: var(--bg-hover); }

  .be-chev {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px;
    background: transparent; border: none; cursor: pointer;
    color: var(--text-secondary);
    padding: 0;
    flex-shrink: 0;
  }
  .be-chev:hover { color: var(--text-primary); }
  .be-chev-spacer { cursor: default; pointer-events: none; }

  .be-check {
    margin: 0; flex-shrink: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .be-check:disabled { cursor: not-allowed; opacity: 0.4; }

  .be-file-name {
    color: var(--text-primary);
    font-family: var(--font-mono); font-size: 12px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    flex-shrink: 1; min-width: 0;
  }
  .be-file-count {
    margin-left: auto;
    color: var(--text-secondary); font-size: 11px;
    flex-shrink: 0;
  }

  .be-kind-chip-sm {
    font-size: 9.5px; text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 5px; border-radius: 3px;
    background: var(--bg-elevated);
    color: var(--text-secondary);
    font-family: var(--font-mono);
    flex-shrink: 0;
  }
  .be-site-path {
    font-family: var(--font-mono); font-size: 11px;
    color: var(--syntax-type, var(--accent));
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    flex-shrink: 1; min-width: 0;
  }
  .be-old, .be-new {
    font-family: var(--font-mono); font-size: 11px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    flex-shrink: 1; min-width: 0;
    max-width: 280px;
  }
  .be-old { color: var(--text-secondary); }
  .be-new { color: var(--success, var(--accent)); }
  .be-arrow {
    color: var(--text-disabled);
    flex-shrink: 0;
  }

  .be-skip-chip {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 0 6px;
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em;
    border-radius: 3px;
    background: color-mix(in srgb, var(--warning) 18%, transparent);
    color: var(--warning);
    flex-shrink: 0;
  }
  .be-skip-reason {
    font-size: 11px;
    color: var(--text-secondary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    flex-shrink: 1; min-width: 0;
  }

  /* Footer */
  .be-footer {
    display: flex; align-items: center; justify-content: space-between;
    gap: 12px;
    padding: 8px 4px 4px;
    width: 100%;
  }
  .be-footer-hint {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: 11.5px; color: var(--text-secondary);
  }
  .be-footer-hint kbd {
    font-family: var(--font-mono); font-size: 10px;
    padding: 1px 5px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    color: var(--text-secondary);
  }
  .be-footer-actions { display: flex; gap: 6px; }
</style>
