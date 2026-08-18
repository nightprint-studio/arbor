<script lang="ts">
  /**
   * Which language you are editing — as a switch, not as a list of links.
   *
   * ## What it is for
   *
   * Translating is a loop between two files: read the Italian, write the English, check the Italian
   * again. Doing that through the project tree means four clicks and losing your place each time,
   * and doing it through a list of "other languages" means the list has to be on screen and only
   * offers the languages already done.
   *
   * So this is a picker over **every declared language**, and the two states it distinguishes are the
   * two that matter:
   *
   * - a language that declares the label — go there, on the value;
   * - a language that does not — go to the file it *would* be in, which is how the next translation
   *   gets started. The file may not exist yet; the row says so rather than being absent, because
   *   "German is missing" is exactly the fact you came here to act on.
   *
   * A **disabled** language stays in the list, marked. It is declared, so it is somewhere a
   * translation can legitimately go; it is simply owed nothing, which is why it is not counted as
   * missing anywhere else in the panel.
   *
   * ## What it deliberately does not do
   *
   * It does not create the entry. Writing a key into another language's file is a real edit to a file
   * you are not looking at, and the honest version of it — open the file, with the key ready to type —
   * is what happens instead.
   */
  import { Check, ChevronDown, Languages, Plus } from 'lucide-svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { Sibling, StudioView } from '$lib/ipc/bennu/i18n';

  let {
    view,
    /** Open another language's file, at the value when it has one. */
    onGo,
  }: {
    view: StudioView;
    onGo: (sibling: Sibling) => void;
  } = $props();

  /** Translated first: the common move is comparing against a language that has the text. */
  const ordered: Sibling[] = $derived(
    [...view.siblings].sort((a, b) => Number(b.declares) - Number(a.declares)),
  );

  const done = $derived(ordered.filter((s) => s.declares).length);
</script>

<Dropdown
  searchable={view.siblings.length > 6}
  searchPlaceholder="Language…"
  position="fixed"
  direction="down"
  width="280px"
  maxHeight={320}
>
  {#snippet trigger({ open, toggle })}
    <button
      class="lp-trigger"
      class:active={open}
      type="button"
      use:tooltip={view.siblings.length
        ? `Editing ${view.lang} — ${done} of ${view.siblings.length + 1} languages have this label`
        : 'The only language this project declares'}
      aria-haspopup="menu"
      aria-expanded={open}
      disabled={view.siblings.length === 0}
      onclick={toggle}
    >
      <Languages size={12} />
      <span class="lp-code">{view.lang}</span>
      {#if view.siblings.length}<ChevronDown size={10} />{/if}
    </button>
  {/snippet}
  {#snippet children({ close })}
    <!-- The language being edited leads the list and is not a destination: it is where you are. -->
    <div class="lp-row current">
      <Check size={12} />
      <span class="lp-row-code">{view.lang}</span>
      <span class="lp-row-text">editing</span>
    </div>
    {#each ordered as s (s.lang)}
      <button
        class="lp-row"
        class:absent={!s.declares}
        type="button"
        use:tooltip={s.declares ? s.file : `Not translated yet — opens ${s.file}`}
        onclick={() => { onGo(s); close(); }}
      >
        {#if s.declares}
          <span class="lp-dot"></span>
        {:else}
          <Plus size={12} />
        {/if}
        <span class="lp-row-code">{s.lang}</span>
        <span class="lp-row-text">{s.declares ? s.text : (s.name || 'not translated')}</span>
        {#if !s.enabled}
          <span class="lp-off" use:tooltip={'Declared but switched off — nothing is owed to it'}
            >off</span
          >
        {/if}
      </button>
    {/each}
  {/snippet}
</Dropdown>

<style>
  .lp-trigger {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 5px;
    border: 1px solid var(--border-default);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    color: var(--text-secondary);
    font-size: var(--font-size-2xs);
    cursor: pointer;
  }
  .lp-trigger:hover:not(:disabled),
  .lp-trigger.active { background: var(--bg-hover); color: var(--text-primary); }
  .lp-trigger:disabled { cursor: default; opacity: 0.7; }
  .lp-code { font-family: var(--font-code); }

  .lp-row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 8px;
    border: none;
    background: none;
    color: var(--text-primary);
    font-size: var(--font-size-xs);
    text-align: left;
    cursor: pointer;
  }
  .lp-row:hover { background: var(--bg-hover); }
  .lp-row.current { color: var(--text-muted); cursor: default; }
  .lp-row.current:hover { background: none; }
  .lp-row :global(svg) { flex: none; color: var(--text-disabled); }
  .lp-row.current :global(svg) { color: var(--success); }
  /* The untranslated rows are the actionable ones, so their glyph is the one with a colour. */
  .lp-row.absent :global(svg) { color: var(--warning); }

  .lp-row-code {
    flex: none;
    min-width: 22px;
    font-family: var(--font-code);
    color: var(--text-secondary);
  }
  .lp-row-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
  }
  .lp-row.absent .lp-row-text { font-style: italic; }

  /* A dot rather than a tick: the tick means "you are here", and two similar glyphs meaning
     different things is how a list stops being readable at a glance. */
  .lp-dot {
    flex: none;
    width: 12px;
    display: inline-flex;
    justify-content: center;
  }
  .lp-dot::before {
    content: '';
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--success);
  }

  .lp-off {
    flex: none;
    padding: 0 4px;
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    color: var(--text-disabled);
    font-size: var(--font-size-3xs);
  }
</style>
