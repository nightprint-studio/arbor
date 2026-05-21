<script lang="ts">
  import {
    GitBranch, GitMerge, GitCommitHorizontal, PanelBottom,
    Zap, TerminalSquare, Play, ChevronDown, Workflow, GitPullRequest,
    TicketCheck, FolderTree, History, BarChart2, ShieldAlert, Boxes,
  } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { firePluginAction } from '$lib/ipc/plugin';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import { mrStore } from '$lib/stores/mr.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import BrandIcon from '$lib/components/shared/ui/BrandIcon.svelte';
  import type { ActivityBarEntry, ComboOption } from '$lib/types/plugin';
  import { ACTIVITY_BAR_POINT, parseActivityBarEntry } from '$lib/contributions/activity-bar';
  import { SIDEBAR_POINT, parseSidebarSection } from '$lib/contributions/sidebar';
  import { activityBarConfigStore } from '$lib/stores/activityBarConfig.svelte';
  import PluginIcon from '../plugins/PluginIcon.svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import ActivityBar from './ActivityBar.svelte';
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

<ActivityBar side="left">
  {#snippet top()}
    {#each topItems as item (item.id)}
      {#if item.kind === 'builtin'}
        {@const IconComp = BUILTIN_ICONS[item.id] as any}

        {#if item.id === 'branches'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeSidebarSection === 'branches'}
            use:tooltip={tooltipForAction('Branches & Stashes', 'toggle_branches_sidebar')}
            aria-pressed={uiStore.activeSidebarSection === 'branches'}
            onclick={() => uiStore.toggleSidebarSection('branches')}
          >
            <IconComp size={20} />
          </button>

        {:else if item.id === 'gitflow'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeSidebarSection === 'gitflow'}
            use:tooltip={tooltipForAction('Git Flow', 'toggle_gitflow_sidebar')}
            aria-pressed={uiStore.activeSidebarSection === 'gitflow'}
            onclick={() => uiStore.toggleSidebarSection('gitflow')}
          >
            <IconComp size={20} />
          </button>

        {:else if item.id === 'mr'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeSidebarSection === 'mr'}
            use:tooltip={tooltipForAction('Pull / Merge Requests', 'toggle_mr_sidebar')}
            aria-pressed={uiStore.activeSidebarSection === 'mr'}
            onclick={() => uiStore.toggleSidebarSection('mr')}
          >
            {#if mrBrand}
              <BrandIcon brand={mrBrand} size={18} />
            {:else}
              <IconComp size={20} />
            {/if}
          </button>

        {:else if item.id === 'issues'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeSidebarSection === 'issues'}
            use:tooltip={issuesTip()}
            aria-pressed={uiStore.activeSidebarSection === 'issues'}
            onclick={() => uiStore.toggleSidebarSection('issues')}
          >
            {#if issuesBrand}
              <BrandIcon brand={issuesBrand} size={18} />
            {:else}
              <IconComp size={20} />
            {/if}
          </button>

        {:else if item.id === 'files'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeSidebarSection === 'files'}
            use:tooltip={tooltipForAction('File Tree', 'toggle_files_sidebar')}
            aria-pressed={uiStore.activeSidebarSection === 'files'}
            onclick={() => uiStore.toggleSidebarSection('files')}
          >
            <IconComp size={20} />
          </button>

        {:else if item.id === 'reflog'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeSidebarSection === 'reflog'}
            use:tooltip={tooltipForAction('Reflog', 'toggle_reflog_sidebar')}
            aria-pressed={uiStore.activeSidebarSection === 'reflog'}
            onclick={() => uiStore.toggleSidebarSection('reflog')}
          >
            <IconComp size={20} />
          </button>

        {:else if item.id === 'stats'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeSidebarSection === 'stats'}
            use:tooltip={tooltipForAction('Repository Statistics', 'toggle_stats_sidebar')}
            aria-pressed={uiStore.activeSidebarSection === 'stats'}
            onclick={() => uiStore.toggleSidebarSection('stats')}
          >
            <IconComp size={20} />
          </button>

        {:else if item.id === 'security'}
          <!-- Always rendered so users can confirm the dashboard is wired
               up even on repos where it isn't available. The SecurityPanel
               handles the "not available" / probing copy itself. -->
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeSidebarSection === 'security'}
            use:tooltip={tooltipForAction('Security', 'toggle_security_sidebar')}
            aria-pressed={uiStore.activeSidebarSection === 'security'}
            onclick={() => uiStore.toggleSidebarSection('security')}
          >
            <IconComp size={20} />
          </button>

        {:else if item.id === 'studio'}
          <!-- Studio: project-wide index of .ron / .json / .toml files,
               clicking jumps into the matching viewer (RON / JSON Studio,
               TOML viewer is on the roadmap). -->
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeSidebarSection === 'studio'}
            use:tooltip={'Studio — RON / JSON / TOML index'}
            aria-pressed={uiStore.activeSidebarSection === 'studio'}
            onclick={() => uiStore.toggleSidebarSection('studio')}
          >
            <IconComp size={20} />
          </button>
        {/if}
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
  {/snippet}

  {#snippet bottom()}
    {#each bottomItems as item (item.id)}
      {#if item.kind === 'builtin'}
        {@const IconComp = BUILTIN_ICONS[item.id] as any}

        {#if item.id === 'pipelines'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeBottomSection === 'pipelines'}
            use:tooltip={tooltipForAction('Pipelines', 'toggle_pipelines_panel')}
            aria-pressed={uiStore.activeBottomSection === 'pipelines'}
            onclick={() => uiStore.toggleBottomSection('pipelines')}
          >
            <IconComp size={20} />
          </button>

        {:else if item.id === 'stage'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeBottomSection === 'stage'}
            use:tooltip={tooltipForAction('Stage & Commit', 'stage_view')}
            aria-pressed={uiStore.activeBottomSection === 'stage'}
            onclick={() => uiStore.toggleBottomSection('stage')}
          >
            <IconComp size={20} />
          </button>

        {:else if item.id === 'detail'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeBottomSection === 'detail'}
            use:tooltip={'Commit Detail'}
            aria-pressed={uiStore.activeBottomSection === 'detail'}
            onclick={() => uiStore.toggleBottomSection('detail')}
          >
            <IconComp size={20} />
          </button>

        {:else if item.id === 'terminal'}
          <button
            class="ab-btn"
            class:ab-active={uiStore.activeBottomSection === 'terminal'}
            use:tooltip={tooltipForAction('Terminal', 'toggle_terminal')}
            aria-pressed={uiStore.activeBottomSection === 'terminal'}
            onclick={() => uiStore.toggleBottomSection('terminal')}
          >
            <IconComp size={20} />
          </button>
        {/if}

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
     shared <ActivityBar> shell (layout/ActivityBar.svelte) as :global() rules
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
