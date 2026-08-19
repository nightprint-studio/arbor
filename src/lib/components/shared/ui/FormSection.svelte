<script lang="ts">
  /**
   * FormSection — a labelled band inside a dense form.
   *
   * The unit a long dialog is read in. A form with fifteen controls and no bands is a wall you
   * scan linearly; the same form in four labelled groups is four decisions, and you can skip the
   * ones you do not care about. That is the whole job.
   *
   * Distinct from `SectionHeader`, which titles a *page* — large type, generous margin, meant to
   * be the biggest thing on screen. This is the opposite: a small caps label and a hairline that
   * runs to the edge, deliberately quiet, because in a form the controls are the content and the
   * label is only there to group them.
   *
   * The rule matters as much as the label. Without it the label reads as one more field name; with
   * it, it reads as a boundary, which is what makes the group a group.
   */
  import type { Snippet } from 'svelte';

  interface Props {
    /**
     * Rendered in small caps. Two or three words — it names the group, it does not
     * explain it. Omit it for a group that already sits under a heading of its own:
     * the band is still the unit, it just doesn't need naming twice.
     */
    label?: string;
    /** One line under the label, for a group whose purpose is not obvious from its name. */
    hint?: string;
    /** Right-aligned content in the label row: a count, a small action, a filter. */
    aside?: Snippet;
    /** Drop the top margin — for the first section of a form, where it would be dead space. */
    first?: boolean;
    /**
     * Draw the body as a single bordered card with its rows separated by hairlines
     * instead of by gaps.
     *
     * For a list of **settings**, where each row is one decision of the same kind: the
     * enclosure is what says "these belong together", and it makes a `FormRow`'s own
     * bottom border read as a divider rather than as a stray underline. A form of
     * unlike fields still wants the default — boxing those groups things that only
     * happen to be adjacent.
     */
    boxed?: boolean;
    children: Snippet;
  }

  let { label, hint, aside, first = false, boxed = false, children }: Props = $props();
</script>

<section class="fs" class:first class:headless={!label && !aside}>
  {#if label || aside}
    <div class="fs-head">
      {#if label}<span class="fs-label">{label}</span>{/if}
      <span class="fs-rule" aria-hidden="true"></span>
      {#if aside}<span class="fs-aside">{@render aside()}</span>{/if}
    </div>
  {/if}
  {#if hint}<p class="fs-hint">{hint}</p>{/if}
  <div class="fs-body" class:boxed>{@render children()}</div>
</section>

<style>
  .fs { display: flex; flex-direction: column; margin-top: 18px; }
  .fs.first { margin-top: 0; }

  .fs-head { display: flex; align-items: center; gap: 10px; }
  .fs-label {
    flex-shrink: 0;
    font-size: var(--font-size-3xs);
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--text-disabled);
  }
  /* The rule is what turns the label into a boundary instead of another field name. */
  .fs-rule { flex: 1; height: 1px; background: var(--border-subtle); }
  .fs-aside { flex-shrink: 0; display: flex; align-items: center; gap: 6px; }

  .fs-hint {
    margin: 4px 0 0;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    line-height: 1.45;
  }

  .fs-body { display: flex; flex-direction: column; gap: 8px; padding-top: 10px; }
  /* Boxed: one card, hairline-separated rows. The gap goes — two separators for the
     same boundary read as a gap that failed to close. */
  /* No head to sit under — the margin was spacing it away from a label that isn't
     there, which is just a gap at the top of the group. */
  .fs.headless .fs-body.boxed { margin-top: 0; }
  .fs-body.boxed {
    gap: 0;
    margin-top: 10px;
    padding-top: 0;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
</style>
