<script lang="ts">
  /**
   * The console: a stream of interpreted output, however long it gets.
   *
   * One component rather than three. Run, Build and Tests were each rendering the same
   * `RunLogLine[]` with their own copy of the row markup, the level colours and the
   * scroll-to-bottom effect — three places to fix a bug, and three that had already drifted
   * (only Run stopped following when you scrolled up to read something).
   *
   * ## Why it is virtualised
   *
   * The buffer is capped at a few thousand lines, and a few thousand lines is not a small
   * DOM: every line is a row plus one element per interpreted piece — the level, the
   * timestamp, the thread, the logger, each frame of a trace — so a full buffer was on the
   * order of twenty thousand nodes, all of them live, all of them laid out again whenever the
   * panel resized. {@link VirtualTextView} keeps only what is on screen (plus an overscan),
   * which makes the cost of scrolling a function of the viewport rather than of how long the
   * program has been talking.
   *
   * The price it charges is **fixed-height rows**: the virtualiser knows where line *n* is
   * because every line is the same height, and that is what makes the arithmetic exact rather
   * than a guess that drifts as you scroll. So a long line **scrolls sideways** instead of
   * wrapping — the trade a terminal makes, and the reason `white-space` here is `pre`.
   *
   * The height is measured rather than hard-coded: the app's font scale is a user setting, and
   * a constant would put the rows and the virtualiser's idea of the rows out of step the
   * moment someone changed it.
   *
   * A consequence worth knowing: a select-all copies the lines that are *rendered*, not the
   * whole buffer. Anything that needs the entire transcript has to read it from the store.
   *
   * ## The clean view
   *
   * The virtualiser is handed **entries** rather than lines: `buildView` (see `console/log-view.ts`)
   * turns the buffer into the rows to draw — columns dropped or shortened, lines hidden or dimmed
   * by rules, runs of framework frames folded into one row, repeats counted. A row therefore knows
   * its place in the *buffer* (`entry.index`), which is what the assistant button, the row menu and
   * a copy use; the buffer itself is never edited.
   */
  import VirtualTextView from '$lib/components/shared/ui/VirtualTextView.svelte';
  import LogLine from '$lib/components/shared/ui/LogLine.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import type { RunLogLine } from '$lib/stores/bennu/run.svelte';
  import { openLogLink } from './log-link';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { Sparkles, Eye, PanelRight } from 'lucide-svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { findStackTraces, tracePrompt } from './assistant/assistant-trace';
  import { askAssistant } from './assistant/assistant-launch';
  import { bennuLogViewStore as logView } from '$lib/stores/bennu/log-view.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { buildView, entryText, ruleFromLine, type LogViewRule, type RuleAction } from './console/log-view';
  import BennuLogViewSettings from './console/BennuLogViewSettings.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { bennuLogTailStore as tail } from '$lib/stores/bennu/log-tail.svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';

  /** A SQL statement in the log was pressed: copy it, and say so — the click has no other visible effect. */
  function copySql(sql: string) {
    void copyToClipboard(sql, { successToast: 'SQL copied', errorToast: true });
  }

  interface Props {
    /** The buffer to show, oldest first. */
    lines: RunLogLine[];
    /** Shown centred when there is nothing yet. */
    emptyMessage?: string;
    /** Extra class on the viewport. */
    class?: string;
    /** Offer the project's log files (Tomcat's, the application's) as a source besides the output. */
    sources?: boolean;
  }

  let { lines: runLines, emptyMessage = 'No output.', class: klass = '', sources = false }: Props = $props();

  // A followed log file takes the place of the output: the same rows, the same menu, the same links.
  const lines = $derived(tail.active ? tail.lines : runLines);
  const shownEmpty = $derived(tail.active ? (tail.error ?? 'Nothing in this file yet.') : emptyMessage);

  /** The buffer's own text — what the assistant and a copy are given, whatever the view draws. */
  const original = $derived(lines.map((l) => l.text));

  // ── the clean view ──────────────────────────────────────────────────────────
  // Everything below changes what is DRAWN. `lines` is never edited; see `console/log-view.ts`.
  const view = $derived(logView.settings);
  let query = $state('');
  let sideOpen = $state(false);
  const openFolds = new SvelteSet<number>();
  const openHidden = new SvelteSet<number>();

  const entries = $derived(
    buildView(lines, { settings: view, own: logView.own, query, openFolds, openHidden }),
  );
  /** The virtualiser takes plain strings; the interpreted line is looked up by index in the
   *  row snippet, so nothing is copied but the text. */
  const texts = $derived(entries.map(entryText));
  const hiddenCount = $derived(entries.reduce((n, e) => (e.kind === 'hidden' && !e.open ? n + e.count : n), 0));

  $effect(() => {
    void logView.load();
    void logView.detect(projectStore.project?.root);
  });
  $effect(() => {
    if (sources) void tail.refresh(projectStore.project?.root);
  });

  // ── the source: the output, or a log file ───────────────────────────────────
  const SOURCE_OPEN = '__open__';
  const SOURCE_REFRESH = '__refresh__';
  let picking = $state(false);
  const root = $derived(projectStore.project?.root ?? '');
  const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

  const sourceOptions = $derived.by(() => {
    const found = tail.files ?? [];
    const options = [
      { value: '', label: 'Output' },
      ...found.map((f) => ({ value: f.path, label: `${f.name} · ${f.group === 'tomcat' ? 'Tomcat' : f.rel.split('/').slice(0, -1).join('/') || 'project'}` })),
    ];
    // A file opened from elsewhere is not in the list, and must still be what the select shows.
    if (tail.source && !found.some((f) => f.path === tail.source!.path)) options.push({ value: tail.source.path, label: tail.source.name });
    options.push({ value: SOURCE_OPEN, label: 'Open a log file…' }, { value: SOURCE_REFRESH, label: 'Refresh the list' });
    return options;
  });

  function pickSource(value: string | number) {
    const v = String(value);
    if (v === SOURCE_OPEN) picking = true;
    else if (v === SOURCE_REFRESH) void tail.refresh(root);
    else if (v === '') void tail.close();
    else void tail.open(root, v, tail.files?.find((f) => f.path === v)?.name ?? fileName(v));
  }

  const LEVEL_OPTIONS = [
    { value: 'trace', label: 'All levels' }, { value: 'debug', label: 'Debug and above' },
    { value: 'info', label: 'Info and above' }, { value: 'warn', label: 'Warnings and errors' },
    { value: 'error', label: 'Errors only' },
  ];

  /** A fold or a hidden run was pressed: open it, or close it again. */
  function toggleRun(e: { kind: 'fold' | 'hidden'; key: number }) {
    const set = e.kind === 'fold' ? openFolds : openHidden;
    if (!set.delete(e.key)) set.add(e.key);
  }

  // ── a rule from a line ──────────────────────────────────────────────────────
  /** Make a rule from the line under the pointer, and say so with a way back. */
  function addRuleFrom(index: number, how: 'logger' | 'package' | 'thread', action: RuleAction) {
    const rule = ruleFromLine(lines[index], how, action);
    if (!rule) return;
    logView.addRule(rule);
    if (!view.clean) logView.setClean(true);
    const verb = { hide: 'Hiding', dim: 'Dimming', highlight: 'Highlighting' }[action];
    toastStore.show(`${verb} ${rule.field} ${rule.value}`, 'info', 5000, {
      label: 'Undo',
      onClick: () => undoRule(rule),
    });
  }
  function undoRule(rule: LogViewRule) {
    const i = logView.settings.rules.findIndex((r) => r.field === rule.field && r.value === rule.value && r.action === rule.action);
    if (i >= 0) logView.removeRule(i);
  }

  function hideMdcKey(key: string) {
    logView.patch({ hidden_mdc: [...view.hidden_mdc, key] });
    if (!view.clean) logView.setClean(true);
    toastStore.show(`Hiding MDC ${key}`, 'info', 5000, {
      label: 'Undo',
      onClick: () => logView.patch({ hidden_mdc: logView.settings.hidden_mdc.filter((k) => k !== key) }),
    });
  }

  function rowMenu(index: number): MenuItem[] {
    const l = lines[index];
    const has = (how: 'logger' | 'thread') => !!ruleFromLine(l, how, 'hide');
    // The MDC entries this very line carries: one verb each, so hiding "trace_id" is a right-click away.
    const mdcKeys = [...new Set((l.pieces ?? []).filter((p) => p.token === 'mdckey').map((p) => p.text))]
      .filter((k) => !view.hidden_mdc.includes(k));
    return [
      ...mdcKeys.slice(0, 8).map((k) => ({ id: `hide-mdc:${k}`, label: `Hide MDC “${k}”` })),
      ...(mdcKeys.length ? [{ id: 's-mdc', label: '', separator: true }] : []),
      { id: 'hide-logger', label: 'Hide this logger', disabled: !has('logger') },
      { id: 'hide-package', label: 'Hide everything from its package', disabled: !has('logger') },
      { id: 'dim-logger', label: 'Dim this logger', disabled: !has('logger') },
      { id: 'hl-logger', label: 'Highlight this logger', disabled: !has('logger') },
      { id: 'hide-thread', label: 'Hide this thread', disabled: !has('thread') },
      { id: 's', label: '', separator: true },
      { id: 'copy', label: 'Copy the original line' },
    ];
  }
  function onContextMenu(e: MouseEvent) {
    const cell = (e.target as HTMLElement).closest<HTMLElement>('[data-entry]');
    if (!cell) return;
    const index = Number(cell.dataset.entry);
    e.preventDefault();
    bennuContextMenuStore.show(e.clientX, e.clientY, rowMenu(index), (id) => {
      if (id.startsWith('hide-mdc:')) return hideMdcKey(id.slice('hide-mdc:'.length));
      switch (id) {
        case 'hide-logger': return addRuleFrom(index, 'logger', 'hide');
        case 'hide-package': return addRuleFrom(index, 'package', 'hide');
        case 'dim-logger': return addRuleFrom(index, 'logger', 'dim');
        case 'hl-logger': return addRuleFrom(index, 'logger', 'highlight');
        case 'hide-thread': return addRuleFrom(index, 'thread', 'hide');
        case 'copy': return void copyToClipboard(lines[index].text, { successToast: 'Line copied' });
      }
    });
  }

  // The stack traces in the buffer, by the line each starts on: that line gets the assistant's button.
  const traces = $derived(new Map(findStackTraces(original).map((t) => [t.head, t])));

  function askAboutTrace(index: number) {
    const trace = traces.get(index);
    if (trace) void askAssistant(tracePrompt(original, trace));
  }

  /**
   * Follow the tail only while the reader is AT the bottom — scrolling up to read something
   * is a statement that you want to stay there, and a console that yanks you back down on the
   * next line is unusable on a chatty program. {@link VirtualTextView} re-arms it when you
   * scroll back down.
   */
  let follow = $state(true);

  /** The measured height of one row, in px. Zero until the probe lands — see the fallback at
   *  the call site, which only ever shows for the first frame. */
  let rowH = $state(0);

  /**
   * The row's colour class.
   *
   * **Red means the line said it was an error.** Nothing else does — and in particular the
   * *stream* does not, which is the assumption this used to make.
   *
   * A stream is a channel, not a severity, and treating it as one gets the common case backwards
   * in both directions. `cargo` writes its **entire** log to standard error: `Compiling`,
   * `Finished`, `Running`, every warning. Tomcat and every `java.util.logging` default put their
   * ordinary `INFO` chatter there. Painting all of that red leaves nothing to say when something
   * is actually wrong — a console where everything is red is a console with no colours.
   *
   * So a known level maps to its own colour, and a line nobody could interpret is **neutral**,
   * whichever pipe it arrived on. What keeps that from losing real failures is that the two
   * that carry no level word are recognised anyway: Rust's `thread '…' panicked at`, and the
   * JVM's `Error: Could not find or load main class` — see `arbor-logscan`'s `common.rs`.
   *
   * `warn` gets the warning hue rather than being merely spared the red: "not an error" and "a
   * warning" are different things to say, and a build with three hundred quiet lines and two
   * amber ones is readable at a glance in a way neither red nor grey alone is.
   */
  function rowClass(_text: string, index: number): string {
    const e = entries[index];
    if (!e) return '';
    // A fold or a hidden run: a marker, not a line of the program.
    if (e.kind !== 'line') return 'bc-run';
    const l = lines[e.index];
    if (!l) return '';
    const dim = e.dim ? ' bc-dim' : '';
    // These two are ours, not the program's, and never carried a level: what you typed, and our
    // own status lines.
    if (l.stream === 'in' || l.stream === 'meta') return `stream-${l.stream}${dim}`;
    // No class at all when nothing said anything: the row's own colour is the neutral body one,
    // and inventing a `stream-out` class for it would be a name that claims the stream decided.
    return (l.level ? `level-${l.level}` : '') + dim;
  }
