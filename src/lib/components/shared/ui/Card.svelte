<script lang="ts">
  import type { Snippet } from 'svelte';

  type Variant = 'elevated' | 'flat' | 'subtle';
  type Padding = 'none' | 'sm' | 'md' | 'lg';

  interface Props {
    /** Background tone — 'elevated' (default, bg-elevated), 'flat' (bg-base), 'subtle' (bg-overlay). */
    variant?: Variant;
    /** Inner padding. Default 'md'. */
    padding?: Padding;
    /** Border style — true = standard 1px, false = no border. Default true. */
    bordered?: boolean;
    /** Add hover affordance (border + bg shift). */
    hoverable?: boolean;
    /** Optional title. Use `header` snippet for richer headers. */
    title?: string;
    /** Optional title snippet — overrides `title`. */
    header?: Snippet;
    /** Right-aligned action cluster in the header. */
    actions?: Snippet;
    /** Footer snippet. */
    footer?: Snippet;
    /** Extra class on the root. */
    class?: string;
    children: Snippet;
  }

  let {
    variant   = 'elevated',
    padding   = 'md',
    bordered  = true,
    hoverable = false,
    title,
    header,
    actions,
    footer,
    class: rootClass = '',
    children,
  }: Props = $props();

  const showHeader = $derived(!!header || !!title || !!actions);
</script>

<div
  class="card v-{variant} {rootClass}"
  class:bordered
  class:hoverable
>
  {#if showHeader}
    <div class="card-header">
      <div class="card-title">
        {#if header}{@render header()}{:else if title}{title}{/if}
      </div>
      {#if actions}<div class="card-actions">{@render actions()}</div>{/if}
    </div>
  {/if}

  <div class="card-body p-{padding}">
    {@render children()}
  </div>

  {#if footer}
    <div class="card-footer">{@render footer()}</div>
  {/if}
</div>

<style>
  .card {
    border-radius: var(--radius-md);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .card.bordered { border: 1px solid var(--border); }
  .card.v-elevated { background: var(--bg-elevated); }
  .card.v-flat     { background: var(--bg-base); }
  .card.v-subtle   { background: var(--bg-overlay); }

  .card.hoverable {
    transition: border-color var(--transition-fast), background var(--transition-fast),
                box-shadow var(--transition-fast);
    cursor: pointer;
  }
  .card.hoverable:hover {
    border-color: var(--accent);
    background: var(--bg-hover);
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-subtle);
    background: rgba(255,255,255,0.015);
  }
  .card-title  { flex: 1; font-weight: 600; font-size: var(--font-size-sm); color: var(--text-primary); min-width: 0; }
  .card-actions { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; }

  .card-body { flex: 1; min-height: 0; }
  .p-none { padding: 0; }
  .p-sm   { padding: 8px;  }
  .p-md   { padding: 12px; }
  .p-lg   { padding: 18px; }

  .card-footer {
    padding: 8px 12px;
    border-top: 1px solid var(--border-subtle);
    background: rgba(255,255,255,0.015);
  }
</style>
