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
    /** Rendered in small caps. Two or three words — it names the group, it does not explain it. */
    label: string;
    /** One line under the label, for a group whose purpose is not obvious from its name. */
    hint?: string;
    /** Right-aligned content in the label row: a count, a small action, a filter. */
    aside?: Snippet;
    /** Drop the top margin — for the first section of a form, where it would be dead space. */
    first?: boolean;
    children: Snippet;
  }

  let { label, hint, aside, first = false, children }: Props = $props();
</script>

<section class="fs" class:first>
  <div class="fs-head">
    <span class="fs-label">{label}</span>
    <span class="fs-rule" aria-hidden="true"></span>
    {#if aside}<span class="fs-aside">{@render aside()}</span>{/if}
  </div>
  {#if hint}<p class="fs-hint">{hint}</p>{/if}
  <div class="fs-body">{@render children()}</div>
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
</style>
