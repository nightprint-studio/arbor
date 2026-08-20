<script lang="ts">
  /**
   * Corvus's driver for the shared rails dialog.
   *
   * What used to be here — the dragging, the eye, the lock, the tabs, the drop indicator —
   * now lives in `shared/CustomizeRailsModal`, because Bennu needed the same dialog and a
   * second copy of eight hundred lines of drag handling is a second copy of every corner
   * case in it. What stays is everything that is genuinely about *Corvus's* bar, and that is
   * most of the difficulty:
   *
   *  • **Plugin items.** They are contributions, not built-ins: their ids are synthesised
   *    (`plugin:{name}:{action}`), their labels and icons come from the contribution, and a
   *    separator is an item with a position that has to survive reordering.
   *  • **Two kinds of right bar.** Legacy `activity_bar_items` land in the left bar's bottom
   *    cluster; `add_sidebar(side="right")` contributions land in the right bar.
   *  • **Mirroring.** With `activity_bar_position = 'right'` the built-in bar is drawn on the
   *    right, so the tab that says "Left" has to be the one the user is looking at on the
   *    left. The stored value stays semantic — `left` means *built-ins* — and only the labels
   *    and the tab order swap.
   *
   * The lists are read ONCE, here and in the shared dialog both: `contributionStore` mutates
   * on every `arbor://contributions-changed` event, which any plugin's scheduler emits on a
   * tick. A reactive seed used to re-run a few hundred milliseconds later and overwrite the
   * toggle the user had just made.
   */
  import {
    GitBranch, GitMerge, GitCommitHorizontal, PanelBottom, Workflow, GitPullRequest,
    TicketCheck, FolderTree, History, BarChart2, ShieldAlert, Boxes, TerminalSquare,
  } from 'lucide-svelte';
  import CustomizeRailsModal, {
    type RailEditorRow, type RailEditorSection, type RailEditorTab,
  } from '$lib/components/shared/CustomizeRailsModal.svelte';
  import { contributionStore } from '$lib/stores/corvus/contribution.svelte';
  import {
    activityBarConfigStore, MANDATORY_IDS, BUILTIN_TOP, BUILTIN_BOTTOM,
    type ActivityBarDisplayItem,
  } from '$lib/stores/corvus/activityBarConfig.svelte';
  import type { ActivityBarEntry } from '$lib/types/plugin';
  import type { IconComponent } from '$lib/types/icon';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { ACTIVITY_BAR_POINT, parseActivityBarEntry } from '$lib/contributions/activity-bar';
  import { SIDEBAR_POINT, parseSidebarSection } from '$lib/contributions/sidebar';

  let { onClose }: { onClose: () => void } = $props();

  const mirrored = appearanceStore.activityBarPosition === 'right';

  // ── Contributions ────────────────────────────────────────────────────────────

  function activityBarEntries(): ActivityBarEntry[] {
    return contributionStore.forPoint(ACTIVITY_BAR_POINT)
      .filter((c) => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseActivityBarEntry)
      .filter((e): e is ActivityBarEntry => e !== null);
  }
  function sidebarSections() {
    return contributionStore.forPoint(SIDEBAR_POINT)
      .filter((c) => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseSidebarSection);
  }

  function pluginEntryId(entry: ActivityBarEntry, sepIndex: number): string {
    if (entry.kind === 'action')    return `plugin:${entry.plugin_name}:${entry.action}`;
    if (entry.kind === 'combo')     return `plugin:${entry.plugin_name}:${entry.id}`;
    if (entry.kind === 'separator') return `plugin:${entry.plugin_name}:sep:${sepIndex}`;
    return 'plugin:unknown';
  }
  function pluginEntryLabel(entry: ActivityBarEntry): string {
    if (entry.kind === 'action')    return `${entry.plugin_name}: ${entry.label}`;
    if (entry.kind === 'combo')     return `${entry.plugin_name}: ${entry.id}`;
    if (entry.kind === 'separator') return `${entry.plugin_name}: separator`;
    return 'Plugin item';
  }

  function pluginLeftTopIds(): string[] {
    return sidebarSections()
      .filter((s) => s.side === 'left' && s.position === 'top')
      .map((s) => `plugin:${s.plugin_name}:${s.id}`);
  }
  function pluginLeftBottomIdsFromSidebar(): string[] {
    return sidebarSections()
      .filter((s) => s.side === 'left' && s.position === 'bottom')
      .map((s) => `plugin:${s.plugin_name}:${s.id}`);
  }
  /** Legacy `activity_bar_items` live in the LEFT bar's bottom cluster, by convention. */
  function pluginBottomIds(): string[] {
    const entries = activityBarEntries().filter(
      (e) => e.kind !== 'combo' || !e.target || e.target === 'activity_bar',
    );
    const sepCount: Record<string, number> = {};
    const actionIds = entries.map((e) => {
      if (e.kind === 'separator') {
        sepCount[e.plugin_name] = (sepCount[e.plugin_name] ?? 0) + 1;
        return pluginEntryId(e, sepCount[e.plugin_name]);
      }
      return pluginEntryId(e, 0);
    });
    return [...actionIds, ...pluginLeftBottomIdsFromSidebar()];
  }
  function pluginRightTopIds(): string[] {
    return sidebarSections()
      .filter((s) => s.side === 'right' && s.position === 'top')
      .map((s) => `plugin:${s.plugin_name}:${s.id}`);
  }
  function pluginRightBottomIds(): string[] {
    return sidebarSections()
      .filter((s) => s.side === 'right' && s.position === 'bottom')
      .map((s) => `plugin:${s.plugin_name}:${s.id}`);
  }

  // ── Labels and icons ─────────────────────────────────────────────────────────

  const BUILTIN_ICONS: Record<string, IconComponent> = {
    branches:  GitBranch as unknown as IconComponent,
    gitflow:   GitMerge as unknown as IconComponent,
    mr:        GitPullRequest as unknown as IconComponent,
    issues:    TicketCheck as unknown as IconComponent,
    files:     FolderTree as unknown as IconComponent,
    reflog:    History as unknown as IconComponent,
    stats:     BarChart2 as unknown as IconComponent,
    security:  ShieldAlert as unknown as IconComponent,
    studio:    Boxes as unknown as IconComponent,
    pipelines: Workflow as unknown as IconComponent,
    stage:     GitCommitHorizontal as unknown as IconComponent,
    detail:    PanelBottom as unknown as IconComponent,
    terminal:  TerminalSquare as unknown as IconComponent,
  };

  function pluginLabelFor(id: string): string {
    const sepCount: Record<string, number> = {};
    for (const e of activityBarEntries()) {
      if (e.kind === 'separator') {
        sepCount[e.plugin_name] = (sepCount[e.plugin_name] ?? 0) + 1;
        if (pluginEntryId(e, sepCount[e.plugin_name]) === id) return pluginEntryLabel(e);
      } else if (pluginEntryId(e, 0) === id) {
        return pluginEntryLabel(e);
      }
    }
    return id;
  }
  /** A plugin icon is a short emoji string or nothing — the dialog renders text or its own
   *  fallback glyph, and there is no third case to model. */
  function pluginGlyphFor(id: string): string | undefined {
    for (const e of activityBarEntries()) {
      const icon = e.kind === 'action' ? e.icon : e.kind === 'combo' ? e.run_icon : undefined;
      if (icon && pluginEntryId(e, 0) === id && [...icon].length <= 2) return icon;
    }
    return undefined;
  }

  function row(item: ActivityBarDisplayItem): RailEditorRow {
    return {
      id: item.id,
      label: item.kind === 'plugin' ? pluginLabelFor(item.id) : item.label,
      icon: item.kind === 'builtin' ? BUILTIN_ICONS[item.id] : pluginGlyphFor(item.id),
      visible: item.visible,
      mandatory: item.mandatory,
    };
  }

  // ── The bars ─────────────────────────────────────────────────────────────────

  const SECTION_META: Record<string, { label: string; hint: string }> = {
    top:    { label: 'Sidebar', hint: 'the icon rail' },
    bottom: { label: 'Panel',   hint: 'bottom dock (stage, diff, terminal…)' },
  };

  function sections(side: 'left' | 'right'): RailEditorSection[] {
    const top = side === 'left'
      ? activityBarConfigStore.mergeTop(pluginLeftTopIds())
      : activityBarConfigStore.mergeRightTop(pluginRightTopIds());
    const bottom = side === 'left'
      ? activityBarConfigStore.mergeBottom(pluginBottomIds())
      : activityBarConfigStore.mergeRightBottom(pluginRightBottomIds());
    return [
      { id: `${side}-top`,    ...SECTION_META.top,    items: top.map(row) },
      { id: `${side}-bottom`, ...SECTION_META.bottom, items: bottom.map(row) },
    ];
  }

  /** Canonical order, everything visible: built-ins first, then plugins as registered. */
  function defaults(side: 'left' | 'right'): RailEditorSection[] {
    const build = (
      builtins: { id: string; label: string; mandatory: boolean }[],
      pluginIds: string[],
    ): RailEditorRow[] => [
      ...builtins.map((b) => ({
        id: b.id, label: b.label, icon: BUILTIN_ICONS[b.id],
        visible: true, mandatory: b.mandatory,
      })),
      ...pluginIds.map((id) => ({
        id, label: pluginLabelFor(id), icon: pluginGlyphFor(id),
        visible: true, mandatory: false,
      })),
    ];
    return side === 'left'
      ? [
          { id: 'left-top',    ...SECTION_META.top,    items: build(BUILTIN_TOP, pluginLeftTopIds()) },
          { id: 'left-bottom', ...SECTION_META.bottom, items: build(BUILTIN_BOTTOM, pluginBottomIds()) },
        ]
      : [
          { id: 'right-top',    ...SECTION_META.top,    items: build([], pluginRightTopIds()) },
          { id: 'right-bottom', ...SECTION_META.bottom, items: build([], pluginRightBottomIds()) },
        ];
  }

  const LEFT_TAB: RailEditorTab = {
    id: 'left',
    label: mirrored ? 'Right' : 'Left',
    hint: `Built-in sections and legacy plugins, on the ${mirrored ? 'right' : 'left'} bar.`,
    sections: sections('left'),
  };
  const RIGHT_TAB: RailEditorTab = {
    id: 'right',
    label: mirrored ? 'Left' : 'Right',
    hint: `Plugins registered with side="right", currently on the ${mirrored ? 'left' : 'right'} bar. Nothing here yet? Install a plugin that uses add_sidebar.`,
    sections: sections('right'),
  };
  // The physically-left bar first, whichever one that is — the tabs read as positions.
  const tabs: RailEditorTab[] = mirrored ? [RIGHT_TAB, LEFT_TAB] : [LEFT_TAB, RIGHT_TAB];

  function resetTab(tabId: string): RailEditorSection[] {
    return defaults(tabId === 'left' ? 'left' : 'right');
  }

  async function save(edited: RailEditorTab[]): Promise<void> {
    const bySection = new Map<string, RailEditorRow[]>();
    for (const tab of edited) {
      for (const s of tab.sections) bySection.set(s.id, s.items);
    }
    const items = (id: string) =>
      (bySection.get(id) ?? []).map((i) => ({
        id: i.id,
        visible: MANDATORY_IDS.has(i.id) ? true : i.visible,
      }));
    await activityBarConfigStore.saveItems(
      items('left-top'), items('left-bottom'),
      items('right-top'), items('right-bottom'),
    );
  }
</script>

<CustomizeRailsModal {tabs} onSave={save} onResetTab={resetTab} {onClose} />
