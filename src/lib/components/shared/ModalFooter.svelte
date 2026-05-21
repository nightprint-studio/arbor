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
   */
  import type { Snippet } from 'svelte';

  type Align = 'end' | 'start' | 'center' | 'between';

  let {
    align    = 'end',
    children,
  }: {
    align?:   Align;
    children: Snippet;
  } = $props();
</script>

<div class="footer-row" data-align={align}>
  {@render children()}
</div>

<style>
  .footer-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
  }
  .footer-row[data-align="end"]     { justify-content: flex-end; }
  .footer-row[data-align="start"]   { justify-content: flex-start; }
  .footer-row[data-align="center"]  { justify-content: center; }
  .footer-row[data-align="between"] { justify-content: space-between; }
</style>
