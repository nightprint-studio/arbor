<!--
  FormNodeButtons — `button`, `menu_button`, `suggest_grid`.

  Note: the menu dropdown body (the floating menu portal) is rendered by
  the dispatcher itself so it can `position: fixed` outside the form
  layout, escape any clipped parent, and reach the live `menuAnchor`
  state. This component only renders the trigger button.
-->
<script lang="ts">
  import { Loader, ChevronDown, Plus } from 'lucide-svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import { tooltip } from '$lib/actions/tooltip';

  import type { FormNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
  }
  let { node, ctx }: Props = $props();
</script>

{#if node.type === 'button'}
  {@const n = node as any}
  {@const isLoading = ctx.actionPending === n.action}
  {@const BIcon = n.icon ? PLUGIN_ICONS[n.icon] : null}
  <button
    class="pf-action-btn pf-action-{n.variant ?? 'default'} {n.icon_only ? 'pf-action-icon-only' : ''} {(node as any).class ?? ''}"
    style={(node as any).style}
    type="button"
    use:tooltip={n.tooltip ?? (n.icon_only ? (n.label ?? '') : '')}
    disabled={!!(n.disabled) || !!ctx.actionPending}
    onclick={() => ctx.handleButtonAction(n.action, n.close_after ?? false, n.extra)}
  >
    {#if isLoading}
      <span class="pf-btn-spin"><Loader size={12} /></span>
    {:else if BIcon}
      <BIcon size={12} />
    {/if}
    {#if !n.icon_only}{n.label ?? ''}{/if}
  </button>

{:else if node.type === 'menu_button'}
  {@const n = node as any}
  {@const MIcon = n.icon ? PLUGIN_ICONS[n.icon] : null}
  {@const isOpen = ctx.isMenuOpen(n.id)}
  {@const showChevron = n.show_chevron === true
    || (n.show_chevron !== false && !n.icon_only)}
  <button
    class="pf-action-btn pf-action-{n.variant ?? 'default'} {n.icon_only ? 'pf-action-icon-only' : ''} {(node as any).class ?? ''}"
    class:pf-menu-btn-open={isOpen}
    style={(node as any).style}
    type="button"
    use:tooltip={n.tooltip ?? (n.icon_only ? (n.label ?? '') : '')}
    disabled={!!n.disabled}
    onclick={(e) => (isOpen ? ctx.closeMenu() : ctx.openMenu(e, n.id!))}
  >
    {#if MIcon}<MIcon size={12} />{/if}
    {#if !n.icon_only && n.label}<span>{n.label}</span>{/if}
    {#if showChevron}<ChevronDown size={10} class="pf-menu-btn-chev" />{/if}
  </button>

{:else if node.type === 'suggest_grid'}
  {@const n = node as any}
  <div class="pf-suggest-grid {(node as any).class ?? ''}" style={(node as any).style}>
    {#each n.items ?? [] as item}
      <div class="pf-suggest-item">
        <div class="pf-suggest-name">{item.name}</div>
        {#if item.cmd}<div class="pf-suggest-cmd">{item.cmd}</div>{/if}
        {#if item.action}
          <button class="pf-suggest-add" type="button"
            onclick={() => ctx.handleButtonAction(item.action, false, { name: item.name, cmd: item.cmd ?? '' })}>
            <Plus size={10} /> Add configuration
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/if}