</script>

<div class="bc-root">
  <div class="bc-bar">
    {#if sources}
      <Select
        size="sm"
        value={tail.source?.path ?? ''}
        options={sourceOptions}
        highlight={tail.active}
        searchable={sourceOptions.length > 10}
        ariaLabel="What the console shows"
        onchange={pickSource}
      />
    {/if}
    <IconButton
      tooltip="Clean view — hides the columns, lines and stack frames you chose not to read"
      shortcut="Ctrl+Shift+L"
      active={view.clean}
      onclick={() => logView.setClean(!view.clean)}
    ><Eye size={14} /></IconButton>
    <Select
      size="sm"
      value={view.min_level}
      options={LEVEL_OPTIONS}
      highlight={view.min_level !== 'trace'}
      onchange={(v) => logView.patch({ min_level: v as typeof view.min_level })}
    />
    <div class="bc-filter"><FilterInput bind:value={query} placeholder="Filter lines…" ariaLabel="Filter the output" /></div>
    <span class="bc-sp"></span>
    {#if hiddenCount}<span class="bc-hidden">{hiddenCount} line{hiddenCount > 1 ? 's' : ''} hidden</span>{/if}
    <IconButton tooltip="Log view settings" active={sideOpen} onclick={() => (sideOpen = !sideOpen)}><PanelRight size={14} /></IconButton>
  </div>

  <div class="bc-body">
    <!-- Off-layout, and the same type as a row: what it measures is what a row will be, including
         whatever the user has done to the font scale. -->
    <span class="bc-probe" bind:clientHeight={rowH} aria-hidden="true">0</span>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="bc-main" oncontextmenu={onContextMenu}>
      <VirtualTextView
        class="bc {klass}"
        lines={texts}
        lineHeight={rowH || 17}
        bind:follow
        {rowClass}
        emptyMessage={shownEmpty}
      >
        {#snippet line({ index })}
          {@const e = entries[index]}
          <!-- No whitespace between the caret and the line: the row is `pre`, and a newline in the
               template would arrive on the page as a space the program never printed. -->
          {#if e?.kind === 'line'}{@const l = lines[e.index]}{#if l}<span class="bc-cell" data-entry={e.index} style:--hl={e.color} class:bc-hl={!!e.color}>{#if l.stream === 'in'}<span class="bc-caret">&gt;</span>{/if}<LogLine
              text={e.text}
              pieces={e.pieces}
              level={l.level}
              onopen={openLogLink}
              onsql={copySql}
            />{#if e.count > 1}<span class="bc-rep" title="{e.count} identical lines">×{e.count}</span>{/if}{#if traces.has(e.index)}<button type="button" class="bc-ai" title="Ask Imber to find the cause and fix it" onclick={() => askAboutTrace(e.index)}><Sparkles size={10} />Fix with Imber</button>{/if}</span>{/if}
          {:else if e}<button type="button" class="bc-mark" onclick={() => toggleRun(e)}>{texts[index]}</button>{/if}
        {/snippet}
      </VirtualTextView>
    </div>

    <!-- The slide is on this wrapper, a direct child of the `{#if}`: a transition inside the component would be local to it and never play. -->
    {#if sideOpen}
      <div class="bc-side" transition:slide={{ axis: 'x', duration: animStore.dPanel, easing: cubicOut }}>
        <BennuLogViewSettings />
      </div>
    {/if}
  </div>
</div>

{#if picking}
  <FileExplorerModal
    mode="file"
    title="Open a log file"
    initialPath={root}
    onConfirm={(path) => { picking = false; void tail.open(root, path, fileName(path)); }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  /* Takes the place the bare viewport used to have in its parent, whichever way that parent lays out. */
  .bc-root { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; position: relative; }
  .bc-body { flex: 1; min-height: 0; display: flex; }
  .bc-side { flex: none; display: flex; min-height: 0; overflow: hidden; }
  .bc-main { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; }
  .bc-bar {
    display: flex; align-items: center; gap: 6px; flex: none; padding: 3px 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .bc-filter { width: 190px; }
  .bc-sp { flex: 1; }
  .bc-hidden { color: var(--text-muted); font-size: var(--font-size-xs); }
  /* The rest of the row is the cell's, so a highlight spans the line and not just its text. */
  .bc-cell { flex: 1 0 auto; }
  .bc-hl { background: color-mix(in srgb, var(--hl) 16%, transparent); box-shadow: inset 3px 0 0 var(--hl); margin-left: -12px; padding-left: 12px; }
  .bc-rep {
    margin-left: 8px; padding: 0 6px; font-family: var(--font-ui); font-size: 10px; line-height: 16px;
    color: var(--text-muted); border: 1px solid var(--border-subtle); border-radius: 99px;
  }
  /* A fold or a run of hidden lines: quiet, and clearly pressable. */
  .bc-mark {
    padding: 0; background: none; border: none; font: inherit; color: var(--text-muted); cursor: pointer;
  }
  .bc-mark:hover, .bc-mark:focus-visible { color: var(--accent); }
  :global(.vtv.bc .vtv-row.bc-run) { padding-left: 28px; }
  :global(.vtv.bc .vtv-row.bc-dim) { opacity: 0.45; }

  .bc-probe {
    position: absolute;
    top: 0;
    left: -9999px;
    display: block;
    visibility: hidden;
    pointer-events: none;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.5;
  }

  /* The viewport. The rows are rendered by VirtualTextView, so their own rules are `:global`
     through the class it puts on them — `rowClass` exists for exactly this. */
  /* The assistant's button, at the end of the line that starts a trace. Quiet until the row is hovered, so a
     console full of traces is not a wall of buttons. */
  .bc-ai {
    display: inline-flex; align-items: center; gap: 5px; margin-left: 12px; padding: 0 7px;
    font-family: var(--font-ui); font-size: 10px; line-height: 16px;
    color: var(--accent); background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent); border-radius: var(--radius-sm);
    cursor: pointer; opacity: 0.55; transition: opacity var(--transition-fast);
  }
  :global(.vtv.bc .vtv-row:hover) .bc-ai, .bc-ai:focus-visible { opacity: 1; }
  .bc-ai:hover { background: var(--accent); color: var(--text-on-accent); }
  .bc-caret {
    color: var(--accent);
    margin-right: 6px;
    user-select: none;
  }

  :global(.vtv.bc) {
    /* The Run panel puts this in a ROW beside the debugger's columns, and a flex item without
       it refuses to shrink below its widest line — one long line would push the columns off. */
    min-width: 0;
    padding: 6px 0;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.5;
    user-select: text;
  }
  /* Padding on the row rather than on the viewport, so it survives a sideways scroll. */
  :global(.vtv.bc .vtv-row) {
    padding: 0 12px;
    color: var(--text-secondary);
  }
  /* What the line said it was. The stream it arrived on says nothing — see `rowClass`. */
  :global(.vtv.bc .vtv-row.level-fatal) { color: var(--error); font-weight: 600; }
  :global(.vtv.bc .vtv-row.level-error) { color: var(--error); }
  :global(.vtv.bc .vtv-row.level-warn)  { color: var(--warning); }
  /* Routine. The same body colour an ordinary stdout line has: an `INFO` is not worth a hue, and
     spending one on it is how the two that matter stop standing out. */
  :global(.vtv.bc .vtv-row.level-info)  { color: var(--text-secondary); }
  :global(.vtv.bc .vtv-row.level-debug) { color: var(--text-muted); }
  :global(.vtv.bc .vtv-row.level-trace) { color: var(--text-muted); }
  :global(.vtv.bc .vtv-row.stream-meta) { color: var(--text-muted); font-style: italic; }
  /* What you typed, marked the way a shell marks it — so a transcript reads as a conversation
     rather than as the program talking to itself. */
  :global(.vtv.bc .vtv-row.stream-in) { color: var(--text-primary); }
</style>
