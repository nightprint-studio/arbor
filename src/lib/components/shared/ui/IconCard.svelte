<script lang="ts">
  /**
   * IconCard — generic icon + title + description block.
   *
   *   <IconCard
   *     title="Open local"
   *     description="Pick a folder you already cloned."
   *     layout="stack"
   *     interactive
   *     onclick={open}
   *   >
   *     {#snippet icon()}<FolderOpen size={26} />{/snippet}
   *     {#snippet trailing()}<Kbd action="open_repo" size="sm" />{/snippet}
   *   </IconCard>
   *
   * Covers the family of "tile / row / feature card" patterns that come
   * up in onboarding flows, settings pages, empty states, and provider
   * pickers. Three knobs encode the variations:
   *
   *   - `layout="row"`   icon on the left, text in the middle, trailing
   *                      slot pushed to the right edge. Use for list
   *                      rows and dense feature lists.
   *   - `layout="stack"` icon on top, text below, trailing absolute-
   *                      positioned in the top-right corner. Use for
   *                      square-ish picker tiles (open / clone / init).
   *   - `interactive`    renders as <button>, exposes hover affordance
   *                      and focus ring. Without it the root is a <div>
   *                      and the card reads as info-only.
   *
   * `size` tunes padding + typography between `sm` (compact list rows),
   * `md` (default), and `lg` (large hero tiles).
   *
   * The icon snippet receives no args — pick the size yourself so the
   * caller controls visual weight (Lucide icons size differently from
   * Iconify, from custom SVGs, etc.).
   */
  import type { Snippet } from 'svelte';

  type Layout = 'row' | 'stack';
  type Size   = 'sm' | 'md' | 'lg';
  type Tone   = 'default' | 'accent' | 'transparent';

  interface Props {
    title:        string;
    description?: string;
    layout?:      Layout;
    size?:        Size;
    /** Background treatment.
     *   - `default`     — neutral `--bg-overlay` (the standard tile look)
     *   - `accent`      — subtle accent wash, harmonises with accent-tinted
     *                     surfaces (onboarding, success states, …)
     *   - `transparent` — no fill, border-only — used when the card sits on
     *                     an already-busy background and the fill would
     *                     muddy it. */
    tone?:        Tone;
    interactive?: boolean;
    /** Subtle accent treatment applied to the icon background — when
     *  false the icon sits on plain bg-overlay. Default true: the
     *  accent wash is the standard look that makes icons "pop". */
    accentIcon?:  boolean;
    ariaLabel?:   string;
    onclick?:     (e: MouseEvent) => void;
    /** Leading icon — required for the card to make visual sense. */
    icon:         Snippet;
    /** Optional snippet rendered inline next to the title — useful for
     *  a Kbd hint, a small badge, or a status pill that should sit on
     *  the same baseline as the title rather than at the row's right
     *  edge. The trailing slot still applies for far-right content. */
    titleExtra?:  Snippet;
    /** Optional trailing snippet (Kbd hint, chevron, action button…). */
    trailing?:    Snippet;
    /** Optional extra body content rendered below the description. */
    extra?:       Snippet;
    /** Extra class on the root. */
    class?:       string;
  }

  let {
    title,
    description,
    layout      = 'row',
    size        = 'md',
    tone        = 'default',
    interactive = false,
    accentIcon  = true,
    ariaLabel,
    onclick,
    icon,
    titleExtra,
    trailing,
    extra,
    class: rootClass = '',
  }: Props = $props();
</script>

