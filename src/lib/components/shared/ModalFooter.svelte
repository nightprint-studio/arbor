<script lang="ts">
  /**
   * ModalFooter — standard footer content for <Modal>.
   *
   * Drop into Modal's `footer` snippet to override the default right-aligned
   * button row. Modal already provides the chrome (background, padding,
   * border) — this helper only controls the inner layout (alignment).
   *
   *   <Modal {onClose}>
   *     ...body...
   *     {#snippet footer()}
   *       <ModalFooter align="between">
   *         <button class="btn-ghost">Help</button>
   *         <div style="display:flex; gap:8px">
   *           <button class="btn-ghost" onclick={onClose}>Cancel</button>
   *           <button class="btn-primary" onclick={save}>Save</button>
   *         </div>
   *       </ModalFooter>
   *     {/snippet}
   *   </Modal>
   *
   * For the most common case — a tight right-aligned button cluster — the
   * footer snippet can just contain the buttons directly, since Modal's
   * `.modal-footer` chrome already does `justify-content: flex-end`.
   * ModalFooter is mainly for when you need a different alignment.
   *
   * `hint` (optional) shows a muted "Ctrl+Enter <hint>" cue at the left of the
   * row, so the keyboard submit is discoverable. It is purely a label: the
   * caller owns the actual Ctrl/Cmd+Enter handler (and must gate it on the same
   * condition as the primary button). The hint never takes room from the
   * buttons — it truncates first and disappears when the footer is narrow.
   */
  import type { Snippet } from 'svelte';
  import Kbd from './internal/Kbd.svelte';

  type Align = 'end' | 'start' | 'center' | 'between';

  let {
    align    = 'end',
    hint,
    children,
  }: {
    align?:   Align;
    /** Text after the Ctrl/Cmd+Enter keys, e.g. "to create". Omit for no hint. */
    hint?:    string;
    children: Snippet;
  } = $props();
</script>

<div class="footer-row" data-align={align}>
  {#if hint}
    <span class="footer-hint">
      <Kbd keys={['Ctrl', 'Enter']} size="sm" />
      <span class="footer-hint-text">{hint}</span>
    </span>
  {/if}
  {@render children()}
</div>

<style>
  .footer-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    container-type: inline-size;
  }
  /* Takes only leftover space (min-width 0) so the buttons never shrink for it. */
  .footer-hint {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
    /* --text-secondary, not --text-muted: on the footer's --bg-elevated the muted tone sits
       below 4:1 and the keys' own border disappears into the background. */
    color: var(--text-secondary);
    font-size: var(--font-size-xs);
    white-space: nowrap;
  }
  .footer-hint-text { overflow: hidden; text-overflow: ellipsis; }
  @container (max-width: 380px) { .footer-hint { display: none; } }
  .footer-row[data-align="end"]     { justify-content: flex-end; }
  .footer-row[data-align="start"]   { justify-content: flex-start; }
  .footer-row[data-align="center"]  { justify-content: center; }
  .footer-row[data-align="between"] { justify-content: space-between; }
</style>
