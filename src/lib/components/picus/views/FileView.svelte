<script lang="ts">
  /**
   * Script file view — a file from the repository, opened in the editor.
   *
   * The banner above the buffer is the point: it states the encoding, the line
   * ending, and whether either has drifted from what the folder expects. Saving
   * preserves both; a character that cannot be represented in the destination
   * encoding blocks the save rather than being replaced by `?`.
   *
   * The buffer is highlighted with its folder's own dialect, because the same
   * text means different things in PL/SQL and PL/pgSQL.
   */
  import { TriangleAlert, FileCode2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import DocumentBridge from '../DocumentBridge.svelte';
  import { astStore } from '$lib/stores/picus/ast.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { sqlDiagnostics } from '../sql-intel';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusEditorStore, type PicusEditorHandle } from '$lib/stores/picus/editor.svelte';
  import { openObjectNamed } from '../goto-object';
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
  const text = $derived(loaded?.text ?? '');

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
  const language = $derived(sqlLanguage(dialect, catalogue));

  // The buffer as edited. Nothing persists it yet (saving arrives with the
  // rewriter), but the diagnostics have to follow what is on screen — markers
  // anchored to the text as it was loaded would drift on the first keystroke.
  let edited = $state<string | null>(null);
  $effect(() => { void tab.file; edited = null; });
  const buffer = $derived(edited ?? text);
  const diagnostics = $derived(sqlDiagnostics(buffer, dialect ?? 'oracle', catalogue));

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
      } & PicusEditorHandle)
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
{:else if !loaded && loadingText}
  <StateBlock tone="loading">
    {#snippet spinner()}<Spinner size={14} />{/snippet}
    <span>Reading {file.name}…</span>
  </StateBlock>
{:else}
  <div class="fv">
    <div class="fv-bar" class:fv-bad={drifted}>
      <FileCode2 size={13} />
      <span class="fv-path">{file.path}</span>
      {#if engine}<PicusDialectChip {engine} />{/if}
      <EncodingPill
        {encoding}
        expected={file.expectedEncoding}
        {eol}
        onChange={() => toastStore.show('Re-encoding a file arrives with the rewriter.', 'info')}
      />
      <span class="fv-source">{SOURCE_LABEL[file.encodingSource]}</span>
      <span class="fv-spacer"></span>
      {#if drifted}
        <span class="fv-warn">
          <TriangleAlert size={12} />
          expected {file.expectedEncoding}
        </span>
        <Button
          variant="secondary"
          size="xs"
          onclick={() => toastStore.show(`Conversion back to ${file.expectedEncoding} arrives with the rewriter.`, 'info')}
        >
          Convert back
        </Button>
      {/if}
    </div>

    <div class="fv-code">
      <!-- Keyed on the descriptor: the editor builds its extensions at mount, so a
           change of borrowed catalogue (connecting, or switching database) has to
           rebuild them rather than keep the previous one's tables. -->
      {#key language}
        <CodeEditor
          bind:this={editor}
          value={buffer}
          {language}
          {diagnostics}
          oninput={(v) => { edited = v; picusTabsStore.markDirty(tab.id, true); }}
          oncaret={() => { if (editor) void astStore.revealAt(editor.caretByteOffset()); }}
          onGoto={(word) => openObjectNamed(word, connectionsStore.activeId)}
        />
      {/key}
      <!-- Keeps the right-hand tools describing THIS buffer, and turns a click in
           one of them into a selection here. -->
      <DocumentBridge {editor} text={edited ?? buffer ?? ''} />
    </div>
  </div>
{/if}

<style>
  .fv { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }

  .fv-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    flex-shrink: 0;
    padding: 0 10px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11.5px;
    white-space: nowrap;
  }
  /* A file whose encoding drifted is already wrong on disk — say so loudly. */
  .fv-bar.fv-bad {
    background: color-mix(in srgb, var(--error) 9%, var(--bg-elevated));
    border-bottom-color: color-mix(in srgb, var(--error) 30%, transparent);
  }
  .fv-bar :global(svg) { color: var(--text-disabled); flex-shrink: 0; }

  .fv-path {
    font-family: var(--font-code);
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 380px;
  }
  .fv-source { color: var(--text-disabled); font-size: 10.5px; }
  .fv-spacer { flex: 1; }
  .fv-warn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--error);
    font-weight: 600;
  }
  .fv-warn :global(svg) { color: var(--error); }

  .fv-fail { display: flex; flex-direction: column; align-items: center; gap: 6px; }
  .fv-fail strong { font-size: 12px; }
  .fv-fail span { font-size: 11.5px; line-height: 1.5; color: var(--text-muted); max-width: 70ch; }

  .fv-code { flex: 1; min-height: 0; display: flex; overflow: hidden; }
  .fv-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
