<!--
  SidebarNodeLayout — structural / leaf-content node types for
  PluginSidebarPanel:
    label, heading, divider, text_display, paragraph, button, row,
    section, container, and the `unknown` fallback.

  `section` / `row` / `container` recurse through the dispatcher-provided
  `renderNode` snippet so any child node type works at arbitrary depth.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronRight, ChevronDown } from 'lucide-svelte';
  import PluginIcon from '../PluginIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { SidebarNodeCtx } from './ctx';

  interface Props {
    node:       any;
    index:      number;
    ctx:        SidebarNodeCtx;
    renderNode: Snippet<[any, number]>;
  }
  let { node: n, index: i, ctx, renderNode }: Props = $props();
</script>

{#if n.type === 'label'}
  <div class="node-label">{n.text ?? n.value ?? ''}</div>

{:else if n.type === 'heading'}
  <div class="node-heading">{n.text ?? ''}</div>

{:else if n.type === 'divider'}
  <hr class="node-divider" />

{:else if n.type === 'text_display' || n.type === 'paragraph'}
  <p class="node-para">{n.text ?? ''}</p>

{:else if n.type === 'button'}
  <!-- Variants:
       · `icon_only` — renders only the icon, no label.
       · `variant = 'ghost'` — no background at rest; bg on hover.
       · `disabled` — greyed out. -->
  <button
    class="node-button"
    class:icon-only={n.icon_only}
    class:ghost={n.variant === 'ghost'}
    class:primary={n.variant === 'primary'}
    class:danger={n.variant === 'danger'}
    disabled={n.disabled}
    use:tooltip={n.tooltip ?? (n.icon_only ? (n.label ?? n.text ?? '') : '')}
    onclick={() => !n.disabled && ctx.fireAction(n.action, { id: n.id })}
  >
    {#if n.icon}
      <PluginIcon name={n.icon} size={n.icon_only ? 14 : 13} class="node-icon" />
    {/if}
    {#if !n.icon_only}
      <span>{n.label ?? n.text ?? 'Run'}</span>
    {/if}
  </button>

{:else if n.type === 'row' && Array.isArray(n.children)}
  <div class="node-row" style="gap:{n.gap ?? 4}px;">
    {#each n.children as child, k (ctx.nodeKey(child, k))}
      {@render renderNode(child, k)}
    {/each}
  </div>

{:else if (n.type === 'section' || n.type === 'container') && Array.isArray(n.nodes)}
  <!-- Sections render their children through the same snippet so any
       node type is supported at arbitrary depth. Optional `collapsible`
       makes the title bar a toggle and hides the body when collapsed. -->
  {@const sectionKey = (n.id ?? '') + ':' + ctx.nodeKey(n, i)}
  {@const sectionCollapsible = !!n.collapsible}
  {@const sectionCollapsed = sectionCollapsible && ctx.isSectionCollapsed(n, sectionKey)}
  <div class="node-section" class:collapsible={sectionCollapsible}>
    {#if n.title}
      {#if sectionCollapsible}
        <button
          type="button"
          class="node-section-title node-section-toggle"
          onclick={() => ctx.toggleSection(sectionKey, sectionCollapsed)}
          aria-expanded={!sectionCollapsed}
        >
          {#if sectionCollapsed}
            <ChevronRight size={12} class="section-chevron" />
          {:else}
            <ChevronDown size={12} class="section-chevron" />
          {/if}
          <span class="section-title-text">{n.title}</span>
          {#if n.badge !== undefined && n.badge !== null && n.badge !== ''}
            <span class="card-item-badge section-badge">{n.badge}</span>
          {/if}
        </button>
      {:else}
        <div class="node-section-title">{n.title}</div>
      {/if}
    {/if}
    {#if !sectionCollapsed}
      <div class="node-section-body">
        {#each n.nodes as child, k (ctx.nodeKey(child, k))}
          {@render renderNode(child, k)}
        {/each}
      </div>
    {/if}
  </div>

{:else}
  <div class="node-unknown">[{n.type ?? 'unknown node'}]</div>
{/if}
