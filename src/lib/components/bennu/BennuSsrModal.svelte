<script lang="ts">
  /**
   * Structural search & replace (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd>).
   *
   * ## What the layout is saying
   *
   * **Templates on the left, the query on top, the answer under it.** A query is two or three lines of code with
   * holes in it — never a document — so it gets a band, not a column; what it can be asked is learned from the
   * templates beside it, which are one click from filling the field.
   *
   * The language is offered only where it means something: JSP and "Java in JSP" appear when the project has
   * pages, and a Java-only project has no selector at all. The scope is the language's own `in` clause, said for
   * you from a picker.
   *
   * ## The field talks back while you type
   *
   * `bennu_ssr_explain` touches no files, so every keystroke gets an answer: a syntax error is underlined in the
   * query with its message, the holes it binds are chips that insert themselves, and a replacement naming a hole
   * the query does not bind is underlined too. An empty result list is the worst way to be told of a mistake.
   *
   * ## Replace is two steps, always
   *
   * Preview, then apply what the preview showed — the files that are ticked. The whole reason to prefer a
   * structural replace over a textual one is that it is precise, and precision is only worth something if you can
   * check it.
   */
  import {
    Search, Play, X, Plus, Square, CircleAlert, Timer, TriangleAlert, Eye,
  } from 'lucide-svelte';
  import { untrack } from 'svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import SplitButton from '$lib/components/shared/ui/SplitButton.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import ResizablePanel from '$lib/components/shared/ui/ResizablePanel.svelte';
  import ExportButton, { type Rendition } from '$lib/components/shared/internal/ExportButton.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { EXPORT_EXTENSION, type ExportFormat } from '$lib/utils/tabular-export';
  import { tooltip } from '$lib/actions/tooltip';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuSsrStore } from '$lib/stores/bennu/ssr.svelte';
  import type { SsrDialect } from '$lib/ipc/bennu/ssr';
  import {
    capturesIn, setQueryDialect, setReplacementCaptures, setSavedTemplates, ssrQueryLanguage,
    ssrReplacementLanguage,
  } from './ssr-lang';
  import SsrSidebar from './ssr/SsrSidebar.svelte';
  import SsrResults from './ssr/SsrResults.svelte';
  import SsrReplacePreview from './ssr/SsrReplacePreview.svelte';
  import {
    formatElapsed, nextHoleName, parentOf, plural, queryDiagnostics, relativeTo, replacementDiagnostics, summary,
    WHOLE_PROJECT, type SsrScope,
  } from './ssr/ssr-model';
  import { exportResults, exportSubject } from './ssr/ssr-export';

  let { onClose }: { onClose: () => void } = $props();

  const store = bennuSsrStore;
  const root = $derived(projectStore.project?.root ?? '');
  const explained = $derived(store.explained);
  const report = $derived(store.report);

  $effect(() => { void store.attach(); void store.loadConfig(); });
  $effect(() => { if (root) void store.loadLanguages(root); });

  // The editors take their language once, at mount, and its completion sources are plain functions — so what they
  // offer (the query's holes, the grammar's vocabulary, the saved templates) is pushed to the module as it changes.
  // One structural-search modal is open at a time, which is what makes a module-level value the right shape.
  $effect(() => { setReplacementCaptures(capturesIn(store.query)); });
  $effect(() => { setQueryDialect(store.dialect); });
  $effect(() => {
    setSavedTemplates(store.saved.map((s) => ({ title: s.title, query: s.query, dialect: s.dialect as SsrDialect })));
  });

  /** Open a hit. Closes the modal — you asked to go somewhere. */
  function open(file: string, line: number) {
    void projectStore.openFile(file).then(() => bennuUiStore.requestGoto(line));
    onClose();
  }

  // ── language and scope ─────────────────────────────────────────────────────────
  const languageTabs: TabItem[] = [
    { id: 'java', label: 'Java' },
    { id: 'jsp', label: 'JSP' },
    { id: 'jsp-java', label: 'Java in JSP' },
  ];
  /** Offered only when the project really has pages — a Java-only project has no selector. */
  const showLanguages = $derived(store.hasPages === true);

  const activeFile = $derived(projectStore.activeFilePath ? relativeTo(root, projectStore.activeFilePath) : '');
  const scopeOptions = $derived.by(() => {
    const options: { value: string; label: string }[] = [{ value: 'project', label: 'Whole project' }];
    for (const m of projectStore.workspaceModules) {
      if (m.dir !== '.') options.push({ value: `module:${m.dir}`, label: `Module ${m.dir}` });
    }
    if (activeFile) {
      const dir = parentOf(activeFile);
      if (dir) options.push({ value: `dir:${dir}`, label: `Directory ${dir}` });
      options.push({ value: `file:${activeFile}`, label: `Current file` });
    }
    return options;
  });
  const scopeValue = $derived(
    store.scope.kind === 'project' ? 'project' : `${store.scope.kind === 'module' ? 'module' : store.scope.kind === 'directory' ? 'dir' : 'file'}:${store.scope.path}`,
  );
  function pickScope(value: string) {
    const [kind, ...rest] = value.split(':');
    const path = rest.join(':');
    const scope: SsrScope =
      kind === 'module' ? { kind: 'module', path }
        : kind === 'dir' ? { kind: 'directory', path }
          : kind === 'file' ? { kind: 'file', path }
            : WHOLE_PROJECT;
    store.setScope(scope);
  }

  // ── the fields ─────────────────────────────────────────────────────────────────
  let queryEd = $state<{ focus: () => void; insertAtCursor: (t: string) => void } | null>(null);
  let replaceEd = $state<{ focus: () => void; insertAtCursor: (t: string) => void } | null>(null);
  let replaceFocused = $state(false);

  const queryDiags = $derived(queryDiagnostics(store.query, explained));
  const replaceDiags = $derived(replacementDiagnostics(store.replacement, explained?.captures ?? []));

  /** The placeholder is a working query in the current language — the field's shortest possible lesson. */
  const queryPlaceholder = $derived(
    store.dialect === 'jsp'
      ? '<s:property $pre...$ value="$x$" $post...$/>\ngroup $x$'
      : store.dialect === 'jsp-java'
        ? 'session.getAttribute($key$)\ngroup $key$'
        : '$o: com.acme.OrderService$.$m$($args...$)\ngroup $m$',
  );

  /** A chip puts its hole where the caret is — in the replacement when that is where you were typing. */
  function insertHole(name: string) {
    const target = store.replacing && replaceFocused ? replaceEd : queryEd;
    target?.insertAtCursor(`$${name}$`);
    target?.focus();
  }
  const newHole = () => insertHole(nextHoleName(explained?.captures ?? capturesIn(store.query)));

  // ── states ─────────────────────────────────────────────────────────────────────
  const nothingFound = $derived(
    store.scanned === 0
      ? store.dialect === 'java'
        ? 'No .java files were found to search. Check the scope above.'
        : 'No .jsp, .jspf, .jspx or .tag files were found to search. Check the scope above.'
      : `Nothing matches that — ${plural(store.scanned, 'file')} read.`,
  );
  const elapsed = $derived(store.elapsedMs ? formatElapsed(store.elapsedMs) : null);
  const scanNote = $derived.by(() => {
    if (store.searching || !report) return null;
    return store.prefiltered
      ? `${store.parsed} of ${store.scanned} files parsed`
      : `${store.parsed} files parsed — nothing constant to grep for, so every file was read`;
  });
  const total = $derived(report ? report.total : store.hits.length);
  const fileCount = $derived(report ? report.files : new Set(store.hits.map((h) => h.file)).size);
  const hasResults = $derived(!!report?.groupedBy || store.hits.length > 0);

  // ── running ────────────────────────────────────────────────────────────────────
  let results = $state<{
    move: (d: number) => void; openCurrent: () => void; hasHits: () => boolean;
    togglePeek: () => void; peekOpen: () => boolean; dismissPeek: () => void;
  } | null>(null);

  const run = () => { if (store.valid && !store.searching) void store.search(root); };
  const previewChanges = () => { if (store.valid && !store.previewing) void store.buildPreview(root); };

  const renditions = $derived<Rendition[]>(
    (['csv', 'json', 'markdown'] as ExportFormat[]).map((format) => ({
      id: format,
      label: format === 'csv' ? 'As CSV' : format === 'json' ? 'As JSON' : 'As a Markdown table',
      extension: EXPORT_EXTENSION[format],
      text: () => exportResults(report, store.hits, format),
    })),
  );

  function onKeydown(e: KeyboardEvent) {
    const inEditor = e.target instanceof HTMLElement && !!e.target.closest('.cm-editor');
    // Ctrl+Enter runs, the way it submits every other modal in the app; with Shift it previews the replacement.
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      if (e.shiftKey && store.replacing) previewChanges(); else run();
      return;
    }
    // F3 / Shift+F3 walk the matches from anywhere, the editor included.
    if (e.key === 'F3') { e.preventDefault(); results?.move(e.shiftKey ? -1 : 1); return; }
    // F2 (and Space, below) shows what the selected match looks like in context — or, with Replace on, what it
    // becomes. Esc puts the preview away before it closes anything else.
    if (e.key === 'F2' && results?.hasHits()) { e.preventDefault(); results.togglePeek(); return; }
    if (e.key === 'Escape' && results?.peekOpen()) { e.preventDefault(); e.stopPropagation(); results.dismissPeek(); return; }
    if (e.key === 'Escape' && store.searching) { e.preventDefault(); e.stopPropagation(); store.cancel(); return; }
    // The arrows and Enter walk the results — but only from outside the editors, which own them for the caret.
    if (inEditor || (e.target instanceof HTMLInputElement)) return;
    if (e.key === 'ArrowDown') { e.preventDefault(); results?.move(1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); results?.move(-1); }
    else if (e.key === ' ' && results?.hasHits() && !(e.target instanceof HTMLButtonElement && e.target.closest('.ss-actions'))) {
      e.preventDefault();
      results.togglePeek();
    } else if (e.key === 'Enter' && results?.hasHits()) { e.preventDefault(); results.openCurrent(); }
  }

  // The first focus goes to the query: opening this modal means "I am about to write one".
  $effect(() => { untrack(() => queryEd?.focus()); });
