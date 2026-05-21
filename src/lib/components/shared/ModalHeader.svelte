<script lang="ts">
  /**
   * ModalHeader — standard header content for <Modal>.
   *
   * The whole area to the LEFT of the close button is free-form: pass it via
   * `children`. For the common "single-line title" case there's a `title`
   * shorthand that auto-renders the standardised typography — equivalent to
   * `<span class="modal-title">{title}</span>`.
   *
   *   <!-- shorthand -->
   *   <ModalHeader title="Create Branch" {onClose} />
   *
   *   <!-- free-form (icon + title + status pill, etc.) -->
   *   <ModalHeader {onClose}>
   *     <GitBranch size={14} />
   *     <span class="modal-title">Branch comparison</span>
   *     <span class="badge">{count} ahead</span>
   *   </ModalHeader>
   *
   * `actions` is an optional snippet placed between the content and the close
   * button — typically a refresh / settings cluster.
   */
  import type { Snippet } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    title,
    onClose,
    children,
    actions,
    hideClose = false,
  }: {
    /** Shorthand for `<span class="modal-title">{title}</span>` — used when
     *  `children` is not provided. */
    title?:    string;
    onClose:   () => void;
    /** Free-form content for the entire left side of the header. */
    children?: Snippet;
    /** Snippet rendered between the content and the close button. */
    actions?:  Snippet;
    /** Suppress the trailing close button. Use when the host modal is
     *  intentionally non-dismissable (e.g. a blocking bouncer) — otherwise
     *  the button is visible but inert and confuses users. */
    hideClose?: boolean;
  } = $props();
</script>

<div class="content">
  {#if children}
    {@render children()}
  {:else if title}
    <span class="modal-title">{title}</span>
  {/if}
</div>
{#if actions}
  <span class="actions">{@render actions()}</span>
{/if}
{#if !hideClose}
  <button class="mac-close-btn" onclick={onClose} aria-label="Close" use:tooltip={'Close'}></button>
{/if}

<style>
  .content {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  /* Standard title typography — exposed as `.modal-title` (global-ish via :global)
     so consumers writing free-form headers can opt in with the same styling. */
  .content :global(.modal-title) {
    font-size: var(--font-size-md);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
