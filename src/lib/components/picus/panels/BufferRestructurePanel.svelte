<script lang="ts">
  /**
   * Structural replace, scoped to the document in front of you.
   *
   * The same pattern language as the repository-wide migration, in a tool panel
   * instead of a workspace — because the two are used at completely different
   * moments. That one is a migration you refine over an afternoon: a scope, groups,
   * a diff per file, a write that names every path first. This one is the edit you
   * make *while writing the statement*: forty inserts pasted out of a ticket that
   * all need the same column added.
   *
   * So it is arranged as a field, not as a page. Matches are found as you type —
   * in the pattern or in the document — and the only action is **Replace all**,
   * which hands the ranges to the editor as one ordinary edit. Ctrl+Z takes it
   * back, which is the property that makes it safe to press without a preview.
   */
  import { Replace, CornerDownRight, TriangleAlert } from 'lucide-svelte';
  import type { EditorView, KeyBinding } from '@codemirror/view';
  import { tooltip } from '$lib/actions/tooltip';

  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { bufferRestructureStore } from '$lib/stores/picus/restructure-buffer.svelte';
  import { picusEditorStore } from '$lib/stores/picus/editor.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import type { Hit } from '$lib/ipc/picus/restructure';

  const store = bufferRestructureStore;

  /** The editor to act on. Everything else is meaningless without one. */
  const host = $derived(picusEditorStore.active);

  /**
   * The pattern is SQL, so it gets the SQL editor — highlighting and completion
   * over the connected database's tables and columns. Bound to the **tab's**
   * connection rather than the sidebar's selection, because that is the database
   * the document in front is about.
   */
  const conn = $derived(picusTabsStore.activeConnection);
  const language = $derived(sqlLanguage(conn?.dialect, conn?.id));

  let patternBox = $state<CodeEditor | null>(null);
  let replacementBox = $state<CodeEditor | null>(null);

  /**
   * The keyboard contract of the two boxes.
   *
   * `Tab` cycles between them and `Escape` leaves the panel, because
   * `indentWithTab` is part of the shared editor and without these the panel would
   * be somewhere the keyboard can enter and not leave. They sit below the
   * completion keymap, so while the popup is open it still owns Tab and Escape.
   *
   * `Ctrl+Enter` is the terminal action — the whole flow is type, Tab, type,
   * Ctrl+Enter, without reaching for the mouse once.
   */
  function boxKeys(next: () => CodeEditor | null): KeyBinding[] {
    return [
      { key: 'Mod-Enter', preventDefault: true, run: () => { store.replaceAll(); return true; } },
      { key: 'Tab', preventDefault: true, run: () => { next()?.focus(); return true; } },
      {
        key: 'Shift-Tab',
        preventDefault: true,
        run: () => { next()?.focus(); return true; },
      },
      {
        key: 'Escape',
        preventDefault: true,
        run: (view: EditorView) => { view.contentDOM.blur(); return true; },
      },
    ];
  }

  const patternKeys = boxKeys(() => replacementBox);
  const replacementKeys = boxKeys(() => patternBox);

  /** One line of the match, so a row stays a row in a 300px panel. */
  function oneLine(text: string): string {
    return text.replace(/\s+/g, ' ').trim();
  }

  function rowLabel(hit: Hit): string {
    return `Line ${hit.line}: ${oneLine(hit.text)}`;
  }
</script>

