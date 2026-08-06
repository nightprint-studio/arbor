<script lang="ts">
  /**
   * Structural search & replace (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd>).
   *
   * ## What the layout is saying
   *
   * The query **across the top**, the answer under it, the selected match **beside** it.
   *
   * The query is two or three lines of code with holes in it — never a document — so giving it
   * a tall column of its own wasted the width the answers need and put a line-number gutter
   * next to a field nobody refers to by line. It gets a band; the results get the room.
   *
   * The answer is one of two things and the query decides which: a **table** when it groups, a
   * **list of places** when it does not. That is not a mode you pick — it follows from what you
   * asked, which is the point of putting `group` in the language rather than in a toolbar. The
   * preview column follows the list and is absent under the table, because a row of a table is
   * a count and has no single place to show.
   *
   * ## The list alone does not answer the question
   *
   * `OrderDao.java:412 — svc.place(o)` says where; it does not say whether that is the
   * `place` you meant. The lines around it do, which is why walking the results with ↑/↓
   * re-reads the file beside them and nothing is opened until Enter.
   *
   * ## The field talks back while you type
   *
   * `bennu_ssr_explain` touches no files, so every keystroke gets an answer: whether the query
   * reads, what it binds, how many alternatives it will try, and — for `use of` — the patterns it
   * stands for. A structural query is easy to get subtly wrong, and an empty result list is the
   * worst possible way to be told.
   *
   * ## Replace is two steps, always
   *
   * Preview, then apply what the preview showed. The whole reason to prefer a structural replace
   * over a textual one is that it is precise, and precision is only worth something if you can
   * check it.
   */
  import {
    Search, Replace, Play, Loader, TriangleAlert, FileCode2, CircleAlert, Check, X,
    LayoutTemplate, Timer,
  } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import CodePreview from '$lib/components/shared/ui/CodePreview.svelte';
  import { languageForPath } from './languages';
  import ExportButton, { type Rendition } from '$lib/components/shared/internal/ExportButton.svelte';
  import { exportRows, EXPORT_EXTENSION, type ExportFormat } from '$lib/utils/tabular-export';
  import { readFile } from '$lib/ipc/bennu';
  import { untrack } from 'svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuSsrStore } from '$lib/stores/bennu/ssr.svelte';
  import type { SsrDialect } from '$lib/ipc/bennu/ssr';
  import { SSR_EXAMPLES } from './ssr-examples';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import {
    capturesIn, setQueryDialect, setReplacementCaptures, ssrQueryLanguage, ssrReplacementLanguage,
  } from './ssr-lang';

  let { onClose }: { onClose: () => void } = $props();

  const store = bennuSsrStore;
  const root = $derived(projectStore.project?.root ?? '');
  const explained = $derived(store.explained);
  const report = $derived(store.report);

  $effect(() => { void store.attach(); });

  // The replacement field completes the QUERY's captures, and a `LanguageDescriptor` is static —
  // the editor takes it at mount and its completion source is a plain function. So the names are
  // pushed to the module as the query changes. One structural-search modal is open at a time,
  // which is what makes a module-level value the right shape rather than a shortcut.
  $effect(() => { setReplacementCaptures(capturesIn(store.query)); });
  // The node kinds `#` offers are the grammar's vocabulary, so they follow the language the
  // query is written in — pushed to the module for the same reason as the captures above.
  $effect(() => { setQueryDialect(store.dialect); });

  /** Open a hit. Closes the modal — you asked to go somewhere. */
  function open(file: string, line: number) {
    void projectStore.openFile(file).then(() => bennuUiStore.requestGoto(line));
    onClose();
  }

  /** The one line under the results that says what the scan actually did. Worth showing:
   *  a query with nothing to pre-filter on parses every file, and this is the only place
   *  that explains why it took a while. */
  const scanNote = $derived.by(() => {
    if (store.searching || !report) return null;
    if (!store.prefiltered) {
      return `${store.parsed} files parsed — this query has no literal to narrow by, so every file was read`;
    }
    return `${store.parsed} of ${store.scanned} files parsed`;
  });

  /**
   * The templates, as a menu.
   *
   * They are read once, while you are learning what the language can be asked — a permanent
   * column for them spent a third of the panel on something nobody looks at twice. Each one's
   * `why` becomes the subtitle, so the menu still says what the query is *for* rather than
   * being a list of titles you have to try one by one.
   */
  const templateItems = $derived<DropdownItem[]>(
    SSR_EXAMPLES.filter((ex) => (ex.dialect ?? 'java') === store.dialect).map((ex) => ({
      kind: 'item' as const,
      id: ex.query,
      label: ex.title,
      subtitle: ex.why,
      onclick: () => store.load(ex.query),
    })),
  );

  /**
   * The three languages a query can be written in. Java first: it is what most queries are.
   *
   * The third is not a third language — it is Java again, pointed at the `<% … %>` blocks of the
   * pages. Given its own entry because *which files are walked* is as much a part of the choice
   * as which grammar reads the pattern, and "Java, in the pages" is a question people have.
   */
  const DIALECTS = [
    { value: 'java', label: 'Java', description: 'Search .java sources' },
    { value: 'jsp', label: 'JSP', description: 'Search the tags of .jsp / .jspf / .tag pages' },
    {
      value: 'jsp-java',
      label: 'Java in JSP',
      description: 'A Java query, run over the <% … %> blocks of the pages',
    },
  ];

  /** The placeholder is a working query in the current language — the field's shortest possible
   *  lesson, and a Java one shown over a page search would teach the wrong shape. */
  const queryPlaceholder = $derived(
    store.dialect === 'jsp'
      ? '<s:property $pre...$ value="$x$" $post...$/>\ngroup $x$'
      : store.dialect === 'jsp-java'
        ? 'session.getAttribute($key$)\ngroup $key$'
        : '$o: com.acme.OrderService$.$m$($args...$)\ngroup $m$',
  );

  /**
   * What "no results" actually means, which is two different things.
   *
   * A query that read every file and matched none of them is a statement about the project. A
   * query that found **no files to read** is a statement about the scope — the wrong language
   * picked, or a scope naming a directory that is not there — and telling someone their project
   * contains none of what they asked for, when nothing was ever opened, is how an hour goes.
   */
  const nothingFound = $derived(
    store.scanned === 0
      ? store.dialect === 'java'
        ? 'No .java files were found to search. Check the language picker and any in scope.'
        : 'No .jsp, .jspf, .jspx or .tag files were found to search. Check the language picker and any in scope.'
      : `Nothing in this project matches that — ${store.scanned} file${store.scanned === 1 ? '' : 's'} read.`,
  );

  /** How long the scan took, in the unit that reads: `840ms`, `4.2s`. */
  const elapsed = $derived.by(() => {
    const ms = store.elapsedMs;
    if (!ms) return null;
    return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
  });

  // ── the selected match, and the file around it ───────────────────────────────
  let sel = $state(0);
  /** Clamped where it is read, so a batch landing mid-scan never drags the selection. */
  const selected = $derived(store.hits.length ? Math.min(sel, store.hits.length - 1) : 0);
  const current = $derived(store.hits[selected]);
  /** The row's identity — the preview keys off this, so a batch arriving does not re-read the
   *  file under a selection that has not moved. */
  const currentKey = $derived(current ? `${current.rel}:${current.range.start}` : '');

  const fileCache = new Map<string, string>();
  /** The selected match's file, whole — see `CodePreview` for why not a window. */
  let previewText = $state('');
  let previewOf = $state('');

  $effect(() => {
    void currentKey;
    const hit = untrack(() => current);
    if (!hit || !root) { previewText = ''; previewOf = ''; return; }
    let live = true;
    void (async () => {
      let text = fileCache.get(hit.file);
      if (text === undefined) {
        try {
          text = (await readFile(root, hit.file)).text;
          fileCache.set(hit.file, text);
        } catch {
          if (live) { previewText = ''; previewOf = hit.rel; }
          return;
        }
      }
      if (!live) return;
      previewOf = hit.rel;
      previewText = text;
    })();
    return () => { live = false; };
  });

  function move(delta: number) {
    if (!store.hits.length) return;
    sel = Math.min(Math.max(selected + delta, 0), store.hits.length - 1);
    queueMicrotask(() => {
      document.querySelector(`[data-ssr-row="${sel}"]`)?.scrollIntoView({ block: 'nearest' });
    });
  }

  // ── export ──────────────────────────────────────────────────────────────────
  /**
   * The results, taken out.
   *
   * Two shapes because the answer has two shapes: a grouped query produced a **table** and its
   * rows are what you would paste into a spreadsheet, an ungrouped one produced **places** and
   * those are the rows. Exporting the places under a grouped query would hand back the thing
   * the query deliberately summarised.
   *
   * `unresolved` is a column of its own rather than folded into the count, for the same reason
   * the panel shows it apart: a total that silently included what the classpath could not
   * decide is a number that looks complete and is not.
   */
  function exportText(format: ExportFormat): string {
    if (report?.groupedBy) {
      return exportRows(
        report.rows,
        [
          { key: report.groupedBy, value: (r) => r.key },
          { key: 'count', value: (r) => r.count },
          { key: 'files', value: (r) => r.files },
          { key: 'undecided', value: (r) => r.unresolved },
        ],
        format,
      );
    }
    return exportRows(
      store.hits,
      [
        { key: 'file', value: (h) => h.rel },
        { key: 'line', value: (h) => h.line },
        { key: 'match', value: (h) => h.preview },
        { key: 'enclosing', value: (h) => h.enclosing ?? '' },
        { key: 'undecided', value: (h) => (h.unresolved ? 'yes' : '') },
      ],
      format,
    );
  }

  const renditions = $derived<Rendition[]>(
    (['csv', 'json', 'markdown'] as ExportFormat[]).map((format) => ({
      id: format,
      label: format === 'csv' ? 'As CSV' : format === 'json' ? 'As JSON' : 'As a Markdown table',
      extension: EXPORT_EXTENSION[format],
      text: () => exportText(format),
    })),
  );

  const exportSubject = $derived(
    report?.groupedBy
      ? `${report.rows.length} row${report.rows.length === 1 ? '' : 's'}`
      : `${store.hits.length} match${store.hits.length === 1 ? '' : 'es'}`,
  );
  const nothingToExport = $derived(!store.hits.length && !report?.rows.length);

  function onKeydown(e: KeyboardEvent) {
    // Ctrl+Enter runs, the way it submits every other modal in the app.
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void store.search(root);
      return;
    }
    // The arrows walk the results — but only from outside the query editor, which owns them
    // for moving the caret.
    if (e.target instanceof HTMLElement && e.target.closest('.cm-editor')) return;
    if (e.key === 'ArrowDown') { e.preventDefault(); move(1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
    else if (e.key === 'Enter' && current) { e.preventDefault(); open(current.file, current.line); }
  }
</script>

<Modal {onClose} width="1080px" height="680px" padBody={false} ariaLabel="Structural search">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Search size={14} />
      <span class="modal-title">Structural search</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="ss" role="group" onkeydown={onKeydown}>
    <div class="ss-query">
      <div class="ss-query-bar">
        <span class="ss-label">Query</span>
        <!-- The language is asked, never inferred. A pattern compiled under the wrong grammar
             does not fail loudly — it matches nothing, which reads as "the project contains
             none of this", and that is the one wrong answer a search must not be able to give
             silently. It also decides which files are walked, so it is a scope as much as a
             syntax. -->
        <RadioGroup
          value={store.dialect}
          options={DIALECTS}
          size="sm"
          onchange={(v) => store.setDialect(v as SsrDialect)}
        />
        <span class="ss-bar-sep"></span>
        <!-- The templates are a menu, not a column. They are read once, when you are learning
             what the language can be asked; leaving them permanently on screen spent a third of
             the panel on something nobody looks at twice. -->
        <Dropdown items={templateItems}>
          <!-- The trigger owns the click: `Dropdown`'s wrapper binds only `onkeydown`, so a
               trigger that does not call `toggle` is a button that opens nothing. -->
          {#snippet trigger({ open, toggle })}
            <Button variant="ghost" size="xs" onclick={toggle} ariaExpanded={open}>
              {#snippet iconStart()}<LayoutTemplate size={13} />{/snippet}
              Templates
            </Button>
          {/snippet}
        </Dropdown>
        <span class="ss-bar-spacer"></span>
        <div class="ss-replace-head">
          <span>Replace</span>
          <Toggle
            checked={store.replacing}
            onchange={(v) => store.setReplacing(v)}
            ariaLabel="Show the replacement"
          />
        </div>
      </div>

      <!-- A real editor rather than a textarea: a query is code with holes in it, and the one
           thing that has to stand out — a hole from the code around it — is invisible in plain
           text. It also completes the five things nobody can be expected to remember: the
           clause words, what `group` accepts (including this query's own captures), the
           grammar's node kinds, `@type`/`@value`, and the project's class names.
           No gutter: it is three lines, and nobody refers to a part of it by number. -->
      <div class="ss-field" class:ss-bad={!!explained?.error}>
        <CodeEditor
          language={ssrQueryLanguage}
          value={store.query}
          oninput={(text) => store.setQuery(text)}
          placeholder={queryPlaceholder}
          lineNumbers={false}
          wrap
        />
      </div>

      {#if explained?.error}
        <p class="ss-err">
          <CircleAlert size={12} />
          <span>{explained.error}{explained.errorLine ? ` (line ${explained.errorLine})` : ''}</span>
        </p>
      {:else if explained}
        <p class="ss-meta">
          {#if explained.alternatives > 1}
            <Badge variant="tone" tone="info" size="sm" label={`${explained.alternatives} alternatives`} />
          {/if}
          {#each explained.captures as name (name)}
            <Badge variant="tone" tone="neutral" size="sm" label={`$${name}$`} />
          {/each}
          {#if !explained.literals.length}
            <!-- Said, not implied: this is the difference between a query that reads a handful
                 of files and one that parses the whole project. -->
            <span class="ss-slow" use:tooltip={'Nothing constant to grep for, so every file is parsed'}>
              <TriangleAlert size={11} /> whole-project scan
            </span>
          {/if}
        </p>
      {/if}

      {#if explained?.expansion.length}
        <!-- A shortcut that shows its expansion is a shortcut you can learn from, and one you
             can copy out and edit when it does not do quite what you want. -->
        <details class="ss-expand">
          <summary>What <code>use of</code> looks for</summary>
          {#each explained.expansion as line (line)}<code class="ss-expand-line">{line}</code>{/each}
        </details>
      {/if}

      {#if store.replacing}
        <div class="ss-field ss-field-small">
          <CodeEditor
            language={ssrReplacementLanguage}
            value={store.replacement}
            oninput={(text) => store.setReplacement(text)}
            placeholder={'Optional.ofNullable($a$).map(X::$m$)'}
            lineNumbers={false}
            wrap
          />
        </div>
      {/if}
    </div>

    <div class="ss-results">
      {#if store.error}
        <div class="ss-pad"><Alert variant="error" compact title="That did not run" text={store.error} /></div>
      {:else if store.previewing}
        <div class="ss-mid"><Spinner size={16} /><span>Working out what would change…</span></div>
      {:else if store.preview}
        <!-- The preview: what every file would become, before anything is written. -->
        <div class="ss-head">
          <strong>{store.preview.hits}</strong> change{store.preview.hits === 1 ? '' : 's'} in
          <strong>{store.preview.files.length}</strong> file{store.preview.files.length === 1 ? '' : 's'}
        </div>
        <div class="ss-list">
          {#each store.preview.files as f (f.file)}
            <details class="ss-file">
              <summary>
                <FileCode2 size={12} />
                <span class="ss-file-name">{f.rel}</span>
                <Badge variant="tone" tone="neutral" size="sm" label={String(f.hits)} />
              </summary>
              <div class="ss-diff">
                <pre class="ss-before">{f.before}</pre>
                <pre class="ss-after">{f.after}</pre>
              </div>
            </details>
          {/each}
        </div>
      {:else if store.applyResult}
        <div class="ss-pad">
          <Alert
            variant={store.applyResult.refused.length ? 'warning' : 'success'}
            title={`${store.applyResult.written} file${store.applyResult.written === 1 ? '' : 's'} rewritten`}
            text={store.applyResult.refused.length
              ? `${store.applyResult.refused.length} refused — they changed after the preview was built, so the plan no longer described them.`
              : 'Every file the preview showed was written.'}
          />
          {#each store.applyResult.refused as r (r.file)}
            <p class="ss-refused"><X size={11} /> {r.file} — {r.reason}</p>
          {/each}
        </div>
      {:else if store.searching && !store.hits.length}
        <div class="ss-mid"><Spinner size={16} /><span>Searching…</span></div>
      {:else if report && report.groupedBy}
        <!-- Grouped: the table. -->
        <div class="ss-head">
          <strong>{report.total}</strong> in <strong>{report.files}</strong> file{report.files === 1 ? '' : 's'},
          by <code>{report.groupedBy}</code>
          {#if report.unresolved}
            <span class="ss-unres" use:tooltip={'The classpath could not decide a type constraint for these'}>
              <TriangleAlert size={11} /> {report.unresolved} undecided
            </span>
          {/if}
          {#if store.searching}<Spinner size={11} />{/if}
        </div>
        <table class="ss-table">
          <thead><tr><th>Key</th><th>Count</th><th>Files</th></tr></thead>
          <tbody>
            {#each report.rows as row (row.key)}
              <tr>
                <td class="ss-key">{row.key}</td>
                <td class="ss-num">
                  {row.count}
                  {#if row.unresolved}<span class="ss-unres-n">({row.unresolved}?)</span>{/if}
                </td>
                <td class="ss-num">{row.files}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if store.hits.length}
        <!-- Ungrouped: the places. -->
        <div class="ss-head">
          <strong>{store.hits.length}</strong> match{store.hits.length === 1 ? '' : 'es'}
          {#if store.capped}<Badge variant="tone" tone="warning" size="sm" label="capped" />{/if}
          {#if store.searching}<Spinner size={11} />{/if}
        </div>
        <!-- The places, and the selected one in context. The list says where; only the lines
             around it say whether that is the one you meant. -->
        <div class="ss-split">
          <div class="ss-list">
            {#each store.hits as hit, i (`${hit.rel}:${hit.range.start}`)}
              <button
                class="ss-hit"
                class:ss-on={i === selected}
                data-ssr-row={i}
                type="button"
                onmousemove={() => (sel = i)}
                onclick={() => open(hit.file, hit.line)}
              >
                <span class="ss-hit-where">{hit.rel}:{hit.line}</span>
                <span class="ss-hit-code">{hit.preview}</span>
                {#if hit.enclosing}<span class="ss-hit-in">in {hit.enclosing}</span>{/if}
                {#if hit.unresolved}
                  <TriangleAlert size={11} class="ss-hit-warn" />
                {/if}
              </button>
            {/each}
          </div>
          <div class="ss-preview">
            {#if previewText && current}
              <div class="ss-pv-head" title={previewOf}>{previewOf}</div>
              <div class="ss-pv-body">
                <!-- The matched bytes are marked, not just the line: a structural pattern can
                     match a fragment of a long statement, and banding the whole line would say
                     less than the query did. -->
                <CodePreview
                  text={previewText}
                  language={languageForPath(current.file)}
                  activeLine={current.line}
                  markBytes={current.range}
                />
              </div>
            {:else}
              <p class="ss-pv-note">{previewOf ? 'This file can’t be read.' : 'Reading…'}</p>
            {/if}
          </div>
        </div>
      {:else if report}
        <EmptyState message={nothingFound} />
      {:else}
        <EmptyState message="Write a query and press Ctrl+Enter. The examples on the left are a good place to start." />
      {/if}

      {#if scanNote || elapsed}
        <p class="ss-note">
          {#if elapsed}
            <!-- Worth its own place: this is the one panel whose cost is variable AND
                 explainable — a query with a literal to grep for reads a tenth of the project,
                 one made only of holes reads all of it. Beside the file counts, the number
                 turns the pre-filter into something you can aim at. -->
            <span class="ss-elapsed"><Timer size={11} /> {elapsed}</span>
          {/if}
          {#if scanNote}<span>{scanNote}</span>{/if}
        </p>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="ss-hint"><Kbd keys={['Ctrl', 'Enter']} size="sm" /> run</span>
      <div class="ss-actions">
      <ExportButton
        {renditions}
        fileName="structural-search"
        subject={exportSubject}
        empty={nothingToExport}
        emptyTooltip="Run a query first — there is nothing to take out yet"
        tooltip="Take these results out of Arbor — to the clipboard, or to a file"
      />
      <Button variant="ghost" onclick={onClose}>Close</Button>
      {#if store.replacing}
        <Button
          variant="secondary"
          disabled={!store.valid || store.previewing}
          onclick={() => void store.buildPreview(root)}
        >
          {#snippet iconStart()}<Replace size={13} />{/snippet}
          Preview
        </Button>
        <Button
          variant="primary"
          disabled={!store.preview?.files.length}
          onclick={() => void store.apply(root)}
        >
          {#snippet iconStart()}<Check size={13} />{/snippet}
          Apply
        </Button>
      {:else}
        <Button
          variant="primary"
          disabled={!store.valid || store.searching}
          onclick={() => void store.search(root)}
        >
          {#snippet iconStart()}
            {#if store.searching}<Loader size={13} />{:else}<Play size={13} />{/if}
          {/snippet}
          Search
        </Button>
      {/if}
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  /* Query across the top, answer under it. A query is three lines, not a document — a column
     of its own spent width the answers needed. */
  .ss { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  .ss-query {
    flex-shrink: 0; display: flex; flex-direction: column; gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .ss-query-bar { display: flex; align-items: center; gap: 10px; }
  .ss-bar-spacer { flex: 1; }
  /* Separates the language — which decides what a query even means — from the actions beside
     it, so the bar reads as "this query, in this language" and then "things you can do". */
  .ss-bar-sep {
    width: 1px; align-self: stretch; margin: 2px 2px;
    background: var(--border-subtle);
  }
  .ss-label {
    font-size: var(--font-size-2xs); text-transform: uppercase; letter-spacing: 0.05em;
    color: var(--text-muted);
  }
  /* The editor inside fills it; the field owns the frame, so the focus and error borders are
     the same ones every other input in the app wears. */
  .ss-field {
    display: flex; flex-direction: column;
    height: 84px; min-height: 52px; resize: vertical; overflow: hidden;
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
  .ss-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; margin: 0; }
  .ss-slow {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: var(--font-size-3xs); color: var(--warning); cursor: help;
  }

  .ss-expand { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .ss-expand summary { cursor: pointer; }
  .ss-expand-line {
    display: block; margin-top: 3px;
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-secondary);
    white-space: pre;
  }

  .ss-replace-head {
    display: flex; align-items: center; gap: 6px;
    font-size: var(--font-size-2xs); text-transform: uppercase; letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  /* The answer side. */
  .ss-results { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
  .ss-pad { padding: 10px; display: flex; flex-direction: column; gap: 6px; }
  .ss-mid {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; color: var(--text-disabled); font-size: var(--font-size-xs);
  }
  .ss-head {
    display: flex; align-items: center; gap: 8px; flex-shrink: 0;
    padding: 7px 10px; border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .ss-unres {
    display: inline-flex; align-items: center; gap: 4px;
    color: var(--warning); font-size: var(--font-size-2xs); cursor: help;
  }
  .ss-note {
    display: flex; align-items: center; gap: 10px;
    flex-shrink: 0; margin: 0; padding: 4px 10px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .ss-elapsed {
    display: inline-flex; align-items: center; gap: 4px;
    color: var(--text-muted); font-variant-numeric: tabular-nums;
  }

  /* Places left, the selected one in context right. */
  .ss-split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 3fr) minmax(0, 2fr); }
  .ss-split .ss-list { border-right: 1px solid var(--border-subtle); }
  .ss-preview { min-height: 0; display: flex; flex-direction: column; background: var(--bg-base); }
  .ss-pv-head {
    flex-shrink: 0; padding: 5px 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    direction: rtl; text-align: left;
  }
  /* No `overflow` of its own: the editor inside scrolls, and a second scroller around it is two
     scrollbars for one document. */
  .ss-pv-body { flex: 1; min-height: 0; }
  .ss-pv-note { padding: 12px; font-size: var(--font-size-xs); color: var(--text-disabled); }

  .ss-table { width: 100%; border-collapse: collapse; font-size: var(--font-size-xs); }
  .ss-table th {
    position: sticky; top: 0;
    padding: 5px 10px; text-align: left;
    background: var(--bg-elevated); color: var(--text-muted);
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.05em;
    border-bottom: 1px solid var(--border-subtle);
  }
  .ss-table td { padding: 4px 10px; border-bottom: 1px solid var(--border-subtle); }
  .ss-table tbody tr:hover { background: var(--bg-hover); }
  .ss-key { font-family: var(--font-code); color: var(--text-primary); }
  .ss-num { text-align: right; font-family: var(--font-code); color: var(--text-secondary); width: 80px; }
  .ss-unres-n { color: var(--warning); margin-left: 4px; }

  .ss-list { flex: 1; min-height: 0; overflow: auto; }
  .ss-hit {
    display: flex; align-items: baseline; gap: 8px; width: 100%;
    padding: 3px 10px; text-align: left;
    background: none; border: none; cursor: pointer;
  }
  .ss-hit:hover { background: var(--bg-hover); }
  .ss-hit.ss-on { background: var(--bg-selected); }
  .ss-hit-where {
    flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-2xs);
    color: var(--text-muted);
  }
  .ss-hit-code {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary);
  }
  .ss-hit-in { flex-shrink: 0; font-size: var(--font-size-3xs); color: var(--text-disabled); }

  .ss-file { border-bottom: 1px solid var(--border-subtle); }
  .ss-file summary {
    display: flex; align-items: center; gap: 6px;
    padding: 5px 10px; cursor: pointer; font-size: var(--font-size-xs);
  }
  .ss-file summary:hover { background: var(--bg-hover); }
  .ss-file-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-code); }
  .ss-diff { display: grid; grid-template-columns: 1fr 1fr; gap: 1px; background: var(--border-subtle); }
  .ss-diff pre {
    margin: 0; padding: 6px 8px; overflow: auto; max-height: 260px;
    background: var(--bg-base);
    font-family: var(--font-code); font-size: var(--font-size-3xs); line-height: 1.5;
    white-space: pre;
  }
  .ss-before { color: var(--text-muted); }
  .ss-after { color: var(--text-primary); }

  .ss-refused {
    display: flex; align-items: center; gap: 5px; margin: 0;
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .ss-hint { display: inline-flex; align-items: center; gap: 5px; font-size: var(--font-size-2xs); color: var(--text-muted); }
  .ss-actions { display: flex; align-items: center; gap: 8px; }
</style>
