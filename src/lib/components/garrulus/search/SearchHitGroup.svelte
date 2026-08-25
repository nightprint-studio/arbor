<script lang="ts">
  /**
   * One note's block of results: a sticky header, and the excerpt under it.
   *
   * Grouping per note rather than listing every match flat is the whole
   * difference the design asks for (`docs/garrulus-design.md` §12.14). A flat list
   * makes the reader recover "which note is this?" from a path repeated on every
   * row; grouped, the note is stated once, stays visible while its matches scroll
   * past, and the excerpt is free to be about the text.
   *
   * **What the count means.** The index cuts *one* excerpt per note, around the
   * first match, and highlights each distinct query term once inside it. The pill
   * therefore counts terms highlighted in this excerpt — which is what it says
   * when hovered — and is hidden when there is only one, because "1" next to a
   * single visible highlight is noise. The day the backend returns several
   * excerpts per note, `rows` takes them and nothing else here changes.
   */
  import { ChevronDown, ChevronRight, FileText } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { noteFolder, noteName } from '../panels/note-path';
  import { highlightSegments } from './highlight';
  import type { Hit } from '$lib/ipc/garrulus';

  interface Props {
    hit: Hit;
    /** This note's block is the one the keyboard is on. */
    active?: boolean;
    collapsed?: boolean;
    onToggle: () => void;
    /** Show this note in the preview pane. */
    onSelect: () => void;
    /** Open it for real. Absent → the row still selects, and nothing pretends
     *  there is an editor to open it in. */
    onOpen?: () => void;
  }

  let { hit, active = false, collapsed = false, onToggle, onSelect, onOpen }: Props = $props();

  const folder = $derived(noteFolder(hit.id));
  const fallbackName = $derived(noteName(hit.id));
  /** The index resolves a title from frontmatter, the first heading or the file
   *  name; only fall back to the id when it somehow had none. */
  const title = $derived(hit.title || fallbackName);
  const titleParts = $derived(highlightSegments(title, hit.title_matches));
  const excerpt = $derived(
    hit.snippet ? highlightSegments(hit.snippet.text, hit.snippet.ranges) : null,
  );
  const marks = $derived(hit.snippet?.ranges.length ?? 0);

  function onHeaderKey(e: KeyboardEvent) {
    // ←/→ fold this note's block without leaving the list, the way a tree does.
    if (e.key === 'ArrowLeft' && !collapsed) {
      e.preventDefault();
      onToggle();
    } else if (e.key === 'ArrowRight' && collapsed) {
      e.preventDefault();
      onToggle();
    } else if (e.key === 'Enter' && onOpen) {
      e.preventDefault();
      onOpen();
    }
  }
</script>

<section class="hg" class:active data-hit-group={hit.id}>
  <div class="hg-head">
    <button
      type="button"
      class="hg-fold"
      aria-label={collapsed ? 'Show this note’s excerpt' : 'Hide this note’s excerpt'}
      aria-expanded={!collapsed}
      onclick={onToggle}
    >
      {#if collapsed}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
    </button>

    <button
      type="button"
      class="hg-title"
      data-hit-row
      use:tooltip={hit.id}
      onclick={onSelect}
      ondblclick={() => onOpen?.()}
      onkeydown={onHeaderKey}
    >
      <span class="hg-icon"><FileText size={12} /></span>
      <span class="hg-name">
        {#each titleParts as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
      </span>
      {#if folder}<span class="hg-folder">{folder}</span>{/if}
      <span class="hg-grow"></span>
      {#if marks > 1}
        <span
          class="hg-pill"
          use:tooltip={'Query terms highlighted in this excerpt'}
        >{marks}</span>
      {/if}
    </button>
  </div>

  {#if !collapsed && excerpt}
    <button type="button" class="hg-excerpt" onclick={onSelect} ondblclick={() => onOpen?.()}>
      {#each excerpt as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
    </button>
  {/if}
</section>

<style>
  .hg { border-bottom: 1px solid var(--border-subtle); }

  .hg-head {
    display: flex;
    align-items: stretch;
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--bg-elevated);
  }
  .hg.active .hg-head { background: color-mix(in srgb, var(--accent) 16%, var(--bg-elevated)); }

  .hg-fold {
    display: flex;
    align-items: center;
    padding: 0 4px 0 8px;
    border: none;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
  }
  .hg-fold:hover { color: var(--text-primary); }

  .hg-title {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 10px 0 0;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    font-family: inherit;
  }
  .hg-title:hover { background: var(--bg-hover); }

  .hg-icon { display: flex; color: var(--text-muted); flex-shrink: 0; }

  .hg-name {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 1;
  }
  .hg-folder {
    font-size: var(--font-size-2xs);
    font-family: var(--font-code);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .hg-grow { flex: 1; }

  .hg-pill {
    flex-shrink: 0;
    padding: 0 5px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-secondary);
    font-size: var(--font-size-2xs);
    line-height: 15px;
  }

  /* The excerpt is monospace: it is a slice of a file, and a doubled space or a
     stray tab in it is information about the note. */
  .hg-excerpt {
    display: block;
    width: 100%;
    padding: 3px 12px 5px 26px;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    line-height: 1.6;
    color: var(--text-secondary);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .hg-excerpt:hover { background: var(--bg-hover); }

  mark {
    padding: 0 1px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--warning) 32%, transparent);
    color: var(--text-primary);
  }
</style>
