<script lang="ts">
  /**
   * Query editor — write SQL and run it against the tab's connection.
   *
   * The bar above the editor never lets you forget which database you are on:
   * name, colour, schema@host, dialect, and a lock when the session refuses
   * writes.
   *
   * **The rows are not here.** They are a panel of the bottom dock
   * (`QueryResultPanel`), for the reason every other answer in this window is:
   * a grid welded under the editor could not be closed, so a tab that had once
   * run a query kept a third of its height forever.
   *
   * What is here is the decision of *what to run*, and it is the only part that
   * needs the editor: a selection if there is one, otherwise the statement the
   * caret is in. The editor's selection is registered with the query store so
   * that Ctrl+Enter means the same thing from the toolbar and from anywhere else
   * in the window.
   */
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import DocumentBridge from '../DocumentBridge.svelte';
  import { astStore } from '$lib/stores/picus/ast.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { sqlDiagnostics } from '../sql-intel';
  import { abbreviationLines } from '../sql-intel/abbrev';
  import { parseFaultStore } from '$lib/stores/picus/parse-faults.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';
  import { picusEditorStore } from '$lib/stores/picus/editor.svelte';
  import { openObjectNamed } from '../goto-object';
  import { openEditorContextMenu } from '../editor-context-menu';
  import { picusResultsStore } from '$lib/stores/picus/result.svelte';
  import type { PicusTab } from '$lib/types/picus';

  interface Props {
    tab: PicusTab;
  }

  let { tab }: Props = $props();

  let editor = $state<CodeEditor | null>(null);

  const conn = $derived(picusTabsStore.activeConnection);
  // `read` is pure; materialising the record is a write, so it happens in an
  // effect (a write during `$derived` evaluation is a Svelte 5 hard error).
  $effect(() => { queryStore.ensure(tab.id); });
  // Named `tabState` rather than `state`: a local called `state` makes `$state(…)`
  // on the `editor` line above parse as a subscription to *it* instead of as the
  // rune, which is a compile-time coin toss nobody should have to know about.
  const tabState = $derived(queryStore.read(tab.id));
  // Bound to the connection, so completion, hover and the diagnostics all measure
  // this buffer against THIS database's catalogue and no other.
  const language = $derived(sqlLanguage(conn?.dialect, conn?.id));
  /**
   * What is wrong with this buffer, from **both** the sources that can know.
   *
   * The semantic scan is synchronous and deliberately quiet when it does not know:
   * unknown table, unknown column, a write on a read-only connection. The parse is
   * a round trip and is never a matter of not knowing — it is the answer to "is
   * this SQL at all", which the syntax-tree panel has always shown and the editor
   * never did.
   *
   * The abbreviation lines are cut out of the parse for exactly the reason the
   * semantic scan already cuts them out: `s#ordini(id)[stato='EV']` is a shorthand
   * the backend expands, and read as SQL it is nonsense. A line of squiggles under
   * something the tool understands perfectly well is the worst answer available.
   */
  const abbreviations = $derived(abbreviationLines(conn?.id, tabState.sql));

  /**
   * The abbreviation lines, marked as what they are.
   *
   * Without this an abbreviation is coloured as SQL, which it is not: `s#ordini(id)`
   * comes out as a stray identifier, a comment marker and a broken paren, so the one
   * line in the buffer the tool understands *best* is the one that looks most wrong.
   * Marking it says "this is a shorthand, and it is a good one" — or, when the
   * backend refused it, that it is not.
   *
   * The ranges are the backend's own verdict, never a shape test on this side: two
   * opinions about which lines are abbreviations would eventually disagree, and the
   * one drawn would be the wrong one.
   */
  const marks = $derived(
    abbreviations.map((line) => ({
      from: line.from,
      to: line.to,
      className: line.error ? 'picus-abbrev-bad' : 'picus-abbrev',
    })),
  );
  const diagnostics = $derived([
    ...sqlDiagnostics(tabState.sql, conn?.dialect ?? 'postgres', conn?.id),
    ...parseFaultStore
      .for(tabState.sql)
      .filter((fault) => !abbreviations.some((a) => a.from < fault.to && fault.from < a.to)),
  ]);

  /**
   * Let the query store ask this editor where the caret is.
   *
   * Registered rather than passed down, because Run is reachable from three
   * places — this button, the toolbar, and Ctrl+Enter anywhere in the window —
   * and only one of them can see the editor. Cleared on unmount, so a closed
   * tab's editor is never asked.
   */
  $effect(() => {
    const id = tab.id;
    queryStore.bindEditor(
      id,
      () => editor?.selectionRange() ?? { from: 0, to: 0, head: 0, empty: true },
    );
    return () => queryStore.bindEditor(id, null);
  });

  /**
   * And let the *window* reach it, for the commands that are about the document
   * rather than about the query: find and replace, go to a table's structure,
   * replace one match. Same registration, same lifetime.
   */
  $effect(() => {
    const id = tab.id;
    if (!editor) return;
    const held = editor;
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

  /** Run, and reveal the answer — the dock is where every answer in this window is. */
  function run(scope: 'statement' | 'buffer') {
    if (!conn) return;
    picusUiStore.showBottom('results');
    void queryStore.run(tab.id, conn.id, scope);
  }

  /**
   * Run from inside the editor.
   *
   * The window-level handler in `PicusShell` never sees these: CodeMirror binds
   * `Mod-Enter` to `insertBlankLine` and stops the event, so with the caret in the
   * editor — the only place it ever is when you press it — Ctrl+Enter split the
   * line and ran nothing. Claimed here, at the one keystroke's true origin, and
   * always consumed so the newline cannot come back.
   *
   * Static bindings reading live state, never a rebuilt array: the extension set is
   * assembled once at mount.
   */
  const runKeys = [
    { key: 'Mod-Enter', preventDefault: true, run: () => { run('statement'); return true; } },
    { key: 'Mod-Shift-Enter', preventDefault: true, run: () => { run('buffer'); return true; } },
  ];
</script>

<div class="qv">
  <!-- No bar of its own. Run, Run all, Cancel and the connection identity all live
       on the window's contextual toolbar, one row up: this view had a second copy
       of every one of them, so the same action was reachable from two controls
       that then had to be kept in step. -->
  {#if !conn}
    <div class="qv-bar">
      <span class="qv-none">This tab is not bound to a connection.</span>
    </div>
  {/if}

  <!-- The right-click menu is raised from the wrapper rather than from the editor:
       CodeMirror owns its own DOM inside, and a listener there would have to be
       re-attached every time the extension set is rebuilt. -->
  <div
    class="qv-editor"
    oncontextmenu={(e) =>
      openEditorContextMenu(e, {
        editor,
        dialect: conn?.dialect ?? null,
        onRun: () => run('statement'),
      })}
  >
    <!-- Keyed on the descriptor: the editor builds its extensions once, at mount, so
         rebinding the tab to another database has to rebuild them — otherwise the
         completion would keep offering the previous connection's tables. -->
    {#key language}
      <CodeEditor
        bind:this={editor}
        value={tabState.sql}
        {language}
        {diagnostics}
        keyBindings={runKeys}
        {marks}
        oninput={(v) => queryStore.setSql(tab.id, v)}
        oncaret={() => { if (editor) void astStore.revealAt(editor.caretByteOffset()); }}
        onGoto={(word) => openObjectNamed(word, conn?.id)}
      />
    {/key}
    <!-- Keeps the right-hand tools describing THIS buffer. Worth having on a query
         as much as on a script: the syntax tree is how you find out why the
         statement under the cursor ends where it does, and the structural replace
         is how you fix forty of them at once. -->
    <DocumentBridge {editor} text={tabState.sql} dialect={conn?.dialect} />
  </div>
</div>

<style>
  .qv { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }

  /* Only ever drawn for a tab with no connection — the one thing the toolbar
     above cannot say, because it has no database to describe. */
  .qv-bar {
    display: flex;
    align-items: center;
    height: 30px;
    flex-shrink: 0;
    padding: 0 10px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
    white-space: nowrap;
  }
  .qv-none { color: var(--text-disabled); font-style: italic; }

  .qv-editor { flex: 1; min-height: 90px; display: flex; overflow: hidden; }
  .qv-editor > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  /* The abbreviation lines. `:global` because the class is handed to CodeMirror,
     which owns the element it lands on — the one case the shared editor's `marks`
     prop exists for, and the reason it takes a class name rather than a colour.

     A tinted band rather than a coloured foreground: the point is to say "this line
     is not SQL and is not being read as SQL", which is a statement about the whole
     run of text, and recolouring the characters would just be a different wrong
     syntax highlight. */
  .qv-editor :global(.picus-abbrev) {
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 30%, transparent);
  }
  /* Refused by the backend — the same band, in the colour of a thing that will not
     expand. The message itself is already on the line as a diagnostic. */
  .qv-editor :global(.picus-abbrev-bad) {
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--warning) 14%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--warning) 30%, transparent);
  }

</style>
