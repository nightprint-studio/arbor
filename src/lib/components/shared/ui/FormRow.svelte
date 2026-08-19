<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    description?: string;
    /**
     * Give the control the row's remaining width instead of sizing it to its
     * own content.
     *
     * Off by default, because the common control is a toggle or a short select
     * and stretching those would leave a switch marooned at the end of a long
     * empty strip. Set it for anything that holds *text the user has to read* —
     * a picker over a schema's tables, a select whose options are sentences. The
     * default costs nothing until the content happens to be short or empty, at
     * which point the control collapses to the width of its own chevron.
     */
    wideControl?: boolean;
    /**
     * A small glyph for the row, rendered in a tinted square at its left edge.
     *
     * It also switches the row to the **compact** layout: icon · label over
     * description · control, all on one line. The two go together rather than being
     * separate options, because the icon is what makes the compact layout readable —
     * it anchors the left column so the description no longer needs the row's full
     * width to be found. A settings list scans far faster this way; a bare form
     * column, with nothing to anchor it, still wants the default.
     */
    icon?: Snippet;
    /** Right-aligned control (toggle, input, select, button…). */
    children: Snippet;
  }

  let { label, description, wideControl = false, icon, children }: Props = $props();
</script>

{#if icon}
  <!-- Compact: the icon carries the left edge, so label and description stack beside
       it and the control keeps the right. -->
  <div class="fr-row fr-compact">
    <span class="fr-icon">{@render icon()}</span>
    <div class="fr-text">
      <span class="fr-title">{label}</span>
      {#if description}<span class="fr-hint">{description}</span>{/if}
    </div>
    <div class="fr-control" class:fr-wide={wideControl}>
      {@render children()}
    </div>
  </div>
{:else}
<div class="fr-row">
  <!-- Header: label on the left, control on the right.
       Keeping the label compact (no description here) prevents narrow controls
       like a Toggle from being stuck in a half-row "second column" while the
       description tries to wrap into a tiny strip on the left. -->
  <div class="fr-header" class:fr-wide-header={wideControl}>
    <span class="fr-title">{label}</span>
    <div class="fr-control" class:fr-wide={wideControl}>
      {@render children()}
    </div>
  </div>

  {#if description}
    <p class="fr-desc">{description}</p>
  {/if}
</div>
{/if}

<style>
  /* `fr-` prefixed class names so SettingsPanel's `.content :global(.row-title)`
     overrides (intended for legacy direct usage in CacheSection /
     RecoverySection) don't clobber FormRow's typography.  Mirrors the
     `.feat-row` styling from ExperimentalSection: same padding, title size,
     description tone — every settings row reads at the same visual weight. */
  .fr-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .fr-row:last-child { border-bottom: none; }

  .fr-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-width: 0;
  }

  .fr-title {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--text-primary);
    flex: 1;
    min-width: 0;
  }

  .fr-control {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  /* Opt-in: the control takes what the title leaves. `min-width: 0` lets it
     shrink past its content instead of pushing the title out of the row. */
  .fr-control.fr-wide { flex: 1 1 auto; min-width: 0; }
  /* …and the title stops competing for it: without this the two split the row
     evenly and a one-word label reserves half of it. */
  .fr-header.fr-wide-header .fr-title { flex: 0 1 auto; }

  .fr-desc {
    margin: 0;
    font-size: 0.77rem;
    color: var(--text-secondary);
    line-height: 1.55;
  }

  /* ── Compact (icon) variant ─────────────────────────────────────────────── */
  .fr-compact {
    flex-direction: row;
    align-items: center;
    gap: 11px;
    padding: 10px 14px;
    min-height: 46px;
  }
  .fr-icon {
    display: flex; align-items: center; justify-content: center;
    width: 28px; height: 28px; flex-shrink: 0;
    border-radius: 8px;
    color: var(--accent);
    background: var(--accent-subtle);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .fr-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  /* The title is a flex child of `.fr-text` here, not of the header row. */
  .fr-compact .fr-title { flex: none; }
  .fr-hint {
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
</style>
