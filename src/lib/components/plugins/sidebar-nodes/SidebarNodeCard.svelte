<!--
  SidebarNodeCard — `card_item` (MR/Reflog-style rows) + `list`.
  card_item renders state-icon + title-badge + subtitle + meta-chips +
  per-row hover actions; list renders a flat clickable list. Neither
  node recurses into child plugin-nodes — they own a fixed layout.
-->
<script lang="ts">
  import PluginIcon from '../PluginIcon.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { SidebarNodeCtx } from './ctx';

  interface Props {
    node: any;
    ctx:  SidebarNodeCtx;
  }
  let { node: n, ctx }: Props = $props();
</script>

{#if n.type === 'list' && Array.isArray(n.items)}
  <ul class="node-list">
    {#each n.items as item, j (`${item.id ?? item.value ?? item.label ?? 'item'}:${j}`)}
      {@const hasAction = !!(item.action ?? n.item_action)}
      <li class="node-list-item" class:clickable={hasAction}>
        {#if hasAction}
          <button
            type="button"
            class="node-list-item-btn"
            onclick={() => ctx.fireAction(item.action ?? n.item_action, {
              id: item.id, value: item.value, label: item.label,
            })}
          >
            {#if item.icon}
              <PluginIcon name={item.icon} size={14} class="node-icon" />
            {/if}
            <span class="node-list-label">{item.label ?? item.value ?? '…'}</span>
            {#if item.detail}
              <span class="node-list-detail">{item.detail}</span>
            {/if}
          </button>
        {:else}
          {#if item.icon}
            <PluginIcon name={item.icon} size={14} class="node-icon" />
          {/if}
          <span class="node-list-label">{item.label ?? item.value ?? '…'}</span>
          {#if item.detail}
            <span class="node-list-detail">{item.detail}</span>
          {/if}
        {/if}
      </li>
    {/each}
  </ul>

{:else if n.type === 'card_item'}
  {@const ci = n as any}
  {#snippet cardBody()}
    {#if ci.icon || ci.icon_spin}
      <span class="card-item-state" class:accent={ci.icon_variant === 'accent'}
                                    class:success={ci.icon_variant === 'success'}
                                    class:warning={ci.icon_variant === 'warning'}
                                    class:danger={ci.icon_variant === 'danger'}>
        {#if ci.icon_spin}
          <Spinner size="sm" variant="spin" ariaLabel="Running" />
        {:else}
          <PluginIcon name={ci.icon} size={14} class="node-icon" />
        {/if}
      </span>
    {/if}
    <div class="card-item-body">
      <div class="card-item-top">
        <span class="card-item-title">{ci.title ?? ''}</span>
        {#if ci.badge !== undefined && ci.badge !== null && ci.badge !== ''}
          <span class="card-item-badge">{ci.badge}</span>
        {/if}
      </div>
      {#if ci.subtitle}
        <div class="card-item-sub">{ci.subtitle}</div>
      {/if}
      {#if Array.isArray(ci.meta) && ci.meta.length > 0}
        <div class="card-item-meta">
          {#each ci.meta as m, k (`${m.text}:${k}`)}
            <span class="meta-chip meta-chip-{m.variant ?? 'muted'}">{m.text}</span>
          {/each}
        </div>
      {/if}
    </div>
    {#if Array.isArray(ci.actions) && ci.actions.length > 0}
      <div class="card-item-actions">
        {#each ci.actions as a, k (`${a.action}:${k}`)}
          <button
            type="button"
            class="card-item-action"
            class:danger={a.variant === 'danger'}
            class:accent={a.variant === 'accent'}
            class:always-visible={!!a.always_visible}
            use:tooltip={a.tooltip ?? a.label ?? ''}
            disabled={!!a.disabled}
            onclick={(e) => {
              e.stopPropagation();
              if (!a.disabled) ctx.fireAction(a.action, { id: ci.id, ...(a.extra ?? {}) });
            }}
          >
            {#if a.icon}<PluginIcon name={a.icon} size={13} class="node-icon" />{/if}
          </button>
        {/each}
      </div>
    {/if}
  {/snippet}
  {#if ci.action}
    <div
      class="card-item clickable"
      role="button"
      tabindex="0"
      onclick={() => ctx.fireAction(ci.action, { id: ci.id })}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); ctx.fireAction(ci.action, { id: ci.id }); } }}
      use:tooltip={ci.tooltip ?? ''}
    >
      {@render cardBody()}
    </div>
  {:else}
    <div class="card-item" use:tooltip={ci.tooltip ?? ''}>
      {@render cardBody()}
    </div>
  {/if}
{/if}
