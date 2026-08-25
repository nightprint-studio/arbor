<script lang="ts">
  /**
   * Script file view — a file from the repository, opened in the editor.
   *
   * What the file *is* — path, engine, encoding, line ending — is stated once, on
   * the window's toolbar, along with Save. This view used to carry a second bar
   * repeating all of it. What stays here is the one claim that is about the text
   * rather than about the tab: an encoding that has drifted from what the folder
   * declares.
   *
   * Saving preserves the encoding and the line endings, and a character the
   * destination encoding cannot represent **blocks** the save rather than being
   * written as `?` — the same guarantee the generator's writes carry, through the
   * same `prepare_one` / `commit` pair.
   *
   * The buffer is highlighted with its folder's own dialect, because the same
   * text means different things in PL/SQL and PL/pgSQL.
   */
  import { TriangleAlert } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import DocumentBridge from '../DocumentBridge.svelte';
  import { astStore } from '$lib/stores/picus/ast.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { sqlDiagnostics } from '../sql-intel';
  import { longLineMarks } from '../sql-intel/long-line';
  import { validationStore } from '$lib/stores/picus/validation.svelte';
  import { picusProvidersStore } from '$lib/stores/picus/providers.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { connectionsStore, isSessionOpen } from '$lib/stores/picus/connections.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusEditorStore, type PicusEditorHandle } from '$lib/stores/picus/editor.svelte';
  import { openObjectNamed } from '../goto-object';
  import { openEditorContextMenu, type EditorTarget } from '../editor-context-menu';
  import { saveOpenScript } from '../save-script';
  import { isDialect, type PicusTab } from '$lib/types/picus';

  interface Props {
    tab: PicusTab;
  }

  let { tab }: Props = $props();

  const file = $derived(tab.file ? picusProjectStore.fileByPath(tab.file) : null);
  const engine = $derived(tab.file ? picusProjectStore.dialectOfFile(tab.file) : null);
  /**
   * The single dialect to highlight and check against, if there is one.
   *
   * `null` for a portable file as well as for an unclassified one. In both cases
   * the editor still completes keywords and closes blocks — the grammar is one
   * permissive superset — it simply does not claim a dialect it does not have.
   */
  const dialect = $derived(isDialect(engine) ? engine : null);

  /**
   * The text arrives from `picus_script_text`, not from the tree.
   *
   * `loadText` writes the store's cache, so it is called from an `$effect` and
   * never from a `$derived` — a write during derivation is the
   * `state_unsafe_mutation` trap this store family has already paid for once.
   */
  $effect(() => { if (tab.file) void picusProjectStore.loadText(tab.file); });

  const loaded = $derived(tab.file ? picusProjectStore.textFor(tab.file) : null);
  const loadError = $derived(tab.file ? picusProjectStore.textErrorFor(tab.file) : '');
  const loadingText = $derived(!!tab.file && picusProjectStore.isTextLoading(tab.file));

  /**
   * The text this view last put on screen, kept across the store's cache being
   * emptied.
   *
   * Re-reading the repository throws every buffer away — deliberately, since they
   * came from the previous read — and **a save triggers a re-read**. Without this
   * the view fell back to its loading state for the moment in between, which
   * unmounts CodeMirror; the editor came back a frame later scrolled to the top,
   * so saving halfway down a long script threw you back to line 1. It is keyed by
   * path so it can never hand one file's text to another.
   */
  let shown = $state<{ path: string; text: string } | null>(null);
  $effect(() => {
    const path = tab.file;
    const fresh = loaded?.text;
    if (path && fresh !== undefined) shown = { path, text: fresh };
  });
  const held = $derived(shown && shown.path === tab.file ? shown.text : null);

  const text = $derived(loaded?.text ?? held ?? '');

  /**
   * The encoding the backend actually decoded with wins over the tree's entry:
   * they are read at different moments, and the one that produced the bytes on
   * screen is the one a later write has to preserve.
   */
  const encoding = $derived(loaded?.encoding ?? file?.encoding ?? '');
  const eol = $derived(loaded?.eol ?? file?.eol ?? 'LF');
  const drifted = $derived(!!file && !!encoding && encoding !== file.expectedEncoding);

  /**
   * Which catalogue this script may be measured against.
   *
   * A script file has no connection of its own, so it borrows the active one —
   * but **only when the dialects agree**. Checking an Oracle script against a
   * PostgreSQL database would report the whole file as unknown, and one wrong
   * warning costs more than every right one gains. With no match the editor still
   * completes keywords, closes blocks and says nothing about objects.
   */
  const catalogue = $derived(
    dialect && connectionsStore.active?.dialect === dialect ? connectionsStore.active.id : undefined,
  );
  /** …and whether that borrowed connection is actually open. A closed one has a
   *  catalogue cached but no session, so it can complete names and cannot validate. */
  const catalogueOpen = $derived(!!catalogue && isSessionOpen(connectionsStore.active));
  const language = $derived(sqlLanguage(dialect, catalogue));

  // The buffer as edited. The diagnostics follow what is on screen rather than
  // what was loaded — markers anchored to the text on disk would drift on the
  // first keystroke — and the toolbar compares this against the loaded text to
  // know whether there is anything to save.
  let edited = $state<string | null>(null);
  $effect(() => { void tab.file; edited = null; });
  const buffer = $derived(edited ?? text);
  /** Whether the borrowed connection can validate — its engine, and its session.
   *  Without the second half a closed connection left the last green tick on screen. */
  const canValidate = $derived(
    catalogueOpen && (picusProvidersStore.capabilities(dialect)?.validate ?? false),
  );
  // The colour the editor gives up on past 10 000 characters into a line, put
  // back for the literals — see `sql-intel/long-line.ts`.
  const marks = $derived(longLineMarks(buffer, dialect ?? 'oracle'));

  const diagnostics = $derived([
    ...sqlDiagnostics(buffer, dialect ?? 'oracle', catalogue),
    ...validationStore.for(buffer),
  ]);

  // Validate against the borrowed catalogue when there is one; a file with no
  // matching connection simply reads as "unavailable".
  $effect(() => {
    validationStore.follow(buffer, catalogue, canValidate);
  });
  $effect(() => () => validationStore.clear());

  /**
   * Reveal the line a finding pointed at.
   *
   * The tab carries the request (`revealLine`) plus a nonce, because stepping to
   * two findings on the same line has to move the caret both times. The editor's
   * own `scrollToLineCol` does the work — it is already the shared imperative API
   * Bennu's go-to uses, so nothing is forked here.
   *
   * It runs after the text lands: revealing line 24 of an empty buffer would land
   * on line 1 and look like the navigation was ignored.
   */
  // Structural, matching Bennu's `EditorController`: the shared editor's
  // imperative surface is what a host binds to, not the component's whole type.
  let editor = $state<
    | ({
        scrollToLineCol: (line: number, col?: number) => void;
        caretByteOffset: () => number;
      } & PicusEditorHandle &
        EditorTarget)
    | null
  >(null);
  $effect(() => {
    const line = tab.revealLine;
    const nonce = tab.revealNonce;
    void nonce;
    if (!line || !editor || !buffer) return;
    editor.scrollToLineCol(line);
  });

  /**
   * Let the window's commands reach this editor — find and replace, go to a
   * table's structure, replace one match.
   *
   * The same registration a query tab makes. Both are "the document in front", and
   * a binding that worked in one and not the other would be a bug the user meets
   * by switching tabs.
   */
  $effect(() => {
    const id = tab.id;
    const held = editor;
    if (!held) return;
    picusEditorStore.bind(id, {
      focus: () => held.focus(),
      openSearch: () => held.openSearch(),
      getValue: () => held.getValue(),
      selectionRange: () => held.selectionRange(),
      selectRange: (from, to) => held.selectRange(from, to),
      selectByteRange: (a, b) => held.selectByteRange(a, b),
      replaceByteRange: (a, b, text) => held.replaceByteRange(a, b, text),
      replaceByteRanges: (edits) => held.replaceByteRanges(edits),
      wordAtCaret: () => held.wordAtCaret(),
    });
    return () => picusEditorStore.bind(id, null);
  });

  /** Which source decided this file's encoding — never leave the guess silent. */
  const SOURCE_LABEL = {
    bom: 'declared by a byte-order mark',
    utf8: 'valid UTF-8 with multibyte characters',
    inherited: 'pure ASCII — inherited from the folder',
    heuristic: 'single-byte heuristic',
    forced: 'pinned by hand',
  } as const;
