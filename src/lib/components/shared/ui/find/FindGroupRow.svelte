<script lang="ts">
  /** The header row of one group in a find list: icon, name, trail, badge and count. */
  import type { Snippet } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { FindGroup } from './find-types';

  interface Props {
    group: FindGroup;
    /** Leading icon; the host knows what kind of thing the group is. */
    lead?: Snippet<[FindGroup]>;
  }

  let { group, lead }: Props = $props();
</script>

<div class="fg" class:ext={group.external} use:tooltip={group.tooltip ?? group.label}>
  {#if lead}{@render lead(group)}{/if}
  <span class="fg-name">{group.label}</span>
  {#if group.detail}<span class="fg-detail">{group.detail}</span>{/if}
  {#if group.badge}<span class="fg-badge">{group.badge}</span>{/if}
  {#if group.count !== undefined}<span class="fg-count">{group.count}</span>{/if}
</div>

<style>
  /* Laid out to the same height as a hit row (the list's `rowHeight`), which is what lets one
     windowed list hold a grouped result. */
  .fg {
    display: flex; align-items: center; gap: 6px; height: 100%;
    padding: 0 14px; color: var(--text-secondary);
    font-size: var(--font-size-xs); font-weight: 600;
  }
  .fg :global(svg) { align-self: center; color: var(--text-muted); flex-shrink: 0; }
  .fg-name { flex-shrink: 0; max-width: 60%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fg-detail {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: var(--font-size-3xs); font-weight: 400;
    color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  .fg-count {
    margin-left: auto; font-size: var(--font-size-3xs); font-weight: 500;
    color: var(--text-disabled);
    background: var(--bg-elevated); border-radius: 99px; padding: 0 6px;
  }
  /* `margin-left: auto` on the count would push the badge to the left edge if it came after it,
     so the badge sits before it and both stay right-aligned. */
  .fg-badge {
    flex-shrink: 0; max-width: 200px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    margin-left: auto;
    font-size: var(--font-size-3xs); font-weight: 400;
    color: var(--text-muted);
    background: var(--bg-elevated); border-radius: var(--radius-sm); padding: 0 5px;
  }
  .fg-badge + .fg-count { margin-left: 0; }

  .fg.ext {
    background: color-mix(in srgb, var(--warning) 7%, transparent);
    box-shadow: inset 2px 0 0 color-mix(in srgb, var(--warning) 45%, transparent);
  }
</style>
