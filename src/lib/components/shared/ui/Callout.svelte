<script lang="ts">
  /**
   * Callout — in-document highlighted block.
   *
   *   <Callout variant="tip" title="Quick Start">
   *     Press <Kbd action="open_repo" /> and pick a folder.
   *   </Callout>
   *
   * Designed for documentation pages, onboarding-style hints, and inline
   * "by-the-way" notes embedded in body copy. Visually tinted, with a
   * coloured leading bar and bold title.
   *
   * Distinct from `Alert` (transient app messages — save/error/loading)
   * because the use case is different: Callouts live inside long-form
   * prose, NEVER dismiss themselves, and read more like a typography
   * element than a UI banner. Don't merge the two — the day we want a
   * new Alert variant (toast-style, action-row, icon swap) we don't
   * want it to also touch every documentation page.
   *
   * Variants follow the documentation conventions:
   *   - tip     — green wash, neutral helpful note
   *   - info    — accent (brand) wash, factual context
   *   - warning — amber wash, caveat or sharp edge
   *   - danger  — red wash, destructive or breaking change
   */
  import type { Snippet } from 'svelte';

  type Variant = 'tip' | 'info' | 'warning' | 'danger';

  interface Props {
    variant?: Variant;
    /** Bold heading rendered above the body. Optional. */
    title?: string;
    children: Snippet;
  }

  let { variant = 'info', title, children }: Props = $props();
</script>

<aside class="callout v-{variant}" role="note">
  {#if title}<strong class="callout-title">{title}</strong>{/if}
  <div class="callout-body">{@render children()}</div>
</aside>

<style>
  .callout {
    --callout-color: var(--accent);
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 14px;
    border: 1px solid color-mix(in srgb, var(--callout-color) 45%, transparent);
    border-left: 3px solid var(--callout-color);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--callout-color) 8%, transparent);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    color: var(--text-primary);
  }
  .callout-title {
    color: var(--callout-color);
    font-weight: 600;
    font-size: var(--font-size-sm);
    letter-spacing: 0.1px;
  }
  .callout-body {
    color: var(--text-secondary);
  }
  /* Inline children (Kbd, code, links) stay legible on the wash. */
  .callout-body :global(code) {
    font-family: var(--font-code);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--border-subtle);
    color: var(--text-primary);
    font-size: 0.9em;
  }

  .v-tip     { --callout-color: var(--success); }
  .v-info    { --callout-color: var(--accent);  }
  .v-warning { --callout-color: var(--warning); }
  .v-danger  { --callout-color: var(--error);   }
</style>
