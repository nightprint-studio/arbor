<script lang="ts">
  import {
    GitBranch, GitMerge, GitCommitHorizontal, PanelBottom,
    Zap, TerminalSquare, Play, ChevronDown, Workflow, GitPullRequest,
    TicketCheck, FolderTree, History, BarChart2, ShieldAlert, Boxes,
  } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { contributionStore } from '$lib/stores/corvus/contribution.svelte';
  import { firePluginAction } from '$lib/ipc/plugin';
  import { issuesStore } from '$lib/stores/corvus/issues.svelte';
  import { mrStore } from '$lib/stores/corvus/mr.svelte';
  import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
  import BrandIcon from '$lib/components/shared/internal/BrandIcon.svelte';
  import type { ActivityBarEntry, ComboOption } from '$lib/types/plugin';
  import { ACTIVITY_BAR_POINT, parseActivityBarEntry } from '$lib/contributions/activity-bar';
  import { SIDEBAR_POINT, parseSidebarSection } from '$lib/contributions/sidebar';
  import { VIEW_POINT, parseViewSection } from '$lib/contributions/view';
  import { activityBarConfigStore } from '$lib/stores/corvus/activityBarConfig.svelte';
  import PluginIcon from '../plugins/PluginIcon.svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import ActivityBar from '$lib/components/shared/ui/ActivityBar.svelte';
  import { tooltipForAction } from '$lib/utils/shortcut';
  // Activity bar is a narrow vertical rail; tooltips fly out to the right
  // so they don't overlap the bar itself.
  import { tooltipRight as tooltip } from '$lib/actions/tooltip';
  import type { TooltipInput } from '$lib/stores/tooltip.svelte';

  // ── Icon map ──────────────────────────────────────────────────────────────────
  const BUILTIN_ICONS: Record<string, unknown> = {
    branches:  GitBranch,
    gitflow:   GitMerge,
    mr:        GitPullRequest,
    issues:    TicketCheck,
    files:     FolderTree,
    reflog:    History,
    stats:     BarChart2,
    security:  ShieldAlert,
    studio:    Boxes,
    pipelines: Workflow,
    stage:     GitCommitHorizontal,
    detail:    PanelBottom,
    terminal:  TerminalSquare,
  };

  // ── Plugin items (bottom section only — actions/combos/separators) ────────────
  // Only activity_bar-targeted items, grouped by plugin name (alphabetical) so
  // buttons from the same plugin appear consecutively and the overall order is
  // stable regardless of plugin registration timing. Array#sort is stable
  // (ES2019) so within-plugin ordering is preserved.
  const rawPluginItems = $derived(
    contributionStore.forPoint(ACTIVITY_BAR_POINT)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseActivityBarEntry)
      .filter((e): e is ActivityBarEntry => e !== null)
      .filter(e => e.kind !== 'combo' || !e.target || e.target === 'activity_bar')
      .slice()
      .sort((a, b) => a.plugin_name.localeCompare(b.plugin_name))
  );

  // Build stable IDs for plugin items (same logic as in CustomizeActivityBarModal).
  function pluginEntryId(entry: ActivityBarEntry, sepIdx: number): string {
    if (entry.kind === 'action')    return `plugin:${entry.plugin_name}:${entry.action}`;
    if (entry.kind === 'combo')     return `plugin:${entry.plugin_name}:${entry.id}`;
    if (entry.kind === 'separator') return `plugin:${entry.plugin_name}:sep:${sepIdx}`;
    return 'plugin:unknown';
  }

  // Resolved plugin items with their stable IDs.
  const pluginItemsWithIds = $derived.by(() => {
    const sepCount: Record<string, number> = {};
    return rawPluginItems.map(e => {
      if (e.kind === 'separator') {
        sepCount[e.plugin_name] = (sepCount[e.plugin_name] ?? 0) + 1;
        return { entry: e, id: pluginEntryId(e, sepCount[e.plugin_name]) };
      }
      return { entry: e, id: pluginEntryId(e, 0) };
    });
  });

  const pluginBottomIds = $derived(pluginItemsWithIds.map(p => p.id));

  // ── Ordered + filtered item lists from config store ───────────────────────────
  const topItems    = $derived(
    activityBarConfigStore.mergeTop([]).filter(i => i.visible)
  );
  const bottomItems = $derived(
    activityBarConfigStore.mergeBottom(pluginBottomIds).filter(i => i.visible)
  );

  // Plugin sidebar sections registered via `add_sidebar({side: "left"})`.
  // Right-side entries are owned by ActivityBarRight.svelte.
  // Ordering + visibility flow through `activityBarConfigStore` so the user
  // can hide / reorder them via the Customize Activity Bar modal.
  function _leftSectionKey(s: { plugin_name: string; id: string }): string {
    return `plugin:${s.plugin_name}:${s.id}`;
  }
  function _leftResolveOrdered(position: 'top' | 'bottom') {
    const sections = contributionStore.forPoint(SIDEBAR_POINT)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseSidebarSection)
      .filter(s => s.side === 'left' && s.position === position);
    const byKey = new Map(sections.map(s => [_leftSectionKey(s), s]));
    const pluginIds = sections.map(_leftSectionKey);
    const merged = position === 'top'
      ? activityBarConfigStore.mergeTop(pluginIds)
      : activityBarConfigStore.mergeBottom(pluginIds);
    return merged
      .filter(i => i.visible)
      .map(i => byKey.get(i.id))
      .filter((s): s is NonNullable<ReturnType<typeof byKey.get>> => !!s);
  }
  const leftNewTopSections    = $derived(_leftResolveOrdered('top'));
  const leftNewBottomSections = $derived(_leftResolveOrdered('bottom'));

  // Plugin main-area views (add_view API). Rendered as their own icon group in
  // the top area; clicking toggles the body view (only one active at a time).
  // Sorted by plugin name for a stable order across registration timing.
  const pluginViews = $derived(
    contributionStore.forPoint(VIEW_POINT)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseViewSection)
      .slice()
      .sort((a, b) => a.plugin_name.localeCompare(b.plugin_name))
  );

  // ── Helpers ───────────────────────────────────────────────────────────────────
  function isEmoji(s?: string) { return s && [...s].length <= 2; }

  // ── Combo helpers ─────────────────────────────────────────────────────────────
  function selectOption(pluginName: string, comboId: string, opt: ComboOption) {
    const entry = rawPluginItems.find(
      e => e.kind === 'combo' && e.plugin_name === pluginName && e.id === comboId
    );
    if (entry?.kind !== 'combo') return;

    // Action options (e.g. "⚙ Settings…", "⊕ New profile…") behave like the
    // "New Workspace" footer in WorkspaceDropdown: they fire the combo's
    // run_action directly so the plugin can open its modal, and they do NOT
    // update the persisted selection. The previously selected item stays
    // visible in the button.
    if (opt.action) {
      firePluginAction(pluginName, entry.run_action, JSON.stringify({ value: opt.value, label: opt.label })).catch(() => {});
      return;
    }

    pluginStore.setComboSelection(pluginName, comboId, opt.value);
    if (entry.select_action) {
      firePluginAction(pluginName, entry.select_action, JSON.stringify({ value: opt.value, label: opt.label })).catch(() => {});
    }
  }

  async function runCombo(pluginName: string, runAction: string, pluginComboId: string) {
    const value = pluginStore.getComboSelection(pluginName, pluginComboId);
    const entry = rawPluginItems.find(
      e => e.kind === 'combo' && e.plugin_name === pluginName && e.id === pluginComboId
    );
    const label = entry?.kind === 'combo'
      ? (entry.options.find(o => o.value === value)?.label ?? value)
      : value;
    try { await firePluginAction(pluginName, runAction, JSON.stringify({ value, label })); }
    catch { /* ignore */ }
  }

  /** Build DropdownItem[] for a combo's selectable options, preserving the
   *  user-supplied group order. Action options are surfaced via the footer
   *  snippet, not via items. */
  function buildComboItems(entry: Extract<ActivityBarEntry, { kind: 'combo' }>): DropdownItem[] {
    const selected = pluginStore.getComboSelection(entry.plugin_name, entry.id);
    const out: DropdownItem[] = [];
    let currentGroup: string | null | undefined = undefined;
    let groupBucket: DropdownItem[] | null = null;

    const flush = () => {
      if (currentGroup && groupBucket && groupBucket.length > 0) {
        out.push({
          kind:  'group',
          id:    `g:${currentGroup}`,
          label: currentGroup,
          items: groupBucket,
        });
      } else if (groupBucket) {
        out.push(...groupBucket);
      }
      groupBucket = null;
    };

    for (const opt of entry.options) {
      if (opt.action) continue;
      const grp = opt.group ?? null;
      if (grp !== currentGroup) {
        flush();
        currentGroup = grp;
        groupBucket  = [];
      }
      groupBucket!.push({
        kind:     'item',
        id:       opt.value,
        label:    opt.label,
        icon:     opt.icon ? PLUGIN_ICONS[opt.icon] : undefined,
        subtitle: opt.subtitle,
        meta:     opt.meta,
        disabled: !!opt.disabled,
        active:   opt.value === selected,
        onclick:  () => selectOption(entry.plugin_name, entry.id, opt),
      });
    }
    flush();
    return out;
  }

  // ── Render helpers ────────────────────────────────────────────────────────────

  /** Find the plugin entry for a resolved item id. */
  function pluginEntryFor(id: string): ActivityBarEntry | undefined {
    return pluginItemsWithIds.find(p => p.id === id)?.entry;
  }

  /** Top-section tooltip for "Issues" button (needs provider name + shortcut). */
  function issuesTip(): TooltipInput {
    const label = `Issues (${issuesStore.activeProvider === 'jira' ? 'Jira' : 'Linear'})`;
    return tooltipForAction(label, 'toggle_issues_sidebar');
  }

  // ── Built-in section button metadata ─────────────────────────────────────────
  // The built-in top/bottom buttons are structurally identical (icon + active
  // accent + toggle); they differ only by tooltip and which store/toggle they
  // drive. The tooltip table keeps that the only per-button data — a label +
  // (where one exists) the keybinding action so the live shortcut shows in the
  // tooltip. A bare string is used where there's no bound shortcut. `issues` is
  // dynamic (provider name) so it's resolved separately.
  const BUILTIN_TIPS: Record<string, { label: string; action: string } | string> = {
    branches:  { label: 'Branches & Stashes',      action: 'toggle_branches_sidebar' },
    gitflow:   { label: 'Git Flow',                action: 'toggle_gitflow_sidebar'  },
    mr:        { label: 'Pull / Merge Requests',   action: 'toggle_mr_sidebar'       },
    files:     { label: 'Files',                   action: 'toggle_files_sidebar'    },
    reflog:    { label: 'Reflog',                  action: 'toggle_reflog_sidebar'   },
    stats:     { label: 'Repository Statistics',   action: 'toggle_stats_sidebar'    },
    security:  { label: 'Security',                action: 'toggle_security_sidebar' },
    studio:    'Studio — RON / JSON / TOML index',
    pipelines: { label: 'Pipelines',               action: 'toggle_pipelines_panel'  },
    stage:     { label: 'Stage & Commit',          action: 'stage_view'              },
    detail:    'Commit Detail',
    terminal:  { label: 'Terminal',                action: 'toggle_terminal'         },
  };

  function builtinTip(id: string): TooltipInput {
    if (id === 'issues') return issuesTip();
    const t = BUILTIN_TIPS[id];
    if (!t) return '';
    return typeof t === 'string' ? t : tooltipForAction(t.label, t.action);
  }

  // ── Brand icon resolution for the built-in MR / Issues buttons ───────────────
  // Mirrors what IntelliJ does: when a provider is detected, the sidebar icon
  // becomes the provider's brand mark (rendered monochrome via <BrandIcon>, so
  // it stays in the activity bar's color palette). When no provider is known
  // we fall back to the generic lucide icon.

  // Prime per-tab provider detection eagerly on tab switch — both stores
  // would otherwise only learn the provider when their respective sidebars
  // are opened, so the brand icon would be stuck on the lucide fallback
  // until the user clicked it. MR detection IPC is cached; the issues
  // tracker resolver reads `repo_config.toml` (cheap, local).
  $effect(() => {
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    mrStore.detectProvider(tabId).catch(() => {});
    issuesStore.loadProviderForTab(tabId);
  });

  const mrBrand = $derived(
    mrStore.provider === 'github' ? 'github' :
    mrStore.provider === 'gitlab' ? 'gitlab' : null
  );

  const issuesBrand = $derived(
    issuesStore.activeProvider === 'jira'   ? 'jira'   :
    issuesStore.activeProvider === 'linear' ? 'linear' : null
  );

  // The Security icon is always rendered; the SecurityPanel itself shows
  // a loading / "not available" state based on `providerSupportState`.
