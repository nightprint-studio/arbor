<script lang="ts">
  import { RefreshCw, ArrowUpToLine, ArrowDownToLine, Archive, GitCommitHorizontal, X, Check, Play, ChevronDown } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { pullBranch, pushBranch } from '$lib/ipc/remote';
  import { getStatus } from '$lib/ipc/stage';
  import { stashSave } from '$lib/ipc/branch';
  import { applyPostStashChange } from '$lib/utils/applyPostStashChange';
  import { getGraph, getRepoFingerprint } from '$lib/ipc/graph';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { firePluginAction, execHook } from '$lib/ipc/plugin';
  import { handlePullResult, handlePullThrown } from '$lib/utils/pullResultHandler';
  import { startPullOperation } from '$lib/utils/operations-bridge';
  import type { ActivityBarEntry, ComboOption } from '$lib/types/plugin';
  import { ACTIVITY_BAR_POINT, parseActivityBarEntry } from '$lib/contributions/activity-bar';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import { tooltipForAction } from '$lib/utils/shortcut';
  import { tooltip } from '$lib/actions/tooltip';

  const tab        = $derived(tabsStore.activeTab);
  const status     = $derived(repoStore.status);

  let isPulling   = $state(false);
  let isPushing   = $state(false);
  let stashOpen   = $state(false);
  let stashMsg    = $state('');
  let stashInputEl: HTMLInputElement | undefined = $state();
  $effect(() => { if (stashOpen) stashInputEl?.focus(); });
  let isStashing  = $state(false);

  type RepoCombo = Extract<ActivityBarEntry, { kind: 'combo' }>;

  // Combos registered with target="repo_actions" appear here instead of ActivityBar
  const repoCombos = $derived(
    contributionStore.forPoint(ACTIVITY_BAR_POINT)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseActivityBarEntry)
      .filter((e): e is RepoCombo =>
        e !== null && e.kind === 'combo' && e.target === 'repo_actions'
      )
  );

  // Run-icon visual variant: the green Play glyph reads as "execute / run";
  // every other icon (Hammer/Share2/Upload/…) is a build/export action and
  // tints accent so it doesn't compete with the green run button.
  function runTintClass(iconName: string | undefined): string {
    return (!iconName || iconName === 'Play') ? 'tint-run' : 'tint-accent';
  }

  function selectComboOption(combo: RepoCombo, opt: ComboOption) {
    // Action options (e.g. "⚙ Run settings…", "⊕ New profile…") behave like
    // the "New Workspace" footer in WorkspaceDropdown: they fire the combo's
    // run_action directly so the plugin can open its modal, and they do NOT
    // update the persisted selection — the previously selected item stays
    // visible in the run button.
    if (opt.action) {
      firePluginAction(combo.plugin_name, combo.run_action, JSON.stringify({ value: opt.value, label: opt.label })).catch(() => {});
      return;
    }
    pluginStore.setComboSelection(combo.plugin_name, combo.id, opt.value);
    if (combo.select_action) {
      firePluginAction(combo.plugin_name, combo.select_action, JSON.stringify({ value: opt.value, label: opt.label })).catch(() => {});
    }
  }

  async function runCombo(combo: RepoCombo) {
    const value = pluginStore.getComboSelection(combo.plugin_name, combo.id);
    const opt   = combo.options.find(o => o.value === value);
    const label = opt?.label ?? value;
    try {
      await firePluginAction(combo.plugin_name, combo.run_action, JSON.stringify({ value, label }));
    } catch { /* ignore */ }
  }

  function comboDropdownItems(combo: RepoCombo): DropdownItem[] {
    const selValue = pluginStore.getComboSelection(combo.plugin_name, combo.id);

    // Bucket options by group, preserving first-seen order so the visual
    // sequence matches the plugin's option list. `null` = ungrouped (no
    // header rendered for that bucket; named buckets always render a header,
    // even with a single group — the plugin's structural choice is honoured).
    type Bucket = { group: string | null; items: Extract<DropdownItem, { kind: 'item' }>[] };
    const buckets: Bucket[] = [];
    for (const opt of combo.options) {
      if (opt.action) continue;
      const grp = opt.group ?? null;
      let b = buckets.find(x => x.group === grp);
      if (!b) { b = { group: grp, items: [] }; buckets.push(b); }
      b.items.push({
        kind:    'item',
        id:      opt.value,
        label:   opt.label,
        active:  opt.value === selValue,
        onclick: () => selectComboOption(combo, opt),
      });
    }

    if (buckets.length === 0) {
      // Empty marker — Dropdown's emptyMessage only fires when items=[];
      // we synthesize a disabled placeholder so the user gets a hint even
      // when there are still action options present.
      return [{
        kind: 'item', id: '__empty', label: 'No configurations — open a project first',
        disabled: true, onclick: () => {},
      }];
    }

    const out: DropdownItem[] = [];
    for (const b of buckets) {
      if (b.group) {
        out.push({ kind: 'group', id: `g:${b.group}`, label: b.group, items: b.items });
      } else {
        out.push(...b.items);
      }
    }
    return out;
  }

  function actionComboOptions(combo: RepoCombo): ComboOption[] {
    return combo.options.filter(o => o.action);
  }

  // ── Tab change → fire on_repo_open on all plugins ────────────────────────
  $effect(() => {
    const tab = tabsStore.activeTab;
    if (!tab) return;
    // Fire on_repo_open so plugins refresh their state for the new repo.
    // The Rust fire_hook handler also sets __arbor_current_repo__ for this hook name.
    execHook('on_repo_open', JSON.stringify({ path: tab.path })).catch(() => {});
  });


  const changeCount = $derived(
    (status?.staged.length ?? 0) +
    (status?.unstaged.length ?? 0) +
    (status?.untracked.length ?? 0)
  );

  async function refreshGraph() {
    if (!tab) return;
    try {
      repoStore.setRefreshing(true);
      const gd = await getGraph(tab.id, 0, 500);
      graphStore.setGraph(gd);
      graphStore.refresh(); // triggers Sidebar effect → reloads branches with updated ahead/behind
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      repoStore.setRefreshing(false);
    }
  }

  async function pull() {
    if (!tab || isPulling) return;
    // Detached HEAD has no upstream → `git pull origin` errors out.  Check
    // `is_detached` explicitly: in detached HEAD `current_branch` is still
    // populated (with the abbreviated SHA from `head.shorthand()`), so a
    // bare `!current_branch` check would let the call through.
    if (!status?.current_branch || status.is_detached) {
      uiStore.showToast(
        'Pull non disponibile in detached HEAD — fai il checkout di un branch',
        'warning',
      );
      return;
    }
    isPulling = true;

    // Pre-create the OperationsOverlay card so the progress is visible the
    // instant the user clicks Pull.  The backend emits phase events keyed
    // by this opId; the operations bridge listens globally and routes them
    // here.  We use a `pull-{tab}-{ts}` id so retries get separate cards.
    const opId = `pull-${tab.id}-${Date.now()}`;
    const branchLabel = status?.current_branch ?? null;
    startPullOperation(opId, tab.name ?? 'Repository', branchLabel);

    try {
      // Snapshot the ref state before the pull so we can detect a no-op
      // (already up-to-date) and skip the costly `getGraph` reload that
      // refreshGraph() does — the lane assignment over the whole history
      // is wasted work when nothing actually moved.
      const beforeFp = await getRepoFingerprint(tab.id).catch(() => null);

      const result = await pullBranch(tab.id, 'origin', opId);
      const s = await getStatus(tab.id);
      repoStore.setStatus(s);

      const afterFp = await getRepoFingerprint(tab.id).catch(() => null);
      const noChange = beforeFp !== null && afterFp !== null && beforeFp === afterFp;

      if (noChange) {
        // Refs unchanged → graph topology unchanged.  Still do the light
        // post-stash refresh: the auto-stash dance (push then pop) leaves
        // the stash list in its original state on success, but if anything
        // odd happened mid-flow this catches it.  No `getGraph` round-trip.
        await applyPostStashChange(tab.id);
      } else {
        // Refresh graph + sidebar so the new stash (if any) appears in the
        // Stashes section even when a later error path returns early.
        await refreshGraph();
      }

      // All the stash-conflict / stash-apply-error / pull-error branching
      // lives in the shared util so every entry point (sidebar, palette,
      // …) behaves identically. `handlePullResult` returns true only on a
      // clean pull.
      const clean = handlePullResult(result, {
        workdirConflicts: s.conflicted.map(f => f.path),
        status: s,
      });
      if (clean) {
        uiStore.showToast(noChange ? 'Already up to date' : 'Pull successful', noChange ? 'info' : 'success');
      }
    } catch (err) {
      // The backend throws (rejects the IPC call) when the pull fails with
      // no stash context — e.g. a dangling CHERRY_PICK_HEAD.  Probe the
      // status and route into the conflict modal before falling back to a
      // toast.
      const st = await getStatus(tab.id).catch(() => null);
      if (st) repoStore.setStatus(st);
      if (!handlePullThrown(err, st)) {
        uiStore.showToast(`Pull failed: ${err}`, 'error');
      }
    } finally { isPulling = false; }
  }

  async function push() {
    if (!tab || isPushing) return;
    if (!status?.current_branch || status.is_detached) {
      uiStore.showToast(
        'Push non disponibile in detached HEAD — fai il checkout di un branch',
        'warning',
      );
      return;
    }
    isPushing = true;
    try {
      await pushBranch(tab.id, 'origin', `refs/heads/${status.current_branch}`);
      uiStore.showToast('Push successful', 'success');
      const [, s] = await Promise.all([refreshGraph(), getStatus(tab.id)]);
      repoStore.setStatus(s);
    } catch (err) {
      uiStore.showToast(`Push failed: ${err}`, 'error');
    } finally { isPushing = false; }
  }

  function openStageArea() {
    uiStore.toggleBottomSection('stage');
  }

  function openStashDialog() {
    if (!tab) return;
    stashMsg = '';
    stashOpen = true;
  }

  async function confirmStash() {
    if (!tab || isStashing) return;
    isStashing = true;
    try {
      await stashSave(tab.id, stashMsg.trim() || undefined, true);
      uiStore.showToast('Changes stashed', 'success');
      stashOpen = false;
      stashMsg  = '';
      // Light refresh — stash save doesn't change graph topology, only the
      // stash list + working dir state.  Skips the costly getGraph call.
      await applyPostStashChange(tab.id);
    } catch (err) {
      uiStore.showToast(`Stash failed: ${err}`, 'error');
    } finally {
      isStashing = false;
    }
  }

  function cancelStash() {
    stashOpen = false;
    stashMsg = '';
  }

  function onStashKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); confirmStash(); }
    if (e.key === 'Escape') { e.stopPropagation(); cancelStash(); }
  }

  // Listen for global keybinding events dispatched from AppShell
  $effect(() => {
    const onPull  = () => void pull();
    const onPush  = () => void push();
    const onStash = () => openStashDialog();
    window.addEventListener('arbor:pull',  onPull);
    window.addEventListener('arbor:push',  onPush);
    window.addEventListener('arbor:stash', onStash);
    return () => {
      window.removeEventListener('arbor:pull',  onPull);
      window.removeEventListener('arbor:push',  onPush);
      window.removeEventListener('arbor:stash', onStash);
    };
  });
