<script lang="ts">
  /**
   * Read-only `.grove` code view for the mocked editor: line-number gutter +
   * coarse syntax highlighting (see highlight.ts). Supports a transient
   * highlighted line (Ctrl+G goto-line target) and Ctrl+Click on an identifier
   * to "go to declaration" (mocked — flashes the matching `let`/`fn`/`import`).
   *
   * The real editor (Phase 4) replaces this with CodeMirror 6 + Tree-sitter;
   * the surrounding shell, tabs and keybindings stay.
   */
  import { tokenizeLine } from './highlight';

  let {
    source,
    /** 1-based line to flash/scroll to (Ctrl+G), or null. */
    flashLine = null,
    /** Called when the user Ctrl+Clicks an identifier — value is the word. */
    onGotoDecl,
  }: {
    source: string;
    flashLine?: number | null;
    onGotoDecl?: (word: string, line: number) => void;
  } = $props();

  const lines = $derived(source.replace(/\n$/, '').split('\n'));
  let scroller = $state<HTMLElement | null>(null);

  // Scroll the flashed line into view when it changes.
  $effect(() => {
    if (flashLine == null || !scroller) return;
    const el = scroller.querySelector(`[data-line="${flashLine}"]`) as HTMLElement | null;
    el?.scrollIntoView({ block: 'center' });
  });

  function onLineClick(e: MouseEvent, lineNo: number) {
    if (!(e.ctrlKey || e.metaKey)) return;
    const target = e.target as HTMLElement;
    const word = target.dataset.word;
    if (word) { e.preventDefault(); onGotoDecl?.(word, lineNo); }
  }
</script>

<div class="cv" bind:this={scroller}>
  {#each lines as line, idx}
    {@const lineNo = idx + 1}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="cv-row"
      class:flash={flashLine === lineNo}
      data-line={lineNo}
      onclick={(e) => onLineClick(e, lineNo)}
    >
      <span class="cv-gutter">{lineNo}</span>
      <span class="cv-code">
        {#each tokenizeLine(line) as tok}
          {#if tok.kind === 'ident' || tok.kind === 'fn'}
            <span class="tok tok-{tok.kind} word" data-word={tok.text}>{tok.text}</span>
          {:else}
            <span class="tok tok-{tok.kind}">{tok.text}</span>
          {/if}
        {/each}
        {#if line === ''}{'​'}{/if}
      </span>
    </div>
  {/each}
</div>

<style>
  .cv {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--bg-base);
    font-family: var(--font-code);
    font-size: 12.5px;
    line-height: 1.55;
    padding: 6px 0 40vh;   /* generous bottom pad so the last lines can center */
    tab-size: 2;
  }
  .cv-row {
    display: flex;
    align-items: baseline;
    white-space: pre;
    padding: 0 12px 0 0;
  }
  .cv-row:hover { background: color-mix(in srgb, var(--bg-hover) 50%, transparent); }
  .cv-row.flash { background: var(--accent-subtle); animation: cv-flash 1.4s ease-out; }
  @keyframes cv-flash {
    0%   { background: color-mix(in srgb, var(--accent) 32%, transparent); }
    100% { background: transparent; }
  }

  .cv-gutter {
    flex-shrink: 0;
    width: 42px;
    padding-right: 12px;
    text-align: right;
    color: var(--text-disabled);
    user-select: none;
    font-variant-numeric: tabular-nums;
  }
  .cv-code { flex: 1; min-width: 0; color: var(--text-primary); }

  .word { cursor: text; }
  /* Ctrl-hover affordance: underline identifiers when a modifier is held.
     (Pure CSS can't read Ctrl, so we keep it subtle — hover only.) */
  .cv-row:hover .word:hover { text-decoration: underline dotted; cursor: pointer; }

  /* ── Token palette ──
     Fixed One-Dark-ish hues: distinct per category and readable on dark
     backgrounds. This is a stop-gap for the Step 0 mock — Phase 4 wires
     CodeMirror 6 + a proper theme-aware grammar that replaces these. */
  .tok-comment  { color: #7a828e; font-style: italic; }
  .tok-string   { color: #98c379; }
  .tok-keyword  { color: #c678dd; font-weight: 600; }   /* let/fn/tracks/import… */
  .tok-island   { color: #56b6c2; font-weight: 600; }   /* s() / n() islands     */
  .tok-fn       { color: #61afef; }                      /* transforms / methods  */
  .tok-number   { color: #d19a66; }                      /* numbers / degrees     */
  .tok-note     { color: #e5c07b; }                      /* notes & $splice vars  */
  .tok-operator { color: #abb2bf; }                      /* & * / ! @ : ~ _ [] <> */
  .tok-punct    { color: #828a99; }                      /* . , =                 */
  .tok-ident    { color: var(--text-primary); }
  .tok-plain    { color: var(--text-primary); }
</style>
