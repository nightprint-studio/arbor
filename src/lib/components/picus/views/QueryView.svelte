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
  import { Play, Square, Lock, ListOrdered } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import AstBridge from '../AstBridge.svelte';
  import { astStore } from '$lib/stores/picus/ast.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { sqlDiagnostics } from '../sql-intel';
  import { tooltip } from '$lib/actions/tooltip';
  import { connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';
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
  const state = $derived(queryStore.read(tab.id));
  // Bound to the connection, so completion, hover and the diagnostics all measure
  // this buffer against THIS database's catalogue and no other.
  const language = $derived(sqlLanguage(conn?.dialect, conn?.id));
  // Re-runs on the text, the connection and the schema; the analysis is a linear
  // scan and returns nothing at all while the catalogue is unread.
  const diagnostics = $derived(
    sqlDiagnostics(state.sql, conn?.dialect ?? 'postgres', conn?.id),
  );

  // Only for the Cancel button: the background row count keeps running after the
  // statement itself has finished, and a Cancel that could not stop it would be a
  // button for the cheap half of the work. The rows themselves are the dock's.
  const result = $derived(picusResultsStore.forOwner(tab.id));

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
  <!-- Which database this tab talks to — always visible, never inferred. -->
  <div class="qv-bar">
    {#if conn}
      <span class="qv-dot" style:background={connectionColorVar(conn)}></span>
      <span class="qv-name">{conn.name}</span>
      <span class="qv-host">{conn.schema}@{conn.host}</span>
      <PicusDialectChip dialect={conn.dialect} />
      <span class="qv-spacer"></span>
      <Badge variant="tone" tone="neutral" size="sm" label={`db ${conn.dbVersion}`} />
      {#if conn.readOnly}
        <span class="qv-ro" use:tooltip={'The backend refuses write statements on this connection'}>
          <Lock size={11} /> read-only
        </span>
      {/if}
      <Button
        variant="primary"
        size="xs"
        disabled={state.running}
        tooltip={{
          content: 'Run the selection, or the statement under the cursor',
          shortcut: 'Ctrl+Enter',
        }}
        onclick={() => run('statement')}
      >
        {#snippet iconStart()}<Play size={12} />{/snippet}
        Run
      </Button>
      <!-- The whole buffer is a *second* key, never the default. A scratchpad
           holds yesterday's INSERTs above today's SELECT, and a Run that sent the
           file would execute them again. -->
      <Button
        variant="secondary"
        size="xs"
        disabled={state.running}
        ariaLabel="Run every statement in this tab"
        tooltip={{
          content: 'Run every statement in this tab, in order, stopping at the first failure',
          shortcut: 'Ctrl+Shift+Enter',
        }}
        onclick={() => run('buffer')}
      >
        {#snippet iconStart()}<ListOrdered size={12} />{/snippet}
        Run all
      </Button>
      {#if state.running || result?.counting}
        <!-- Also while only the background count is running: that count is the
             expensive part on a large table, and a Cancel that could not stop it
             would be a button for the cheap half of the work. -->
        <Button
          variant="secondary"
          size="xs"
          tooltip={{ content: state.running ? 'Stop the statement' : 'Stop counting the exact number of rows', shortcut: 'Ctrl+Shift+C' }}
          onclick={() => void queryStore.cancel(tab.id, conn.id)}
        >
          {#snippet iconStart()}<Square size={12} />{/snippet}
          Cancel
        </Button>
      {/if}
    {:else}
      <span class="qv-none">This tab is not bound to a connection.</span>
    {/if}
  </div>

  <div class="qv-editor">
    <!-- Keyed on the descriptor: the editor builds its extensions once, at mount, so
         rebinding the tab to another database has to rebuild them — otherwise the
         completion would keep offering the previous connection's tables. -->
    {#key language}
      <CodeEditor
        bind:this={editor}
        value={state.sql}
        {language}
        {diagnostics}
        keyBindings={runKeys}
        oninput={(v) => queryStore.setSql(tab.id, v)}
        oncaret={() => { if (editor) void astStore.revealAt(editor.caretByteOffset()); }}
      />
    {/key}
    <!-- Keeps the syntax-tree panel describing THIS buffer. Worth having on a
         query as much as on a script: the panel is how you find out why the
         statement under the cursor ends where it does. -->
    <AstBridge {editor} text={state.sql} />
  </div>
</div>

<style>
  .qv { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }

  .qv-bar {
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
  .qv-dot { width: 8px; height: 8px; border-radius: 2px; flex-shrink: 0; }
  .qv-name { font-weight: 500; }
  .qv-host { color: var(--text-muted); font-family: var(--font-code); font-size: 10.5px; }
  .qv-none { color: var(--text-disabled); font-style: italic; }
  .qv-spacer { flex: 1; }
  .qv-ro {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--warning);
    font-size: 11px;
  }

  .qv-editor { flex: 1; min-height: 90px; display: flex; overflow: hidden; }
  .qv-editor > :global(*) { flex: 1; min-width: 0; min-height: 0; }

</style>