</script>

<div class="repo-actions">
  <div class="action-row">
    <!-- Refresh -->
    <button class="action-btn" use:tooltip={'Refresh graph'} disabled={!tab || repoStore.isRefreshing} onclick={refreshGraph}>
      <RefreshCw size={13} class={repoStore.isRefreshing ? 'spinning' : ''} />
    </button>

    <div class="sep"></div>

    <!-- Stage & Commit -->
    <button
      class="action-btn stage-btn"
      class:active={uiStore.activeBottomSection === 'stage'}
      class:has-changes={changeCount > 0}
      use:tooltip={tooltipForAction('Stage & Commit', 'stage_view')}
      disabled={!tab}
      onclick={openStageArea}
    >
      <GitCommitHorizontal size={13} />
      {#if changeCount > 0}
        <span class="changes-pill">{changeCount}</span>
      {/if}
    </button>

    <div class="sep"></div>

    <!-- Pull -->
    <button
      class="action-btn pull-btn"
      class:has-behind={status?.behind && status.behind > 0}
      use:tooltip={!status?.current_branch || status.is_detached
        ? 'Pull non disponibile in detached HEAD — fai il checkout di un branch'
        : 'Pull'}
      disabled={!tab || isPulling || !status?.current_branch || status.is_detached}
      onclick={pull}
    >
      <ArrowDownToLine size={13} class={isPulling ? 'spinning' : ''} />
      {#if status?.behind && status.behind > 0}
        <span class="badge behind">{status.behind}</span>
      {/if}
    </button>

    <!-- Push -->
    <button
      class="action-btn push-btn"
      class:has-ahead={status?.ahead && status.ahead > 0}
      use:tooltip={!status?.current_branch || status.is_detached
        ? 'Push non disponibile in detached HEAD — fai il checkout di un branch'
        : 'Push'}
      disabled={!tab || isPushing || !status?.current_branch || status.is_detached}
      onclick={push}
    >
      <ArrowUpToLine size={13} class={isPushing ? 'spinning' : ''} />
      {#if status?.ahead && status.ahead > 0}
        <span class="badge ahead">{status.ahead}</span>
      {/if}
    </button>

    <div class="sep"></div>

    <!-- Stash -->
    <button class="action-btn" use:tooltip={'Stash changes…'} disabled={!tab || changeCount === 0} onclick={openStashDialog}>
      <Archive size={13} />
    </button>

    <!-- RepoActions combos (plugins with target="repo_actions") -->
    {#if repoCombos.length > 0}
      <div class="sep"></div>
      {#each repoCombos as combo (combo.plugin_name + combo.id)}
        {@const selValue   = pluginStore.getComboSelection(combo.plugin_name, combo.id)}
        {@const selOpt     = combo.options.find(o => o.value === selValue)}
        {@const selLabel   = selOpt?.label ?? '—'}
        {@const ddItems    = comboDropdownItems(combo)}
        {@const ddActions  = actionComboOptions(combo)}
        {@const RunIcon    = PLUGIN_ICONS[combo.run_icon ?? 'Play'] ?? Play}
        {#if combo.variant === 'profile'}
          <!-- Profile pill: colored chip, opens the same dropdown via Dropdown widget -->
          <Dropdown
            position="fixed"
            direction="down"
            items={ddItems}
            showFooter={ddActions.length > 0}
            class="profile-pill-root"
          >
            {#snippet trigger({ open, toggle })}
              <button
                class="profile-pill"
                class:pill-dev={selOpt?.color === 'dev'}
                class:pill-prod={selOpt?.color === 'prod'}
                class:pill-test={selOpt?.color === 'test'}
                class:combo-open={open}
                use:tooltip={combo.tooltip ?? 'Active build profile'}
                disabled={!tab}
                onclick={toggle}
              >
                <span class="pill-dot"></span>
                <span class="pill-label">{selLabel}</span>
                <ChevronDown size={9} />
              </button>
            {/snippet}
            {#snippet footer({ close })}
              {#each ddActions as opt}
                <button class="dd-footer-action" onclick={() => { close(); selectComboOption(combo, opt); }}>
                  {opt.label}
                </button>
              {/each}
            {/snippet}
          </Dropdown>
        {:else}
          <Dropdown
            position="fixed"
            direction="down"
            items={ddItems}
            showFooter={ddActions.length > 0}
            class="repo-combo-root {runTintClass(combo.run_icon)}"
          >
            {#snippet trigger({ open, toggle, close })}
              <div class="repo-combo" class:combo-open={open}>
                <button
                  class="action-btn combo-run-btn"
                  use:tooltip={combo.tooltip ?? `Run: ${selLabel}`}
                  disabled={!tab}
                  onclick={() => { close(); runCombo(combo); }}
                >
                  <RunIcon size={12} />
                </button>
                <button
                  class="action-btn combo-sel-btn"
                  use:tooltip={'Select configuration'}
                  disabled={!tab}
                  onclick={toggle}
                >
                  <ChevronDown size={10} />
                </button>
              </div>
            {/snippet}
            {#snippet footer({ close })}
              {#each ddActions as opt}
                <button class="dd-footer-action" onclick={() => { close(); selectComboOption(combo, opt); }}>
                  {opt.label}
                </button>
              {/each}
            {/snippet}
          </Dropdown>
        {/if}
      {/each}
    {/if}

  </div>

  <!-- Stash dialog (inline dropdown) -->
  {#if stashOpen}
    <div class="stash-dialog" role="dialog" aria-label="Stash changes">
      <p class="stash-label">Stash message <span class="stash-opt">(optional)</span></p>
      <input
        class="stash-input"
        type="text"
        placeholder="WIP on {status?.current_branch ?? 'branch'}…"
        bind:value={stashMsg}
        onkeydown={onStashKeydown}
        bind:this={stashInputEl}
      />
      <div class="stash-actions">
        <button class="stash-btn cancel" onclick={cancelStash} disabled={isStashing}>
          <X size={11} /> Cancel
        </button>
        <button class="stash-btn confirm" onclick={confirmStash} disabled={isStashing}>
          <Check size={11} /> {isStashing ? 'Stashing…' : 'Stash All'}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .repo-actions {
    padding: 5px 8px;
    /* border-bottom: 1px solid var(--border); */
    position: relative;
  }

  .action-row {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .action-btn {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 24px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    flex-shrink: 0;
  }
  .action-btn:hover:not(:disabled) { background: rgba(255,255,255,0.06); color: var(--text-primary); }
  .action-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  /* Color-coded remote buttons */
  .pull-btn.has-behind {
    color: var(--warning);
  }
  .pull-btn.has-behind:hover:not(:disabled) {
    color: var(--warning);
    background: rgba(226,163,53,0.12);
  }

  .push-btn.has-ahead {
    color: var(--success);
  }
  .push-btn.has-ahead:hover:not(:disabled) {
    color: var(--success);
    background: rgba(95,173,86,0.12);
  }

  /* Stage & commit button: auto-width to accommodate inline pill */
  .stage-btn {
    width: auto;
    padding: 0 6px;
    gap: 4px;
  }
  .stage-btn.active {
    background: var(--accent-subtle);
    color: var(--accent);
  }
  .stage-btn:not(:disabled):not(.active):hover {
    background: var(--accent-subtle);
    color: var(--accent);
  }

  /* Inline pill — replaces the absolute badge for the stage button */
  .changes-pill {
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    background: var(--accent);
    color: var(--text-on-accent);
    border-radius: 999px;
    padding: 1px 5px;
    min-width: 14px;
    text-align: center;
    pointer-events: none;
    flex-shrink: 0;
  }
  .stage-btn.active .changes-pill {
    background: var(--accent);
    color: var(--text-on-accent);
  }

  .sep {
    width: 1px; height: 16px;
    background: var(--border);
    margin: 0 2px;
    flex-shrink: 0;
  }

  .badge {
    position: absolute;
    top: 1px; right: 1px;
    min-width: 14px; height: 14px;
    border-radius: 999px;
    font-size: 9px; font-weight: 600;
    display: flex; align-items: center; justify-content: center;
    padding: 0 2px; line-height: 1;
    pointer-events: none;
  }
  .badge.ahead   { background: var(--success); color: var(--text-on-accent); }
  .badge.behind  { background: var(--warning); color: var(--text-on-accent); }

  /* ── Stash dropdown ── */
  .stash-dialog {
    position: absolute;
    top: calc(100% + 4px);
    left: 8px;
    right: 8px;
    z-index: var(--z-sticky);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 10px 8px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    animation: dropIn 120ms cubic-bezier(0.16,1,0.3,1);
  }

  @keyframes dropIn {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .stash-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    margin: 0 0 6px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }
  .stash-opt {
    font-weight: 400;
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-muted);
  }

  .stash-input {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    padding: 5px 8px;
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .stash-input:focus { border-color: var(--accent); }

  .stash-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 8px;
  }

  .stash-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .stash-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .stash-btn.cancel {
    background: transparent;
    color: var(--text-muted);
  }
  .stash-btn.cancel:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .stash-btn.confirm {
    background: var(--accent);
    color: var(--text-on-accent);
    border-color: var(--accent);
  }
  .stash-btn.confirm:hover:not(:disabled) {
    background: var(--accent-hover, #3b5fc0);
  }

  :global(.spinning) { animation: spin 1s linear infinite; }

  /* ── RepoAction combo widget (JetBrains run style) ──
     The dropdown chrome is provided by the shared <Dropdown> widget; only the
     split-button trigger (run + chevron) is styled here. */
  .repo-combo {
    display: flex;
    align-items: center;
    border-radius: var(--radius-sm);
    overflow: hidden;
    border: 1px solid var(--border);
    transition: border-color var(--transition-fast);
  }
  .repo-combo:hover:not(:has(:disabled)) { border-color: var(--accent); }
  .repo-combo.combo-open { border-color: var(--accent); background: var(--accent-subtle); }

  .combo-run-btn {
    width: 26px !important;
    height: 22px !important;
    border-radius: 0 !important;
    border: none !important;
    border-right: 1px solid var(--border) !important;
  }
  /* Run-icon tint variants — class is applied to the Dropdown root so it
     wraps both buttons and the open menu. */
  :global(.repo-combo-root.tint-run .combo-run-btn) {
    color: var(--success) !important;
  }
  :global(.repo-combo-root.tint-run .combo-run-btn:hover:not(:disabled)) {
    background: rgba(95, 173, 86, 0.12) !important;
    color: var(--success) !important;
  }
  :global(.repo-combo-root.tint-accent .combo-run-btn) {
    color: var(--accent) !important;
  }
  :global(.repo-combo-root.tint-accent .combo-run-btn:hover:not(:disabled)) {
    background: var(--accent-subtle) !important;
    color: var(--accent) !important;
  }

  .combo-sel-btn {
    width: 16px !important;
    height: 22px !important;
    border-radius: 0 !important;
    border: none !important;
    padding: 0 !important;
    min-width: unset !important;
  }

  /* Footer action buttons rendered inside the shared Dropdown's footer slot —
     match the muted-then-hover-accent style from the previous custom menu. */
  :global(.repo-combo-root .dd-footer .dd-footer-action),
  :global(.profile-pill-root .dd-footer .dd-footer-action) {
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
  :global(.repo-combo-root .dd-footer .dd-footer-action:hover),
  :global(.profile-pill-root .dd-footer .dd-footer-action:hover) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  /* ── Profile pill badge ── */
  .profile-pill {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 7px 0 5px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-elevated);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
    white-space: nowrap;
  }
  .profile-pill:hover:not(:disabled),
  .profile-pill.combo-open {
    border-color: var(--accent);
    background: var(--accent-subtle);
    color: var(--accent);
  }
  .profile-pill:disabled { opacity: 0.4; cursor: not-allowed; }

  .pill-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-muted);
    flex-shrink: 0;
    transition: background var(--transition-fast);
  }

  /* Semantic profile colors */
  .profile-pill.pill-dev .pill-dot  { background: var(--success); }
  .profile-pill.pill-prod .pill-dot { background: var(--error, #f87171); }
  .profile-pill.pill-test .pill-dot { background: var(--accent); }

  .profile-pill.pill-dev  { --pill-color: var(--success); }
  .profile-pill.pill-prod { --pill-color: var(--error, #f87171); }
  .profile-pill.pill-test { --pill-color: var(--accent); }

  .profile-pill.pill-dev:hover:not(:disabled),
  .profile-pill.pill-dev.combo-open {
    border-color: var(--success);
    background: rgba(95, 173, 86, 0.12);
    color: var(--success);
  }
  .profile-pill.pill-prod:hover:not(:disabled),
  .profile-pill.pill-prod.combo-open {
    border-color: var(--error, #f87171);
    background: rgba(248, 113, 113, 0.12);
    color: var(--error, #f87171);
  }
  .profile-pill.pill-test:hover:not(:disabled),
  .profile-pill.pill-test.combo-open {
    border-color: var(--accent);
    background: var(--accent-subtle);
    color: var(--accent);
  }

  .pill-label { line-height: 1; }
</style>