</script>

{#if !file}
  <StateBlock tone="error" label="This file is no longer in the repository index." />
{:else if loadError}
  <StateBlock tone="error">
    <div class="fv-fail">
      <strong>{file.path} could not be read.</strong>
      <span>{loadError}</span>
      <Button variant="secondary" size="xs" onclick={() => void picusProjectStore.loadText(file.path, true)}>
        Try again
      </Button>
    </div>
  </StateBlock>
{:else if held === null && loadingText}
  <StateBlock tone="loading">
    {#snippet spinner()}<Spinner size={14} />{/snippet}
    <span>Reading {file.name}…</span>
  </StateBlock>
{:else}
  <div class="fv">
    <!-- No bar of its own. The path, the engine, the encoding and the drift all
         live on the window's toolbar one row up: this view had a second copy of
         every one of them, so an editor opened on a script started two rows of
         chrome down and the same facts were stated twice. What is genuinely this
         view's — the drift warning, which is about the file and not about the tab
         — sits inside the editor's own frame below. -->
    {#if drifted}
      <div class="fv-drift">
        <TriangleAlert size={12} />
        <span>
          This file is {encoding} where {SOURCE_LABEL[file.encodingSource].toLowerCase()}
          says {file.expectedEncoding}. Saving keeps it as it is.
        </span>
        <Button
          variant="secondary"
          size="xs"
          onclick={() => toastStore.show(`Conversion back to ${file.expectedEncoding} arrives with the rewriter.`, 'info')}
        >
          Convert back
        </Button>
      </div>
    {/if}

    <!-- Same menu as a query tab, plus the verb this tab actually has. Raised from
         the wrapper: CodeMirror owns the DOM inside and rebuilds it whenever the
         extension set changes. -->
    <!-- `presentation`: a positioning wrapper with no meaning of its own, so it stays
         out of the accessibility tree rather than announcing a group. -->
    <div
      class="fv-code"
      role="presentation"
      oncontextmenu={(e) =>
        openEditorContextMenu(e, {
          editor,
          dialect,
          onSave: () => { if (tab.file) void saveOpenScript(tab.file); },
        })}
    >
      <!-- Keyed on the descriptor: the editor builds its extensions at mount, so a
           change of borrowed catalogue (connecting, or switching database) has to
           rebuild them rather than keep the previous one's tables. -->
      {#key language}
        <CodeEditor
          bind:this={editor}
          value={buffer}
          {language}
          {diagnostics}
          {marks}
          oninput={(v) => { edited = v; picusTabsStore.markDirty(tab.id, true); }}
          oncaret={() => { if (editor) void astStore.revealAt(editor.caretByteOffset()); }}
          onGoto={(word) => openObjectNamed(word, connectionsStore.activeId)}
        />
      {/key}
      <!-- Keeps the right-hand tools describing THIS buffer, and turns a click in
           one of them into a selection here. -->
      <!-- The dialect matters here as much as in a query tab: it decides what ends a
           statement, and a file with no engine is parsed as the portable intersection
           rather than as whichever one comes first. -->
      <DocumentBridge {editor} text={edited ?? buffer ?? ''} {dialect} />
    </div>
  </div>
{/if}

<style>
  .fv { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }

  /* A file whose encoding drifted is already wrong on disk — and unlike the
     facts on the toolbar, this one is a claim about the file rather than a label
     for the tab, so it stays here where the text it describes is. */
  .fv-drift {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding: 5px 10px;
    background: color-mix(in srgb, var(--error) 9%, var(--bg-base));
    border-bottom: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    font-size: var(--font-size-xs);
  }
  .fv-drift :global(svg) { color: var(--error); flex-shrink: 0; }
  .fv-drift span { flex: 1; min-width: 0; color: var(--text-secondary); }

  .fv-fail { display: flex; flex-direction: column; align-items: center; gap: 6px; }
  .fv-fail strong { font-size: var(--font-size-sm); }
  .fv-fail span { font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); max-width: 70ch; }

  .fv-code { flex: 1; min-height: 0; display: flex; overflow: hidden; }
  .fv-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