<PanelShell title="Structural replace">
  {#snippet icon()}<Replace size={13} />{/snippet}

  {#snippet actions()}
    {#if store.scanning}<Spinner size={11} />{/if}
    <Button
      variant="ghost"
      size="xs"
      tooltip="What the pattern language can express"
      onclick={() => picusUiStore.openDocs('restructuring')}
    >
      Syntax
    </Button>
  {/snippet}

  {#if !host}
    <StateBlock
      tone="info"
      label="Open a query or a script and this rewrites the statements in it."
    />
  {:else}
    <div class="br">
      <div class="br-field">
        <span id="br-find-label">Find</span>
        <div class="br-box" role="group" aria-labelledby="br-find-label">
          <!-- Keyed on the descriptor: the editor builds its extensions at mount,
               so rebinding the tab to another database has to rebuild them, or the
               completion keeps offering the previous connection's tables. -->
          {#key language}
            <CodeEditor
              bind:this={patternBox}
              value={store.pattern}
              {language}
              placeholder={'INSERT INTO ORDINI ($cols...$) VALUES ($vals...$)'}
              wrap
              keyBindings={patternKeys}
              oninput={(v) => store.setPattern(v)}
            />
          {/key}
        </div>
      </div>

      <div class="br-field">
        <span id="br-replace-label">Replace with</span>
        <div class="br-box" role="group" aria-labelledby="br-replace-label">
          {#key language}
            <CodeEditor
              bind:this={replacementBox}
              value={store.replacement}
              {language}
              placeholder={'INSERT INTO ORDINI ($cols$, STATO) VALUES ($vals$, \'NUOVO\')'}
              wrap
              keyBindings={replacementKeys}
              oninput={(v) => store.setReplacement(v)}
            />
          {/key}
        </div>
      </div>

      {#if store.error && !store.matches.length}
        <!-- Quiet, and only when there is nothing to show: the pattern is being
             typed, and half of one is not an error worth shouting about. -->
        <div class="br-note">{store.error}</div>
      {/if}

      <div class="br-bar">
        <Badge
          variant="tone"
          tone={store.matches.length ? 'accent' : 'neutral'}
          size="sm"
          label={store.asked
            ? `${store.matches.length} match${store.matches.length === 1 ? '' : 'es'}`
            : 'nothing to find yet'}
        />
        {#if store.failing.length}
          <Badge
            variant="tone"
            tone="warning"
            size="sm"
            label={`${store.failing.length} cannot be written`}
          />
        {/if}
        <span class="br-spacer"></span>
        <Button
          variant="primary"
          size="xs"
          disabled={!store.ready.length}
          tooltip={{
            content: store.rewriting
              ? 'Rewrite every match, as one edit — Ctrl+Z takes it back'
              : 'Write a replacement above first',
            shortcut: 'Ctrl+Enter',
          }}
          onclick={() => store.replaceAll()}
        >
          Replace all
        </Button>
      </div>

      {#if store.failing.length}
        <div class="br-alert">
          <Alert
            variant="warning"
            compact
            title="Some matches cannot be written"
            text="They caught a different number of elements than the template addresses. They are marked below and are left alone; the rest are still rewritten."
          />
        </div>
      {/if}

      <!-- The matches. A row reveals itself in the editor; its own button rewrites
           just that one, which is what you want when the pattern is right for
           thirty-nine of forty. -->
      <ul class="br-list">
        {#each store.matches as hit (hit.range.start)}
          <li class="br-item" class:br-on={store.selectedAt === hit.range.start}>
            <button
              type="button"
              class="br-hit"
              use:tooltip={rowLabel(hit)}
              onclick={() => store.reveal(hit)}
            >
              <span class="br-line">{hit.line}</span>
              <span class="br-text">{oneLine(hit.text)}</span>
              {#if hit.problem}
                <span class="br-bad" use:tooltip={hit.problem}><TriangleAlert size={11} /></span>
              {:else if hit.replacement}
                <span class="br-after">
                  <CornerDownRight size={10} />
                  {oneLine(hit.replacement)}
                </span>
              {/if}
            </button>
            {#if hit.replacement && !hit.problem}
              <Button
                variant="icon"
                size="xs"
                ariaLabel={`Rewrite the match on line ${hit.line}`}
                tooltip="Rewrite only this one"
                onclick={() => store.replaceOne(hit)}
              >
                {#snippet iconStart()}<Replace size={12} />{/snippet}
              </Button>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</PanelShell>

<style>
  .br {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }

  .br-field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 8px 0 8px;
    flex-shrink: 0;
  }
  .br-field > span {
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }

  /* Three or four wrapped lines: a pattern is a statement, not a document, and
     the matches underneath are the part you are reading — but 62px with no
     wrapping showed one line and a horizontal scrollbar, so the start of what
     you were typing scrolled out of its own box. */
  .br-box {
    height: 84px;
    display: flex;
    min-width: 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .br-note {
    padding: 6px 10px 0 10px;
    font-size: var(--font-size-xs);
    line-height: 1.45;
    color: var(--text-muted);
  }

  .br-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px;
    flex-shrink: 0;
  }
  .br-spacer { flex: 1; }

  .br-alert { padding: 0 8px 8px 8px; }

  .br-list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    list-style: none;
    border-top: 1px solid var(--border-subtle);
  }

  .br-item {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 4px;
  }
  .br-item:hover { background: var(--bg-hover); }
  .br-on { background: var(--bg-selected); }

  .br-hit {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 4px 6px 4px 8px;
    background: none;
    border: none;
    text-align: left;
    cursor: pointer;
    font-size: var(--font-size-xs);
    color: var(--text-primary);
  }

  .br-line {
    flex-shrink: 0;
    min-width: 22px;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    text-align: right;
  }

  .br-text,
  .br-after {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
  }
  .br-text { flex: 1; }
  .br-after {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 3px;
    color: var(--success);
  }
  .br-bad { color: var(--warning); flex-shrink: 0; }
</style>
