<!--
  ConflictDiffColumns — two-column synchronized diff view (ours / theirs).

  Renders the per-file DisplayItem stream as a CSS-subgrid table: column
  headers (sticky), context rows shown in both columns, conflict regions
  with per-line checkboxes plus a row of "Take ours / Take both / Take
  theirs" buttons, and a sticky bottom scrollbar that drives `scrollLeft`
  on every row cell in lock-step.

  The "theirs" column comes in two themes — amber (stash) and blue (merge)
  — driven by the `theme` prop so the same widget serves both modes.

  Layout assumptions: the consumer wraps this in a flex column that gives
  the widget `flex: 1`; the widget then turns the scroll area into a CSS
  grid with `minmax(0, 1fr) 4px minmax(0, 1fr)` columns. Sticky header +
  sticky bottom hscroll bar live inside the same scroll viewport.
-->
<script lang="ts">
  import { ChevronLeft, ChevronRight, ChevronDown, Equal } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { highlight } from '$lib/utils/diff-formatter';
  import type { DisplayItem } from '$lib/utils/conflict/region-types';
  import type { SideState } from '$lib/utils/conflict/conflict-display';

  interface Props {
    /** 'stash' → amber theirs column, 'merge' → blue theirs column. */
    theme:        'stash' | 'merge';
    /** Branch / source labels surfaced in the column headers. */
    oursLabel:    string;
    theirsLabel:  string;
    /** Subline shown in lighter text under the header label ("ours"/"theirs"
     *  for merge mode, "current file in workdir"/"version in stash" for stash). */
    oursSub:      string;
    theirsSub:    string;
    /** Labels for the per-region Take-* buttons. */
    takeOursLabel:   string;
    takeTheirsLabel: string;
    /** "— empty —" label shown when a side has no lines in a region. */
    oursEmptyLabel?:   string;
    theirsEmptyLabel?: string;

    items:        DisplayItem[];
    /** Currently focused conflict block — gets the outline highlight. */
    activeId:     number | null;
    /** Master-checkbox states for both column headers. */
    oursState:    SideState;
    theirsState:  SideState;

    /** Path used as the highlighter's filename hint. */
    path:         string;

    onToggleLine:   (side: 'ours' | 'theirs', regionId: number, lineIdx: number) => void;
    onAcceptOurs:   (regionId: number) => void;
    onAcceptTheirs: (regionId: number) => void;
    onAcceptBoth:   (regionId: number) => void;
    onSetAllSide:   (side: 'ours' | 'theirs', checked: boolean) => void;
    onActivate:     (regionId: number) => void;
    onExpandCollapsed: (contextKey: string) => void;

    /** Bindable scroll wrapper element — the parent uses it to wire the
     *  shared horizontal scrollbar lock-step pattern. */
    scrollEl?:        HTMLElement | null;
    hscrollInnerEl?:  HTMLElement | null;
    onHscroll:        (e: Event) => void;
  }

  let {
    theme, oursLabel, theirsLabel, oursSub, theirsSub,
    takeOursLabel, takeTheirsLabel,
    oursEmptyLabel = '— empty —', theirsEmptyLabel = '— empty —',
    items, activeId, oursState, theirsState, path,
    onToggleLine, onAcceptOurs, onAcceptTheirs, onAcceptBoth, onSetAllSide,
    onActivate, onExpandCollapsed,
    scrollEl = $bindable(null), hscrollInnerEl = $bindable(null), onHscroll,
  }: Props = $props();
</script>

