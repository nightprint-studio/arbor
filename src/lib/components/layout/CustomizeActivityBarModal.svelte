<script lang="ts">
  import {
    GripVertical, Eye, EyeOff, Lock,
    GitBranch, GitMerge, GitCommitHorizontal, PanelBottom,
    Zap, TerminalSquare, Workflow, GitPullRequest, TicketCheck,
    FolderTree, History, BarChart2, ShieldAlert, Boxes, RotateCcw,
  } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import {
    activityBarConfigStore,
    MANDATORY_IDS,
    BUILTIN_TOP,
    BUILTIN_BOTTOM,
    type ActivityBarDisplayItem,
  } from '$lib/stores/activityBarConfig.svelte';
  import type { ActivityBarEntry } from '$lib/types/plugin';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { ACTIVITY_BAR_POINT, parseActivityBarEntry } from '$lib/contributions/activity-bar';
  import { SIDEBAR_POINT, parseSidebarSection } from '$lib/contributions/sidebar';
  import { onMount } from 'svelte';

  function activityBarEntries(): ActivityBarEntry[] {
    return contributionStore.forPoint(ACTIVITY_BAR_POINT)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseActivityBarEntry)
      .filter((e): e is ActivityBarEntry => e !== null);
  }
  function sidebarSections() {
    return contributionStore.forPoint(SIDEBAR_POINT)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseSidebarSection);
  }

  let { onClose }: { onClose: () => void } = $props();

  // ── Icon mapping for built-in items ─────────────────────────────────────────
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

  // ── Derive plugin item IDs ───────────────────────────────────────────────────

  function pluginEntryId(entry: ActivityBarEntry, sepIndex: number): string {
    if (entry.kind === 'action')    return `plugin:${entry.plugin_name}:${entry.action}`;
    if (entry.kind === 'combo')     return `plugin:${entry.plugin_name}:${entry.id}`;
    if (entry.kind === 'separator') return `plugin:${entry.plugin_name}:sep:${sepIndex}`;
    return `plugin:unknown`;
  }

  function pluginEntryLabel(entry: ActivityBarEntry): string {
    if (entry.kind === 'action')    return `${entry.plugin_name}: ${entry.label}`;
    if (entry.kind === 'combo')     return `${entry.plugin_name}: ${entry.id}`;
    if (entry.kind === 'separator') return `${entry.plugin_name}: separator`;
    return 'Plugin item';
  }

  function isEmojiIcon(s?: string) { return s && [...s].length <= 2; }

  // ── Build plugin ID list for the LEFT bar sections ──────────────────────────
  // Plugin items from `activity_bar_items` (actions/combos/separators) live
  // in the bottom section by convention. New `add_sidebar(side="left")` entries
  // contribute to TOP or BOTTOM depending on their declared position — we
  // surface them here so the user can reorder / hide them from the Left tab.
  //
  // IMPORTANT: these are PLAIN FUNCTIONS, not `$derived`. The modal snapshots
  // them ONCE in `onMount`; making them reactive caused the snapshot effect to
  // re-fire on every `arbor://contributions-changed` event (emitted by any
  // plugin's scheduler tick — e.g. security-auto-refresh), which overwrote the
  // user's pending toggles in the modal mid-session.
  function pluginLeftTopIds(): string[] {
    return sidebarSections()
      .filter(s => s.side === 'left' && s.position === 'top')
      .map(s => `plugin:${s.plugin_name}:${s.id}`);
  }
  function pluginLeftBottomIdsFromSidebar(): string[] {
    return sidebarSections()
      .filter(s => s.side === 'left' && s.position === 'bottom')
      .map(s => `plugin:${s.plugin_name}:${s.id}`);
  }
  function pluginBottomIds(): string[] {
    const entries = activityBarEntries().filter(
      e => e.kind !== 'combo' || !e.target || e.target === 'activity_bar'
    );
    const sepCount: Record<string, number> = {};
    const actionIds = entries.map(e => {
      if (e.kind === 'separator') {
        sepCount[e.plugin_name] = (sepCount[e.plugin_name] ?? 0) + 1;
        return pluginEntryId(e, sepCount[e.plugin_name]);
      }
      return pluginEntryId(e, 0);
    });
    return [...actionIds, ...pluginLeftBottomIdsFromSidebar()];
  }

  // Label/icon helpers for plugin items
  function pluginLabelFor(id: string): string {
    const entries = activityBarEntries();
    const sepCount: Record<string, number> = {};
    for (const e of entries) {
      if (e.kind === 'separator') {
        sepCount[e.plugin_name] = (sepCount[e.plugin_name] ?? 0) + 1;
        if (pluginEntryId(e, sepCount[e.plugin_name]) === id) return pluginEntryLabel(e);
      } else {
        if (pluginEntryId(e, 0) === id) return pluginEntryLabel(e);
      }
    }
    return id;
  }

  function pluginIconFor(id: string): string | undefined {
    const entries = activityBarEntries();
    for (const e of entries) {
      if (e.kind === 'action' && pluginEntryId(e, 0) === id) return e.icon;
      if (e.kind === 'combo'  && pluginEntryId(e, 0) === id) return e.run_icon;
    }
    return undefined;
  }

  // ── Working copies (mutable lists for drag-and-drop) ────────────────────────
  // We keep all four lists in memory. The user switches between bars via the
  // top-level tabs; tab switch flushes current edits back into the owning
  // side arrays and swaps topItems/bottomItems to the target side. On save
  // we persist all four at once.
  let leftTopItems      = $state<ActivityBarDisplayItem[]>([]);
  let leftBottomItems   = $state<ActivityBarDisplayItem[]>([]);
  let rightTopItems_    = $state<ActivityBarDisplayItem[]>([]);
  let rightBottomItems_ = $state<ActivityBarDisplayItem[]>([]);

  /** Which bar we're currently editing. */
  let activeTab = $state<'left' | 'right'>('left');

  // Plugin ids registered for the RIGHT bar (via add_sidebar with side="right").
  function pluginRightTopIds(): string[] {
    return sidebarSections()
      .filter(s => s.side === 'right' && s.position === 'top')
      .map(s => `plugin:${s.plugin_name}:${s.id}`);
  }
  function pluginRightBottomIds(): string[] {
    return sidebarSections()
      .filter(s => s.side === 'right' && s.position === 'bottom')
      .map(s => `plugin:${s.plugin_name}:${s.id}`);
  }

  // Mutable lists actually bound to the DOM — they swap content as the user
  // flips between Left/Right tabs. Initialized from the left bar, rewritten
  // on each tab switch.
  let topItems    = $state<ActivityBarDisplayItem[]>([]);
  let bottomItems = $state<ActivityBarDisplayItem[]>([]);

  // Snapshot the merged state from the store ONCE on mount. We deliberately
  // do NOT do this in a `$effect` — `contributionStore._byPoint` mutates on
  // every `arbor://contributions-changed` event (plugin schedulers fire these
  // routinely, e.g. security-auto-refresh ticks). A reactive seed would then
  // re-run and overwrite the user's pending toggles within a few hundred ms,
  // making it impossible to land an edit before Save.
  onMount(() => {
    leftTopItems      = activityBarConfigStore.mergeTop(pluginLeftTopIds()).map(i => ({ ...i }));
    leftBottomItems   = activityBarConfigStore.mergeBottom(pluginBottomIds()).map(i => ({ ...i }));
    rightTopItems_    = activityBarConfigStore.mergeRightTop(pluginRightTopIds()).map(i => ({ ...i }));
    rightBottomItems_ = activityBarConfigStore.mergeRightBottom(pluginRightBottomIds()).map(i => ({ ...i }));
    if (activeTab === 'left') {
      topItems    = leftTopItems.map(i => ({ ...i }));
      bottomItems = leftBottomItems.map(i => ({ ...i }));
    } else {
      topItems    = rightTopItems_.map(i => ({ ...i }));
      bottomItems = rightBottomItems_.map(i => ({ ...i }));
    }
  });

  /** Save current edits into the owning side arrays (for tab switch / save). */
  function flushCurrentTab() {
    if (activeTab === 'left') {
      leftTopItems    = topItems.map(i => ({ ...i }));
      leftBottomItems = bottomItems.map(i => ({ ...i }));
    } else {
      rightTopItems_    = topItems.map(i => ({ ...i }));
      rightBottomItems_ = bottomItems.map(i => ({ ...i }));
    }
  }

  function setActiveTab(tab: 'left' | 'right') {
    if (tab === activeTab) return;
    flushCurrentTab();
    activeTab = tab;
    if (tab === 'left') {
      topItems    = leftTopItems.map(i => ({ ...i }));
      bottomItems = leftBottomItems.map(i => ({ ...i }));
    } else {
      topItems    = rightTopItems_.map(i => ({ ...i }));
      bottomItems = rightBottomItems_.map(i => ({ ...i }));
    }
  }

  // ── Drag-and-drop (same mouse-event pattern as TitleBar/BranchTree) ──────────

  type Section = 'top' | 'bottom';
  let dragState = $state<{
    section: Section;
    fromIndex: number;
    insertBefore: number;
  } | null>(null);

  let topListEl    = $state<HTMLElement | undefined>(undefined);
  let bottomListEl = $state<HTMLElement | undefined>(undefined);

  function startDrag(e: MouseEvent, section: Section, fromIndex: number) {
    if (e.button !== 0) return;
    const startY = e.clientY;
    let active = false;

    function onMove(ev: MouseEvent) {
      if (!active) {
        if (Math.abs(ev.clientY - startY) < 4) return;
        active = true;
        dragState = { section, fromIndex, insertBefore: fromIndex };
        document.body.style.cursor = 'grabbing';
      }
      if (!dragState) return;
      const listEl = section === 'top' ? topListEl : bottomListEl;
      dragState = { ...dragState, insertBefore: calcInsert(ev.clientY, listEl) };
    }

    function onUp() {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      if (!active || !dragState) { dragState = null; return; }

      const { section: s, fromIndex: from, insertBefore } = dragState;
      dragState = null;

      const list = s === 'top' ? topItems : bottomItems;
      const to = insertBefore <= from ? insertBefore : insertBefore - 1;
      if (to === from) return;

      const next = [...list];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      if (s === 'top') topItems = next;
      else             bottomItems = next;
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  function calcInsert(y: number, listEl: HTMLElement | undefined): number {
    if (!listEl) return 0;
    const rows = listEl.querySelectorAll<HTMLElement>('[data-drag-idx]');
    for (let i = 0; i < rows.length; i++) {
      const r = rows[i].getBoundingClientRect();
      if (y < r.top + r.height / 2) return i;
    }
    return rows.length;
  }

  // ── Toggle visibility ────────────────────────────────────────────────────────
  function toggleVisibility(section: Section, idx: number) {
    if (section === 'top') {
      topItems = topItems.map((item, i) =>
        i === idx && !item.mandatory ? { ...item, visible: !item.visible } : item
      );
    } else {
      bottomItems = bottomItems.map((item, i) =>
        i === idx && !item.mandatory ? { ...item, visible: !item.visible } : item
      );
    }
  }

  // ── Restore defaults (current tab only) ──────────────────────────────────────
  // Resets the active tab's top + bottom lists to the canonical order:
  // built-ins first (in BUILTIN_TOP / BUILTIN_BOTTOM order), then plugin items
  // in their natural registration order. All items are made visible.
  // Pending edits on the opposite tab are preserved; Save still required to
  // persist the change.
  function buildDefault(side: 'left' | 'right', section: 'top' | 'bottom'): ActivityBarDisplayItem[] {
    const builtins = side === 'left'
      ? (section === 'top' ? BUILTIN_TOP : BUILTIN_BOTTOM)
      : [];
    const pluginIds = side === 'left'
      ? (section === 'top' ? pluginLeftTopIds() : pluginBottomIds())
      : (section === 'top' ? pluginRightTopIds() : pluginRightBottomIds());

    const result: ActivityBarDisplayItem[] = [];
    for (const b of builtins) {
      result.push({
        id: b.id,
        visible: true,
        label: b.label,
        mandatory: b.mandatory,
        kind: 'builtin',
        section,
      });
    }
    for (const pid of pluginIds) {
      result.push({
        id: pid,
        visible: true,
        label: pluginLabelFor(pid),
        mandatory: false,
        kind: 'plugin',
        section,
      });
    }
    return result;
  }

  function resetCurrentTab() {
    topItems    = buildDefault(activeTab, 'top');
    bottomItems = buildDefault(activeTab, 'bottom');
  }

  // ── Save ─────────────────────────────────────────────────────────────────────
  let saving = $state(false);

  async function handleSave() {
    saving = true;
    try {
      // Commit the currently-visible tab's edits back to its side arrays,
      // then persist both bars in one call.
      flushCurrentTab();
      await activityBarConfigStore.saveItems(
        leftTopItems, leftBottomItems,
        rightTopItems_, rightBottomItems_,
      );
      onClose();
    } finally {
      saving = false;
    }
  }

  </script>

<Modal {onClose} width="420px" height="80vh" padBody={false} ariaLabel="Customize Activity Bar">
  {#snippet header()}
    <ModalHeader title="Customize Activity Bar" {onClose} />
  {/snippet}

  <div class="cab-body">
  <!-- Tabs: Left / Right bar -->
  <div class="tabs" role="tablist">
    <button
      class="tab-btn"
      class:tab-active={activeTab === 'left'}
      role="tab"
      aria-selected={activeTab === 'left'}
      onclick={() => setActiveTab('left')}
    >Left</button>
    <button
      class="tab-btn"
      class:tab-active={activeTab === 'right'}
      role="tab"
      aria-selected={activeTab === 'right'}
      onclick={() => setActiveTab('right')}
    >Right</button>
  </div>

  <!-- Content -->
  <div class="cab-content">
    <p class="hint">
      {#if activeTab === 'left'}
        Built-in sections + legacy plugins on the <strong>left</strong> bar.
      {:else}
        Plugins registered with <code>side="right"</code>.
        Nothing here yet? Install a plugin that uses the new <code>add_sidebar</code> API.
      {/if}
      <br>
      Drag to reorder · click the eye to show/hide · <Lock size={10} class="hint-lock" /> locked items are always visible.
    </p>

    <!-- Top section -->
    <div class="section">
      <div class="section-label">
        <span>Sidebar</span>
        <span class="section-label-hint">left icon rail</span>
      </div>
      <div class="item-list" bind:this={topListEl}>
        {#each topItems as item, i}
          {#if dragState?.section === 'top' && dragState.insertBefore === i}
            <div class="drop-indicator" aria-hidden="true"></div>
          {/if}
          <div
            class="item"
            class:item-hidden={!item.visible}
            class:item-dragging={dragState?.section === 'top' && dragState.fromIndex === i}
            data-drag-idx={i}
          >
            <!-- Drag handle -->
            <button
              class="drag-handle"
              class:drag-locked={item.mandatory}
              onmousedown={(e) => !item.mandatory && startDrag(e, 'top', i)}
              disabled={item.mandatory}
              use:tooltip={item.mandatory ? 'Locked — cannot be reordered' : 'Drag to reorder'}
              aria-label={item.mandatory ? 'Locked' : 'Drag to reorder'}
            >
              <GripVertical size={14} />
            </button>

            <!-- Icon -->
            <span class="item-icon">
              {#if BUILTIN_ICONS[item.id]}
                {@const IconComp = BUILTIN_ICONS[item.id] as any}
                <IconComp size={16} />
              {:else}
                <Zap size={16} />
              {/if}
            </span>

            <!-- Label -->
            <span class="item-label">{item.label}</span>

            <!-- Visibility toggle -->
            {#if item.mandatory}
              <span class="lock-icon" use:tooltip={'Always visible'}><Lock size={13} /></span>
            {:else}
              <button
                class="vis-btn"
                class:vis-hidden={!item.visible}
                onclick={() => toggleVisibility('top', i)}
                use:tooltip={item.visible ? 'Hide' : 'Show'}
                aria-label={item.visible ? 'Hide item' : 'Show item'}
              >
                {#if item.visible}
                  <Eye size={14} />
                {:else}
                  <EyeOff size={14} />
                {/if}
              </button>
            {/if}
          </div>
        {/each}
        {#if dragState?.section === 'top' && dragState.insertBefore === topItems.length}
          <div class="drop-indicator" aria-hidden="true"></div>
        {/if}
      </div>
    </div>

    <!-- Bottom section -->
    <div class="section">
      <div class="section-label">
        <span>Panel</span>
        <span class="section-label-hint">bottom dock (stage, diff, terminal…)</span>
      </div>
      <div class="item-list" bind:this={bottomListEl}>
        {#each bottomItems as item, i}
          {#if dragState?.section === 'bottom' && dragState.insertBefore === i}
            <div class="drop-indicator" aria-hidden="true"></div>
          {/if}
          <div
            class="item"
            class:item-hidden={!item.visible}
            class:item-dragging={dragState?.section === 'bottom' && dragState.fromIndex === i}
            data-drag-idx={i}
          >
            <button
              class="drag-handle"
              class:drag-locked={item.mandatory}
              onmousedown={(e) => !item.mandatory && startDrag(e, 'bottom', i)}
              disabled={item.mandatory}
              use:tooltip={item.mandatory ? 'Locked — cannot be reordered' : 'Drag to reorder'}
              aria-label={item.mandatory ? 'Locked' : 'Drag to reorder'}
            >
              <GripVertical size={14} />
            </button>

            <span class="item-icon">
              {#if item.kind === 'builtin' && BUILTIN_ICONS[item.id]}
                {@const IconComp = BUILTIN_ICONS[item.id] as any}
                <IconComp size={16} />
              {:else}
                {@const pIcon = pluginIconFor(item.id)}
                {#if pIcon && isEmojiIcon(pIcon)}
                  <span class="emoji-icon">{pIcon}</span>
                {:else}
                  <Zap size={16} />
                {/if}
              {/if}
            </span>

            <span class="item-label">
              {item.kind === 'plugin' ? pluginLabelFor(item.id) : item.label}
            </span>

            {#if item.mandatory}
              <span class="lock-icon" use:tooltip={'Always visible'}><Lock size={13} /></span>
            {:else}
              <button
                class="vis-btn"
                class:vis-hidden={!item.visible}
                onclick={() => toggleVisibility('bottom', i)}
                use:tooltip={item.visible ? 'Hide' : 'Show'}
                aria-label={item.visible ? 'Hide item' : 'Show item'}
              >
                {#if item.visible}
                  <Eye size={14} />
                {:else}
                  <EyeOff size={14} />
                {/if}
              </button>
            {/if}
          </div>
        {/each}
        {#if dragState?.section === 'bottom' && dragState.insertBefore === bottomItems.length}
          <div class="drop-indicator" aria-hidden="true"></div>
        {/if}
      </div>
    </div>
  </div>

  </div>

  {#snippet footer()}
    <Button
      variant="ghost"
      onclick={resetCurrentTab}
      title={`Restore default order on the ${activeTab === 'left' ? 'Left' : 'Right'} bar`}
    >
      {#snippet iconStart()}<RotateCcw size={14} />{/snippet}
      Restore defaults
    </Button>
    <span class="footer-spacer"></span>
    <Button variant="secondary" onclick={onClose}>Cancel</Button>
    <Button variant="primary" onclick={handleSave} disabled={saving} loading={saving}>
      {saving ? 'Saving…' : 'Save'}
    </Button>
  {/snippet}
</Modal>

<style>
  .cab-body {
    height: 100%;
    display: flex;
    flex-direction: column;
    font-family: var(--font-ui-sans);
  }

  /* ── Tabs — full-width, centered, two equal halves.  The underline on the
     active tab uses the accent color and animates in on hover (the hover
     state is subtle so the active pair reads as the anchor). */
  .tabs {
    display: flex;
    align-items: stretch;
    width: 100%;
    background: transparent;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  /* Footer spacer — pushes Restore (left) away from Cancel/Save (right).
     The modal-footer uses justify-content: flex-end, so flex: 1 on the
     spacer eats the remaining space. */
  .footer-spacer { flex: 1; }
  .tab-btn {
    flex: 1 1 0;
    min-width: 0;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.02em;
    cursor: pointer;
    position: relative;
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .tab-btn:hover:not(.tab-active) { background: var(--bg-hover); color: var(--text-primary); }
  .tab-btn.tab-active { color: var(--accent); }
  .tab-btn.tab-active::after {
    content: '';
    position: absolute;
    left: 20%;
    right: 20%;
    bottom: -1px;
    height: 2px;
    background: var(--accent);
    border-radius: 2px 2px 0 0;
  }

  /* ── Content ───────────────────────────────────────────────────────────────── */
  .cab-content {
    flex: 1;
    overflow-y: auto;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .hint {
    font-size: 11px;
    color: var(--text-muted);
    margin: 0;
    line-height: 1.6;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }
  :global(.hint-lock) {
    color: var(--text-disabled);
    vertical-align: -1px;
  }

  /* ── Section ───────────────────────────────────────────────────────────────── */
  .section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .section-label {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--text-muted);
    padding: 0 2px;
    margin-bottom: 2px;
  }
  .section-label-hint {
    font-weight: 400;
    letter-spacing: 0;
    text-transform: none;
    color: var(--text-disabled);
    font-size: 10.5px;
  }

  .item-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* ── Item row ──────────────────────────────────────────────────────────────── */
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px 6px 4px;
    border-radius: var(--radius-sm);
    background: transparent;
    border: 1px solid var(--border-subtle);
    transition: background var(--transition-fast), border-color var(--transition-fast), opacity var(--transition-fast);
    user-select: none;
  }

  .item:hover {
    background: var(--bg-hover);
    border-color: var(--border);
  }

  .item.item-hidden {
    opacity: 0.45;
  }

  .item.item-dragging {
    opacity: 0.3;
    border-style: dashed;
    border-color: var(--border);
  }

  /* ── Drag handle ───────────────────────────────────────────────────────────── */
  .drag-handle {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    background: transparent;
    border: none;
    color: var(--text-disabled);
    cursor: grab;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
    padding: 0;
    transition: color var(--transition-fast);
  }
  .drag-handle:hover:not(:disabled) { color: var(--text-muted); }
  .drag-handle:active:not(:disabled) { cursor: grabbing; }
  .drag-handle.drag-locked {
    opacity: 0.25;
    cursor: not-allowed;
  }
  .drag-handle.drag-locked:hover { color: var(--text-disabled); }

  /* ── Item icon ─────────────────────────────────────────────────────────────── */
  .item-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .emoji-icon {
    font-size: 14px;
    line-height: 1;
  }

  /* ── Item label ────────────────────────────────────────────────────────────── */
  .item-label {
    flex: 1;
    font-size: 12px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  /* ── Visibility button ─────────────────────────────────────────────────────── */
  .vis-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    flex-shrink: 0;
    padding: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .vis-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .vis-btn.vis-hidden { color: var(--text-disabled); }
  .vis-btn.vis-hidden:hover { color: var(--text-muted); }

  /* ── Lock icon ─────────────────────────────────────────────────────────────── */
  .lock-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    color: var(--text-disabled);
    flex-shrink: 0;
  }

  /* ── Drop indicator ────────────────────────────────────────────────────────── */
  .drop-indicator {
    height: 2px;
    background: var(--accent);
    border-radius: 1px;
    margin: 1px 0;
    pointer-events: none;
  }

</style>