{#if interactive}
  <button
    type="button"
    class="icon-card l-{layout} sz-{size} t-{tone} interactive {rootClass}"
    class:accent-icon={accentIcon}
    aria-label={ariaLabel ?? title}
    onclick={onclick}
  >
    <span class="ic-icon">{@render icon()}</span>
    <span class="ic-text">
      <span class="ic-title-row">
        <strong class="ic-title">{title}</strong>
        {#if titleExtra}{@render titleExtra()}{/if}
      </span>
      {#if description}<span class="ic-desc">{description}</span>{/if}
      {#if extra}<span class="ic-extra">{@render extra()}</span>{/if}
    </span>
    {#if trailing}<span class="ic-trailing">{@render trailing()}</span>{/if}
  </button>
{:else}
  <div
    class="icon-card l-{layout} sz-{size} t-{tone} {rootClass}"
    class:accent-icon={accentIcon}
    role={ariaLabel ? 'group' : undefined}
    aria-label={ariaLabel}
  >
    <span class="ic-icon">{@render icon()}</span>
    <span class="ic-text">
      <span class="ic-title-row">
        <strong class="ic-title">{title}</strong>
        {#if titleExtra}{@render titleExtra()}{/if}
      </span>
      {#if description}<span class="ic-desc">{description}</span>{/if}
      {#if extra}<span class="ic-extra">{@render extra()}</span>{/if}
    </span>
    {#if trailing}<span class="ic-trailing">{@render trailing()}</span>{/if}
  </div>
{/if}

<style>
  .icon-card {
    position: relative;
    display: flex;
    gap: 12px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    text-align: left;
    min-width: 0;
  }

  /* ── Tone ────────────────────────────────────────────────────────────── */
  /* Background treatment. Default mirrors the long-standing neutral tile
     look; `accent` harmonises with accent-tinted surfaces (onboarding,
     success states) by using a subtle accent wash instead of `--bg-overlay`
     which can read as muddy on top of accent gradients. `transparent`
     drops the fill entirely for cases where the parent is already busy. */
  .icon-card.t-default {
    background: var(--bg-overlay);
  }
  .icon-card.t-accent {
    background: color-mix(in srgb, var(--accent) 7%, transparent);
    border-color: color-mix(in srgb, var(--accent) 28%, var(--border-subtle));
  }
  .icon-card.t-transparent {
    background: transparent;
  }

  /* ── Layouts ─────────────────────────────────────────────────────────── */
  .icon-card.l-row {
    flex-direction: row;
    align-items: center;
  }
  .icon-card.l-stack {
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }

  /* ── Sizes ───────────────────────────────────────────────────────────── */
  .icon-card.sz-sm { padding: 10px 12px;     gap: 10px; }
  .icon-card.sz-md { padding: 12px 14px;     gap: 12px; }
  .icon-card.sz-lg { padding: 18px 16px 16px; }

  .icon-card.l-stack.sz-sm { padding: 12px;        }
  .icon-card.l-stack.sz-md { padding: 14px;        }
  .icon-card.l-stack.sz-lg { padding: 18px 16px 16px; }

  /* ── Interactivity ───────────────────────────────────────────────────── */
  .icon-card.interactive {
    cursor: pointer;
    transition: border-color 120ms ease, background 120ms ease, transform 120ms ease;
    /* Strip <button> defaults so the card reads as a card, not a chrome button. */
    font: inherit;
    appearance: none;
  }
  .icon-card.interactive:hover {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }
  .icon-card.interactive.l-stack:hover {
    transform: translateY(-1px);
  }
  .icon-card.interactive:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* ── Icon ────────────────────────────────────────────────────────────── */
  .ic-icon {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
  }
  /* Accent treatment — square wash that makes icons pop on the card bg. */
  .icon-card.accent-icon .ic-icon {
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: 10px;
  }
  .sz-sm.accent-icon .ic-icon { width: 28px; height: 28px; }
  .sz-md.accent-icon .ic-icon { width: 32px; height: 32px; }
  .sz-lg.accent-icon .ic-icon { width: 40px; height: 40px; border-radius: 12px; }
  /* Stack-layout icons run bigger because they're the focal point. */
  .l-stack.sz-md.accent-icon .ic-icon { width: 36px; height: 36px; }
  .l-stack.sz-lg.accent-icon .ic-icon { width: 44px; height: 44px; }

  /* ── Text ────────────────────────────────────────────────────────────── */
  .ic-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ic-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .ic-title {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sz-lg .ic-title { font-size: var(--font-size-md); }

  .ic-desc {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .ic-extra {
    margin-top: 6px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  /* ── Trailing ────────────────────────────────────────────────────────── */
  .ic-trailing {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
  }
  /* In stack layout the trailing slot floats to the corner so it doesn't
     compete with the icon for vertical space. */
  .icon-card.l-stack .ic-trailing {
    position: absolute;
    top: 10px;
    right: 10px;
  }
</style>