</script>

<!-- Shared shape for every built-in section button (top + bottom). The only
     bespoke case is the brand-icon swap for mr / issues when a provider is
     detected; everything else flows through `id` + the passed-in state. -->
{#snippet builtinButton(id: string, tip: TooltipInput, active: boolean, onClick: () => void)}
  {@const IconComp = BUILTIN_ICONS[id] as any}
  <button
    class="ab-btn"
    class:ab-active={active}
    use:tooltip={tip}
    aria-pressed={active}
    onclick={onClick}
  >
    {#if id === 'mr' && mrBrand}
      <BrandIcon brand={mrBrand} size={18} />
    {:else if id === 'issues' && issuesBrand}
      <BrandIcon brand={issuesBrand} size={18} />
    {:else}
      <IconComp size={20} />
    {/if}
  </button>
{/snippet}

<ActivityBar side="left">
  {#snippet top()}
    <!-- Built-in top sections (branches … studio). `security` and `studio`
         are always present in the list even where unavailable — their panels
         render the "not available" / probing copy themselves. -->
    {#each topItems as item (item.id)}
      {#if item.kind === 'builtin' && BUILTIN_ICONS[item.id]}
        {@render builtinButton(
          item.id,
          builtinTip(item.id),
          uiStore.activeSidebarSection === item.id,
          () => uiStore.toggleSidebarSection(item.id),
        )}
      {/if}
    {/each}

    <!-- Plugin sidebar icons (add_sidebar API) on the LEFT bar, top area.
         Clicking toggles the left sidebar panel to the plugin's content,
         which is loaded lazily via `panel:open:<id>` / set_panel_content. -->
    {#each leftNewTopSections as section (section.plugin_name + ':' + section.id)}
      {@const key = `plugin:${section.plugin_name}:${section.id}`}
      <button
        class="ab-btn"
        class:ab-active={uiStore.activeSidebarSection === key}
        use:tooltip={section.tooltip ?? section.label}
        aria-pressed={uiStore.activeSidebarSection === key}
        onclick={() => uiStore.toggleSidebarSection(key)}
      >
        <PluginIcon name={section.icon} size={20} />
      </button>
    {/each}

    <!-- Plugin main-area views (add_view API). Clicking toggles the body view
         that occupies the area where the commit graph lives. -->
    {#each pluginViews as view (view.plugin_name + ':' + view.id)}
      {@const vkey = `plugin:${view.plugin_name}:${view.id}`}
      <button
        class="ab-btn"
        class:ab-active={uiStore.activeMainView === vkey}
        use:tooltip={view.tooltip ?? view.label}
        aria-pressed={uiStore.activeMainView === vkey}
        onclick={() => uiStore.toggleMainView(vkey)}
      >
        <PluginIcon name={view.icon} size={20} />
      </button>
    {/each}
  {/snippet}

  {#snippet bottom()}
    {#each bottomItems as item (item.id)}
      {#if item.kind === 'builtin' && BUILTIN_ICONS[item.id]}
        {@render builtinButton(
          item.id,
          builtinTip(item.id),
          uiStore.activeBottomSection === item.id,
          () => uiStore.toggleBottomSection(item.id as any),
        )}

      {:else if item.kind === 'plugin'}
        {@const entry = pluginEntryFor(item.id)}
        {#if entry}
          {#if entry.kind === 'separator'}
            <div class="ab-separator" role="separator"></div>

          {:else if entry.kind === 'action'}
            <button
              class="ab-btn"
              use:tooltip={{ content: entry.label, description: entry.plugin_name }}
              onclick={async () => {
                try { await firePluginAction(entry.plugin_name, entry.action, '{}'); }
                catch { /* ignore */ }
              }}
            >
              {#if isEmoji(entry.icon)}
                <span class="ab-emoji">{entry.icon}</span>
              {:else}
                <Zap size={20} />
              {/if}
            </button>

          {:else if entry.kind === 'combo'}
            {@const selectedValue = pluginStore.getComboSelection(entry.plugin_name, entry.id)}
            {@const selectedLabel = entry.options.find(o => o.value === selectedValue)?.label ?? '—'}
            {@const ddItems       = buildComboItems(entry)}
            {@const actions       = entry.options.filter(o => o.action)}
            <Dropdown
              position="fixed"
              direction="right"
              items={ddItems}
              showFooter={actions.length > 0}
              emptyMessage="No configurations available"
            >
              {#snippet trigger({ open, toggle })}
                <div class="ab-combo" class:ab-combo-open={open}>
                  <button
                    class="ab-combo-run"
                    use:tooltip={entry.tooltip ?? `Run: ${selectedLabel}`}
                    onclick={() => runCombo(entry.plugin_name, entry.run_action, entry.id)}
                  >
                    {#if isEmoji(entry.run_icon)}
                      <span class="ab-emoji">{entry.run_icon}</span>
                    {:else}
                      <Play size={14} />
                    {/if}
                  </button>
                  <button
                    class="ab-combo-sel"
                    use:tooltip={`Select configuration: ${selectedLabel}`}
                    onclick={toggle}
                  >
                    <ChevronDown size={10} />
                  </button>
                </div>
              {/snippet}

              {#snippet footer({ close })}
                {#each actions as opt}
                  <button
                    class="ab-action-item"
                    onclick={() => { close(); selectOption(entry.plugin_name, entry.id, opt); }}
                  >{opt.label}</button>
                {/each}
              {/snippet}
            </Dropdown>
          {/if}
        {/if}
      {/if}
    {/each}

    <!-- New plugin sidebar icons (add_sidebar API) on the LEFT bar, bottom area.
         Clicking opens / closes the unique bottom panel to the plugin's
         content. A plugin-bottom click always overrides any other bottom
         panel (stage/detail/terminal/jobs/pipelines/another plugin). -->
    {#each leftNewBottomSections as section (section.plugin_name + ':' + section.id)}
      {@const bkey = `plugin:${section.plugin_name}:${section.id}`}
      <button
        class="ab-btn"
        class:ab-active={uiStore.activeBottomSection === bkey}
        use:tooltip={section.tooltip ?? section.label}
        aria-pressed={uiStore.activeBottomSection === bkey}
        onclick={() => uiStore.toggleBottomSection(bkey as any)}
      >
        <PluginIcon name={section.icon} size={20} />
      </button>
    {/each}
  {/snippet}
</ActivityBar>

<style>
  /* Container, button, group, spacer, separator and emoji styles live in the
     shared <ActivityBar> shell (shared/ui/ActivityBar.svelte) as :global() rules
     so they apply equally on the left and right rails. This file only owns
     the combo widget — a left-only construct used by plugin-registered combos
     (e.g. compile-action's run-config picker). */

  .ab-combo {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 34px;
    border-radius: var(--radius-md);
    overflow: hidden;
    transition: background var(--transition-fast);
  }

  .ab-combo:hover { background: var(--bg-hover); }
  .ab-combo.ab-combo-open { background: var(--accent-subtle); }

  .ab-combo-run {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 22px;
    border: none;
    background: transparent;
    color: var(--accent);
    cursor: pointer;
    transition: color var(--transition-fast);
    padding: 0;
  }
  .ab-combo-run:hover { color: var(--accent-hover); }

  .ab-combo-sel {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 12px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    border-top: 1px solid var(--border-subtle);
    transition: color var(--transition-fast), background var(--transition-fast);
    padding: 0;
  }
  .ab-combo-sel:hover {
    background: rgba(255,255,255,0.06);
    color: var(--text-secondary);
  }

  /* ── Action options (footer of combo dropdown) ─────────────────────────────
     Clicking these fires the combo's run_action (opens a modal) and doesn't
     touch the persisted selection. Rendered inside Dropdown's `footer` slot;
     the snippet carries this component's scope, so the styles apply. */
  .ab-action-item {
    display: block;
    width: 100%;
    padding: 6px 10px;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ab-action-item:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