</script>

<Modal {onClose} width="min(1280px, 94vw)" height="min(780px, 92vh)" padBody={false} ariaLabel="Structural search">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Search size={14} />
      <span class="modal-title">Structural search</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="ss" role="group" onkeydown={onKeydown}>
    <ResizablePanel initialSize={260} minSize={170} maxSize={460}>
      <SsrSidebar />
    </ResizablePanel>

    <div class="ss-main">
      <div class="ss-query">
        <div class="ss-bar">
          <!-- The language is asked, never inferred: a pattern compiled under the wrong grammar does not fail
               loudly — it matches nothing, which reads as "the project contains none of this". Offered only when
               the project has pages. -->
          {#if showLanguages}
            <Tabs items={languageTabs} value={store.dialect} variant="segmented" size="xs"
                  ariaLabel="Language of the query" onSelect={(id) => store.setDialect(id as SsrDialect)} />
          {/if}
          <Select value={scopeValue} options={scopeOptions} onchange={pickScope} ariaLabel="Where to search" />
          <span class="ss-spacer"></span>
          <label class="ss-replace-head">
            <span>Replace</span>
            <Toggle checked={store.replacing} onchange={(v) => store.setReplacing(v)} ariaLabel="Show the replacement" />
          </label>
        </div>

        <!-- A real editor: a query is code with holes in it. It colours Java, tells a hole from the code around
             it, completes the clause words, the holes already used, types, node kinds and templates, and
             underlines the parser's error where it is. No gutter: three lines, nobody refers to one by number. -->
        <div class="ss-field" class:ss-bad={!!explained?.error}>
          <CodeEditor
            bind:this={queryEd}
            language={ssrQueryLanguage}
            value={store.query}
            oninput={(text) => store.setQuery(text)}
            placeholder={queryPlaceholder}
            diagnostics={queryDiags}
            lineNumbers={false}
            wrap
          />
        </div>

        {#if explained?.error}
          <p class="ss-err">
            <CircleAlert size={12} />
            <span>{explained.error}{explained.errorLine ? ` (line ${explained.errorLine})` : ''}</span>
          </p>
        {:else}
          <div class="ss-meta">
            {#each explained?.captures ?? [] as name (name)}
              <button class="ss-chip" type="button" use:tooltip={`Insert $${name}$ at the caret`} onclick={() => insertHole(name)}>
                ${name}$
              </button>
            {/each}
            <button class="ss-chip ss-chip-new" type="button" use:tooltip={'Insert a new hole at the caret'} onclick={newHole}>
              <Plus size={10} /> hole
            </button>
            {#if explained && explained.alternatives > 1}
              <Badge variant="tone" tone="info" size="sm" label={`${explained.alternatives} alternatives`} />
            {/if}
            {#if explained && !explained.literals.length}
              <!-- Said, not implied: the difference between a query that reads a handful of files and one that
                   parses the whole project. -->
              <span class="ss-slow" use:tooltip={'Nothing constant to grep for, so every file is parsed'}>
                <TriangleAlert size={11} /> whole-project scan
              </span>
            {/if}
          </div>
        {/if}

        {#if explained?.expansion.length}
          <details class="ss-expand">
            <summary>What <code>use of</code> looks for</summary>
            {#each explained.expansion as line (line)}<code class="ss-expand-line">{line}</code>{/each}
          </details>
        {/if}

        {#if store.replacing}
          <div class="ss-field ss-field-small" class:ss-bad={replaceDiags.length > 0}
               onfocusin={() => (replaceFocused = true)} onfocusout={() => (replaceFocused = false)}>
            <CodeEditor
              bind:this={replaceEd}
              language={ssrReplacementLanguage}
              value={store.replacement}
              oninput={(text) => store.setReplacement(text)}
              placeholder={'Optional.ofNullable($a$).map(X::$m$)'}
              diagnostics={replaceDiags}
              lineNumbers={false}
              wrap
            />
          </div>
          {#if replaceDiags.length}
            <p class="ss-err"><CircleAlert size={12} /><span>{replaceDiags[0].message}</span></p>
          {/if}
        {/if}
      </div>

      <div class="ss-results">
        {#if store.error}
          <div class="ss-pad"><Alert variant="error" compact title="That did not run" text={store.error} /></div>
        {:else if store.previewing}
          <div class="ss-mid"><Spinner size={16} /><span>Working out what would change…</span></div>
        {:else if store.preview}
          <SsrReplacePreview />
        {:else if store.applyResult}
          <div class="ss-pad">
            <Alert
              variant={store.applyResult.refused.length ? 'warning' : 'success'}
              title={`${plural(store.applyResult.written, 'file')} rewritten`}
              text={store.applyResult.refused.length
                ? `${store.applyResult.refused.length} refused — they changed after the preview was built, so the plan no longer described them.`
                : 'Every file you selected was written.'}
            />
            {#each store.applyResult.refused as r (r.file)}
              <p class="ss-refused"><X size={11} /> {r.file} — {r.reason}</p>
            {/each}
          </div>
        {:else if store.searching && !hasResults}
          <div class="ss-mid">
            <Spinner size={16} />
            <span>Searching… {elapsed ?? ''}</span>
            <Button variant="secondary" size="xs" onclick={() => store.cancel()}>
              {#snippet iconStart()}<Square size={11} />{/snippet}
              Stop <Kbd keys={['Esc']} size="sm" />
            </Button>
          </div>
        {:else if hasResults}
          <div class="ss-head">
            <strong>{summary(total, fileCount)}</strong>
            {#if report?.groupedBy}<span>by <code>{report.groupedBy}</code></span>{/if}
            {#if report?.unresolved}
              <span class="ss-unres" use:tooltip={'The classpath could not decide a type constraint for these'}>
                <TriangleAlert size={11} /> {report.unresolved} undecided
              </span>
            {/if}
            {#if store.capped}<Badge variant="tone" tone="warning" size="sm" label="capped" />{/if}
            {#if store.cancelled}<Badge variant="tone" tone="warning" size="sm" label="stopped — partial" />{/if}
            {#if store.searching}
              <Spinner size={11} />
              <Button variant="ghost" size="xs" onclick={() => store.cancel()}>Stop</Button>
            {/if}
          </div>
          <SsrResults bind:this={results} {root} onOpen={open} />
        {:else if store.cancelled}
          <EmptyState message="Stopped before anything was found." />
        {:else if report}
          <EmptyState message={nothingFound} />
        {:else}
          <EmptyState message="Write a query and press Ctrl+Enter — or start from a template on the left." />
        {/if}

        {#if scanNote || elapsed}
          <p class="ss-note">
            {#if elapsed}
              <!-- The one panel whose cost is variable AND explainable: a query with a literal to grep for reads a
                   tenth of the project, one made only of holes reads all of it. -->
              <span class="ss-elapsed" use:tooltip={'How long the search took. A query with a constant in it is narrowed by a text search first; one made only of holes parses every file.'}>
                <Timer size={11} /> {elapsed}
              </span>
            {/if}
            {#if scanNote}<span>{scanNote}</span>{/if}
          </p>
        {/if}
      </div>
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="ss-hint">
        <Kbd keys={['Ctrl', 'Enter']} size="sm" /> run
        <Kbd keys={['F3']} size="sm" /> next
        <Kbd keys={['Shift', 'F3']} size="sm" /> previous
        <Kbd keys={['Enter']} size="sm" /> open
      </span>
      <div class="ss-actions">
        <ExportButton
          {renditions}
          fileName="structural-search"
          subject={exportSubject(report, store.hits.length)}
          empty={!hasResults}
          emptyTooltip="Run a query first — there is nothing to take out yet"
          tooltip="Take these results out of Arbor — to the clipboard, or to a file"
        />
        <Button variant="ghost" onclick={onClose}>Close</Button>
        {#if store.searching}
          <Button variant="secondary" onclick={() => store.cancel()}>
            {#snippet iconStart()}<Square size={12} />{/snippet}
            Stop
          </Button>
        {:else}
          <Button variant={store.replacing ? 'secondary' : 'primary'} disabled={!store.valid} onclick={run}>
            {#snippet iconStart()}<Play size={13} />{/snippet}
            Search
          </Button>
        {/if}
        {#if store.replacing}
          {#if store.preview}
            <SplitButton
              variant="primary"
              direction="up"
              label={`Replace ${store.applyFiles.size === store.preview.files.length ? 'all' : 'selected'} (${plural(store.applyFiles.size, 'file')})`}
              disabled={!store.applyFiles.size}
              options={[{ id: 'all', label: 'Replace all files' }, { id: 'none', label: 'Select none' }]}
              onclick={() => void store.apply(root)}
              onselect={(id) => {
                if (id === 'all') { store.setAllApplyFiles(true); void store.apply(root); } else store.setAllApplyFiles(false);
              }}
            />
          {:else}
            <Button variant="primary" disabled={!store.valid || store.previewing} onclick={previewChanges}>
              {#snippet iconStart()}<Eye size={13} />{/snippet}
              Preview changes
            </Button>
          {/if}
        {/if}
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .ss { display: flex; height: 100%; min-height: 0; }
  .ss-main { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }

  .ss-query {
    flex-shrink: 0; display: flex; flex-direction: column; gap: 6px;
    padding: 8px 10px; border-bottom: 1px solid var(--border-subtle);
  }
  .ss-bar { display: flex; align-items: center; gap: 10px; }
  .ss-spacer { flex: 1; }
  .ss-replace-head {
    display: flex; align-items: center; gap: 6px;
    font-size: var(--font-size-2xs); text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted);
  }

  /* The editor inside fills it; the field owns the frame, so the focus and error borders are the ones every other
     input in the app wears. */
  .ss-field {
    display: flex; flex-direction: column;
    height: 96px; min-height: 52px; resize: vertical; overflow: hidden;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
  }
  .ss-field:focus-within { border-color: var(--accent); }
  .ss-field.ss-bad { border-color: var(--error); }
  .ss-field-small { height: 60px; min-height: 44px; }
  .ss-field :global(.cm-editor) { flex: 1; min-height: 0; }

  .ss-err {
    display: flex; align-items: flex-start; gap: 5px; margin: 0;
    font-size: var(--font-size-2xs); color: var(--error);
  }
  .ss-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; min-height: 20px; }
  .ss-chip {
    display: inline-flex; align-items: center; gap: 3px; padding: 1px 7px;
    font-family: var(--font-code); font-size: var(--font-size-2xs); font-weight: 600;
    color: var(--accent); cursor: pointer;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent); border-radius: 999px;
  }
  .ss-chip:hover { background: color-mix(in srgb, var(--accent) 22%, transparent); }
  .ss-chip-new { color: var(--text-muted); background: none; border-style: dashed; border-color: var(--border-default); }
  .ss-chip-new:hover { color: var(--accent); }
  .ss-slow {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: var(--font-size-3xs); color: var(--warning); cursor: help;
  }

  .ss-expand { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .ss-expand summary { cursor: pointer; }
  .ss-expand-line {
    display: block; margin-top: 3px; white-space: pre;
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-secondary);
  }

  .ss-results { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
  .ss-pad { padding: 10px; display: flex; flex-direction: column; gap: 6px; }
  .ss-mid {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; color: var(--text-disabled); font-size: var(--font-size-xs);
  }
  .ss-head {
    display: flex; align-items: center; gap: 8px; flex-shrink: 0;
    padding: 6px 10px; border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .ss-unres {
    display: inline-flex; align-items: center; gap: 4px;
    color: var(--warning); font-size: var(--font-size-2xs); cursor: help;
  }
  .ss-note {
    display: flex; align-items: center; gap: 10px; flex-shrink: 0; margin: 0; padding: 4px 10px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .ss-elapsed { display: inline-flex; align-items: center; gap: 4px; color: var(--text-muted); font-variant-numeric: tabular-nums; cursor: help; }
  .ss-refused { display: flex; align-items: center; gap: 5px; margin: 0; font-size: var(--font-size-2xs); color: var(--text-muted); }
  .ss-hint { display: inline-flex; align-items: center; gap: 6px; font-size: var(--font-size-2xs); color: var(--text-muted); }
  .ss-actions { display: flex; align-items: center; gap: 8px; }
</style>
