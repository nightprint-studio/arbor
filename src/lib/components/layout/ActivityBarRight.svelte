<script lang="ts">
  /**
   * Right-side ActivityBar — mirrors the left bar's visual structure but
   * shows ONLY plugin-registered entries (via `arbor.ui.add_sidebar` with
   * `side = "right"`). Built-in Arbor features live exclusively on the left.
   *
   * Entries with `position = "top"` render in the top group and open a
   * right-side panel (`uiStore.activeRightSidebar`).
   * Entries with `position = "bottom"` render in the bottom group and open
   * the unique bottom panel (`uiStore.activeBottomSection`) — clicking
   * overrides whatever bottom panel was previously open, regardless of
   * which ActivityBar fired the click.
   *
   * The component renders nothing when no plugin has registered a
   * right-side section, so the whole 38px rail stays invisible in that
   * case (the user explicitly asked for "shown only if there are visible
   * buttons").
   */
  import { uiStore } from '$lib/stores/ui.svelte';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { activityBarConfigStore } from '$lib/stores/activityBarConfig.svelte';
  import type { PluginSidebarSection } from '$lib/types/plugin';
  import { SIDEBAR_POINT, parseSidebarSection } from '$lib/contributions/sidebar';
  import PluginIcon from '../plugins/PluginIcon.svelte';
  import ActivityBar from './ActivityBar.svelte';
  // Right-side activity bar: tooltips fly out to the left away from the bar.
  import { tooltipLeft as tooltip } from '$lib/actions/tooltip';

  function sectionKey(s: PluginSidebarSection): string {
    return `plugin:${s.plugin_name}:${s.id}`;
  }

  // Only non-legacy sections land on the right; legacy ones always go left.
  // We resolve ORDERING + VISIBILITY through `activityBarConfigStore`: the
  // Customize Activity Bar modal writes both to `right_top_items` and
  // `right_bottom_items`, and this is the one place that consumes them. If
  // the user hides an icon, `isVisible` returns false and we skip rendering.
  const rightSections = $derived(
    contributionStore.forPoint(SIDEBAR_POINT)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseSidebarSection)
      .filter(s => s.side === 'right')
  );

  /** Resolve the ordered, visibility-filtered list of sections for a given
   *  position by running the registered ids through the config merger. */
  function resolveOrdered(position: 'top' | 'bottom'): PluginSidebarSection[] {
    const bySide = rightSections.filter(s => s.position === position);
    const byKey = new Map(bySide.map(s => [sectionKey(s), s]));
    const pluginIds = bySide.map(sectionKey);
    const merged = position === 'top'
      ? activityBarConfigStore.mergeRightTop(pluginIds)
      : activityBarConfigStore.mergeRightBottom(pluginIds);
    // The merger returns display items for ALL registered ids in the
    // configured order (with visibility). We drop hidden ones, then map
    // back to the original section objects.
    return merged
      .filter(i => i.visible)
      .map(i => byKey.get(i.id))
      .filter((s): s is PluginSidebarSection => !!s);
  }

  const topSections    = $derived(resolveOrdered('top'));
  const bottomSections = $derived(resolveOrdered('bottom'));

  /** The 38px rail itself should disappear when the user has hidden every
   *  plugin entry — mirrors the "no icons = no rail" rule applied for the
   *  zero-plugins case. */
  const hasVisibleEntries = $derived(topSections.length + bottomSections.length > 0);

  function onClickTop(s: PluginSidebarSection) {
    const key = `plugin:${s.plugin_name}:${s.id}`;
    uiStore.toggleRightSidebar(key);
  }
  function onClickBottom(s: PluginSidebarSection) {
    uiStore.toggleBottomSection(sectionKey(s) as any);
  }
</script>

{#if hasVisibleEntries}
  <ActivityBar side="right" ariaLabel="Right Activity Bar">
    {#snippet top()}
      {#each topSections as s (s.plugin_name + ':' + s.id)}
        {@const key = `plugin:${s.plugin_name}:${s.id}`}
        <button
          class="ab-btn"
          class:ab-active={uiStore.activeRightSidebar === key}
          use:tooltip={s.tooltip ?? s.label}
          aria-pressed={uiStore.activeRightSidebar === key}
          onclick={() => onClickTop(s)}
        >
          <PluginIcon name={s.icon} size={18} />
        </button>
      {/each}
    {/snippet}

    {#snippet bottom()}
      {#each bottomSections as s (s.plugin_name + ':' + s.id)}
        {@const key = sectionKey(s)}
        <button
          class="ab-btn"
          class:ab-active={uiStore.activeBottomSection === key}
          use:tooltip={s.tooltip ?? s.label}
          aria-pressed={uiStore.activeBottomSection === key}
          onclick={() => onClickBottom(s)}
        >
          <PluginIcon name={s.icon} size={18} />
        </button>
      {/each}
    {/snippet}
  </ActivityBar>
{/if}
