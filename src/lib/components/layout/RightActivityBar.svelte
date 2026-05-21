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
   * The container renders nothing when no plugin has registered a
   * right-side section, so the whole 40px rail stays invisible in that
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
  import { tooltipLeft as tooltip } from '$lib/actions/tooltip';

  function sectionKey(s: PluginSidebarSection): string {
    return `plugin:${s.plugin_name}:${s.id}`;
  }
  function bottomKey(s: PluginSidebarSection): string { return sectionKey(s); }

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

  /** The 40px rail itself should disappear when the user has hidden every
   *  plugin entry — mirrors the "no icons = no rail" rule applied for the
   *  zero-plugins case. */
  const hasVisibleEntries = $derived(topSections.length + bottomSections.length > 0);

  function onClickTop(s: PluginSidebarSection) {
    const key = `plugin:${s.plugin_name}:${s.id}`;
    uiStore.toggleRightSidebar(key);
  }
  function onClickBottom(s: PluginSidebarSection) {
    uiStore.toggleBottomSection(bottomKey(s) as any);
  }
</script>

{#if hasVisibleEntries}
  <div class="right-activity-bar" role="navigation" aria-label="Right Activity Bar">
    <div class="ab-group ab-top">
      {#each topSections as s (s.plugin_name + ':' + s.id)}
        {@const key = `plugin:${s.plugin_name}:${s.id}`}
        <button
          class="ab-btn"
          class:ab-active={uiStore.activeRightSidebar === key}
          use:tooltip={s.tooltip ?? s.label}
          aria-pressed={uiStore.activeRightSidebar === key}
          onclick={() => onClickTop(s)}
        >
          <PluginIcon name={s.icon} size={18} class="ab-icon" />
        </button>
      {/each}
    </div>

    <div class="ab-spacer"></div>

    <div class="ab-group ab-bottom">
      {#each bottomSections as s (s.plugin_name + ':' + s.id)}
        {@const key = bottomKey(s)}
        <button
          class="ab-btn"
          class:ab-active={uiStore.activeBottomSection === key}
          use:tooltip={s.tooltip ?? s.label}
          aria-pressed={uiStore.activeBottomSection === key}
          onclick={() => onClickBottom(s)}
        >
          <PluginIcon name={s.icon} size={18} class="ab-icon" />
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  /* Visual parity with the left ActivityBar: same width, bg, group layout.
     The active accent bar is positioned on the LEFT edge here (mirroring the
     left's right-edge accent would invert the semantics — the accent should
     always point into the panel it activates). */
  .right-activity-bar {
    display: flex;
    flex-direction: column;
    width: 38px;
    flex-shrink: 0;
    height: 100%;
    background: var(--bg-elevated);
    overflow: hidden;
    user-select: none;
  }

  .ab-group {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 6px 0;
  }
  .ab-spacer { flex: 1; }

  .ab-btn {
    position: relative;
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ab-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .ab-btn.ab-active {
    color: var(--accent);
    background: var(--accent-subtle);
  }
  /* Mirror of left ActivityBar's active accent bar: on the right we put it on
     the LEFT edge of the button (it's the edge adjacent to the panel it opens). */
  .ab-btn.ab-active::before {
    content: '';
    position: absolute;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
    width: 3px;
    height: 60%;
    /* height: 16px; */
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }

  /* Both Lucide SVG and emoji resolve through the shared PluginIcon
     component. Icons inherit the active color from `.ab-btn`. */
  :global(.right-activity-bar .ab-icon) { color: inherit; }
</style>