<div class="bcol-scroll" class:theme-merge={theme === 'merge'} class:theme-stash={theme === 'stash'} bind:this={scrollEl}>
  <!-- Column headers: sticky vertically; ride the same horizontal scroll. -->
  <div class="bcol-headers">
    <div class="bcol-header bcol-header-ours">
      <input
        type="checkbox"
        class="bcol-header-cb"
        checked={oursState === 'all'}
        indeterminate={oursState === 'partial'}
        onchange={(e) => onSetAllSide('ours', e.currentTarget.checked)}
        use:tooltip={`Toggle all ${oursLabel} lines`}
      />
      <span class="bcol-header-title">{oursLabel}</span>
      <span class="bcol-header-sub">{oursSub}</span>
    </div>
    <div class="bcol-header-divider"></div>
    <div class="bcol-header bcol-header-theirs">
      <input
        type="checkbox"
        class="bcol-header-cb"
        checked={theirsState === 'all'}
        indeterminate={theirsState === 'partial'}
        onchange={(e) => onSetAllSide('theirs', e.currentTarget.checked)}
        use:tooltip={`Toggle all ${theirsLabel} lines`}
      />
      <span class="bcol-header-title">{theirsLabel}</span>
      <span class="bcol-header-sub">{theirsSub}</span>
    </div>
  </div>

  {#each items as item}
    {#if item.kind === 'context'}
      {#each item.lines as line, i}
        <div class="brow brow-context">
          <div class="brow-left">
            <span class="linenum">{item.oursStart + i}</span>
            <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), path)}</code>
          </div>
          <div class="brow-divider"></div>
          <div class="brow-right">
            <span class="linenum">{item.theirsStart + i}</span>
            <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), path)}</code>
          </div>
        </div>
      {/each}
    {:else if item.kind === 'collapsed'}
      <button
        type="button"
        class="brow-collapsed"
        onclick={() => onExpandCollapsed(item.contextKey)}
        use:tooltip={`Show ${item.hiddenLines} hidden lines`}
      >
        <ChevronDown size={11} />
        <span>… {item.hiddenLines} hidden context lines — click to expand</span>
      </button>
    {:else}
      <div
        class="bregion"
        class:bregion-active={activeId === item.regionId}
        data-conflict-id={item.regionId}
        onclick={() => onActivate(item.regionId)}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onActivate(item.regionId); } }}
        role="button"
        tabindex="0"
        aria-label="Conflict block {item.regionId + 1}"
      >
        <div class="bregion-header">
          <div class="bregion-header-left">
            <span class="bregion-label">{theme === 'stash' ? 'Diff' : 'Conflict'} {item.regionId + 1}</span>
            <button class="bregion-icon bregion-icon-ours"
              onclick={(e) => { e.stopPropagation(); onAcceptOurs(item.regionId); }}
              use:tooltip={`Take ${takeOursLabel}`}>
              <ChevronLeft size={11} />
              <span>{takeOursLabel}</span>
            </button>
          </div>
          <button class="bregion-icon bregion-icon-both"
            onclick={(e) => { e.stopPropagation(); onAcceptBoth(item.regionId); }}
            use:tooltip={'Take both'}>
            <Equal size={10} />
            <span>Both</span>
          </button>
          <div class="bregion-header-right">
            <button class="bregion-icon bregion-icon-theirs"
              onclick={(e) => { e.stopPropagation(); onAcceptTheirs(item.regionId); }}
              use:tooltip={`Take ${takeTheirsLabel}`}>
              <span>{takeTheirsLabel}</span>
              <ChevronRight size={11} />
            </button>
          </div>
        </div>
        <div class="bregion-cols">
          <div class="bregion-col bregion-col-ours">
            {#if item.oursLines.length === 0}
              <div class="bregion-empty">{oursEmptyLabel}</div>
            {:else}
              {#each item.oursLines as line, i}
                {@const sel = item.oursSelected[i] ?? false}
                <div
                  class="bline"
                  class:bline-selected={sel}
                  onclick={() => onToggleLine('ours', item.regionId, i)}
                  role="button"
                  tabindex="0"
                  onkeydown={(e) => e.key === 'Enter' && onToggleLine('ours', item.regionId, i)}
                >
                  <input
                    type="checkbox"
                    class="bline-cb"
                    checked={sel}
                    onchange={() => onToggleLine('ours', item.regionId, i)}
                    onclick={(e) => e.stopPropagation()}
                  />
                  <span class="linenum">{item.oursStart + i}</span>
                  <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), path)}</code>
                </div>
              {/each}
            {/if}
          </div>
          <div class="bregion-sep"></div>
          <div class="bregion-col bregion-col-theirs">
            {#if item.theirsLines.length === 0}
              <div class="bregion-empty">{theirsEmptyLabel}</div>
            {:else}
              {#each item.theirsLines as line, i}
                {@const sel = item.theirsSelected[i] ?? false}
                <div
                  class="bline"
                  class:bline-selected={sel}
                  onclick={() => onToggleLine('theirs', item.regionId, i)}
                  role="button"
                  tabindex="0"
                  onkeydown={(e) => e.key === 'Enter' && onToggleLine('theirs', item.regionId, i)}
                >
                  <span class="linenum">{item.theirsStart + i}</span>
                  <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), path)}</code>
                  <input
                    type="checkbox"
                    class="bline-cb"
                    checked={sel}
                    onchange={() => onToggleLine('theirs', item.regionId, i)}
                    onclick={(e) => e.stopPropagation()}
                  />
                </div>
              {/each}
            {/if}
          </div>
        </div>
      </div>
    {/if}
  {/each}

  <div class="bcol-hscroll" onscroll={onHscroll}>
    <div bind:this={hscrollInnerEl}></div>
  </div>
</div>

<style>
  /* Shared CSS grid: every row subgrids from this 3-column layout
     (1fr / 4px separator / 1fr) so the two panes never diverge.
     Horizontal scroll happens INSIDE each row cell — see .bline. */
  .bcol-scroll {
    flex: 1; overflow: hidden auto; min-height: 0; min-width: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 4px minmax(0, 1fr);
    align-content: start;
  }

  .bcol-hscroll {
    grid-column: 1 / -1;
    position: sticky;
    bottom: 0;
    left: 0;
    width: 100%;
    height: 12px;
    overflow-x: auto;
    overflow-y: hidden;
    background: var(--bg-elevated);
    z-index: 3;
    scrollbar-width: thin;
    scrollbar-color: var(--border) var(--bg-elevated);
  }
  .bcol-hscroll::-webkit-scrollbar       { height: 10px; }
  .bcol-hscroll::-webkit-scrollbar-track { background: var(--bg-elevated); }
  .bcol-hscroll::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: var(--radius-sm);
    border: 2px solid var(--bg-elevated);
  }
  .bcol-hscroll::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }
  .bcol-hscroll > div { height: 1px; }

  .bcol-headers {
    grid-column: 1 / -1;
    display: grid; grid-template-columns: subgrid;
    position: sticky; top: 0; z-index: 2;
    border-bottom: 1px solid var(--border-subtle);
  }
  .bcol-header {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 12px; background: var(--bg-elevated);
  }
  .bcol-header-divider { background: var(--border-subtle); }

  .bcol-header-ours   { border-top: 2px solid rgba(95,173,86,.5); }
  /* Theirs theming: amber for stash, blue for merge. */
  .theme-stash .bcol-header-theirs { border-top: 2px solid rgba(226,163,53,.6); }
  .theme-merge .bcol-header-theirs { border-top: 2px solid rgba(77,120,204,.5); }

  .bcol-header-title {
    font-size: 11px; font-weight: 600; color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }
  .bcol-header-sub {
    font-size: 10px; color: var(--text-muted); font-family: var(--font-ui-sans);
  }

  .bcol-header-cb {
    width: 14px; height: 14px;
    margin: 0 4px 0 0;
    cursor: pointer;
    accent-color: var(--accent);
    flex-shrink: 0;
  }

  .brow {
    grid-column: 1 / -1;
    display: grid; grid-template-columns: subgrid;
    border-bottom: 1px solid var(--border-subtle);
    min-height: 20px;
  }
  .brow:last-child { border-bottom: none; }
  .brow-context { background: var(--bg-base); }

  .brow-collapsed {
    grid-column: 1 / -1;
    display: flex; align-items: center; justify-content: center; gap: 6px;
    padding: 5px 12px;
    background: var(--bg-elevated);
    border: none;
    border-top: 1px dashed var(--border-subtle);
    border-bottom: 1px dashed var(--border-subtle);
    color: var(--text-muted);
    font-size: 10px;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .brow-collapsed:hover { background: var(--bg-hover); color: var(--text-primary); }

  .brow-left, .brow-right {
    display: flex; align-items: baseline; gap: 0;
    padding: 1px 0; min-width: 0; overflow: hidden;
  }
  .brow-divider { width: 4px; background: var(--border-subtle); flex-shrink: 0; }

  .linenum {
    flex-shrink: 0;
    display: inline-block;
    min-width: 36px;
    padding: 0 8px;
    text-align: right;
    font-size: 11px;
    font-family: var(--font-code);
    color: var(--text-disabled);
    user-select: none;
    line-height: 1.6;
  }
  .brow-code {
    font-family: var(--font-code); font-size: 12px; line-height: 1.6;
    color: var(--text-primary); white-space: pre;
    flex: 0 0 auto;
  }

  .bregion {
    grid-column: 1 / -1;
    display: grid; grid-template-columns: subgrid;
    align-content: start;
    border-bottom: 2px solid rgba(226,163,53,.25);
    margin-bottom: 2px;
  }
  .bregion:last-child { margin-bottom: 0; }

  .bregion-header {
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 8px;
    padding: 3px 10px;
    background: rgba(226,163,53,.06);
    border-bottom: 1px solid rgba(226,163,53,.20);
    border-top: 1px solid rgba(226,163,53,.20);
  }
  .bregion-header-left {
    display: flex; align-items: center; gap: 8px;
    min-width: 0;
  }
  .bregion-header-right {
    display: flex; align-items: center;
    justify-content: flex-end;
  }
  .bregion-label {
    font-size: 10px; font-weight: 600; color: var(--warning);
    font-family: var(--font-ui-sans);
    white-space: nowrap;
  }

  .bregion-icon {
    display: inline-flex; align-items: center; justify-content: center;
    gap: 4px;
    height: 18px;
    padding: 0 7px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-secondary);
    font-size: 10px;
    font-weight: 600;
    font-family: var(--font-ui-sans);
    letter-spacing: 0.03em;
    text-transform: uppercase;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .bregion-icon-ours {
    background: rgba(95,173,86,.12);
    border-color: rgba(95,173,86,.35);
    color: var(--success);
  }
  .bregion-icon-ours:hover {
    background: rgba(95,173,86,.22);
    border-color: rgba(95,173,86,.55);
  }
  .bregion-icon-both {
    background: rgba(198,120,221,.14);
    border-color: rgba(198,120,221,.40);
    color: var(--color-tag);
  }
  .bregion-icon-both:hover {
    background: rgba(198,120,221,.24);
    border-color: rgba(198,120,221,.60);
  }
  .bregion-icon-theirs {
    background: rgba(77,120,204,.12);
    border-color: rgba(77,120,204,.35);
    color: var(--accent);
  }
  .bregion-icon-theirs:hover {
    background: rgba(77,120,204,.22);
    border-color: rgba(77,120,204,.55);
  }

  .bregion-cols {
    grid-column: 1 / -1;
    display: grid; grid-template-columns: subgrid;
  }
  .bregion-col { display: flex; flex-direction: column; }
  .bregion-col-ours { background: rgba(95,173,86,.04); }
  .theme-stash .bregion-col-theirs { background: rgba(226,163,53,.04); }
  .theme-merge .bregion-col-theirs { background: rgba(77,120,204,.04); }

  .bregion-sep { background: var(--border-subtle); width: 4px; }
  .bregion-empty {
    padding: 6px 12px 6px 42px;
    font-size: 11px; color: var(--text-disabled);
    font-family: var(--font-ui-sans); font-style: italic;
  }

  .bline {
    display: flex; align-items: baseline; gap: 0;
    padding: 1px 0; cursor: pointer;
    border-left: 2px solid transparent;
    transition: background var(--transition-fast), border-left-color var(--transition-fast);
    user-select: none;
    overflow: hidden;
  }
  .bregion-col-ours .bline:hover { background: rgba(95,173,86,.10); }
  .theme-stash .bregion-col-theirs .bline:hover { background: rgba(226,163,53,.10); }
  .theme-merge .bregion-col-theirs .bline:hover { background: rgba(77,120,204,.10); }

  .bregion-col-ours .bline-selected {
    background: rgba(95,173,86,.15);
    border-left-color: rgba(95,173,86,.6);
  }
  .theme-stash .bregion-col-theirs .bline-selected {
    background: rgba(226,163,53,.15);
    border-left-color: rgba(226,163,53,.6);
  }
  .theme-merge .bregion-col-theirs .bline-selected {
    background: rgba(77,120,204,.15);
    border-left-color: rgba(77,120,204,.6);
  }

  .bline-cb {
    flex-shrink: 0;
    width: 14px; height: 14px;
    margin: 0 4px;
    cursor: pointer;
    accent-color: var(--accent);
    position: sticky;
    left: 4px;
    z-index: 1;
  }
  .bregion-col-ours   .bline-cb:checked { background: var(--success); border-color: var(--success); }
  .theme-stash .bregion-col-theirs .bline-cb:checked { background: var(--warning); border-color: var(--warning); }
  .theme-merge .bregion-col-theirs .bline-cb:checked { background: var(--accent);  border-color: var(--accent); }

  .bregion-col-theirs .bline-cb {
    left: auto;
    right: 4px;
  }

  .bregion-active {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
    border-radius: 2px;
  }
  .bregion-active .bregion-header { background: rgba(77,120,204,.06); }
</style>
