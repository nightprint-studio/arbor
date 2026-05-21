<script lang="ts">
  import {
    GitBranch, GitMerge, Tag, Rocket, Zap, Plus,
    RefreshCw, X, Search, GitPullRequest, TicketCheck,
    AlertTriangle,
  } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import type { GitFlowConfig, GitFlowStatus } from '$lib/types/git';
  import type { Issue } from '$lib/types/issues';
  import {
    gitFlowGetStatus, gitFlowInit, gitFlowInitCreateMain,
    gitFlowFeatureStart, gitFlowFeatureFinish,
    gitFlowReleaseStart, gitFlowReleaseFinish,
    gitFlowHotfixStart, gitFlowHotfixFinish,
    getGitFlowConfig,
  } from '$lib/ipc/gitflow';
  import { getRepoConfig } from '$lib/ipc/config';
  import TicketPickerModal from '$lib/components/shared/TicketPickerModal.svelte';
  import CreateMrModal from '$lib/components/mr/CreateMrModal.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SplitButton from '$lib/components/shared/ui/SplitButton.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import FlowBranchSection from './FlowBranchSection.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  const tab = $derived(tabsStore.activeTab);

  // ── State ──────────────────────────────────────────────────────────────────
  let status       = $state<GitFlowStatus | null>(null);
  let config       = $state<GitFlowConfig | null>(null);
  let issueTracker = $state<string | undefined>(undefined);
  let loading      = $state(false);
  let busy         = $state(false);
  let error        = $state<string | null>(null);

  let pendingPr = $state<{ sourceBranch: string; targetBranch: string; title: string } | null>(null);

  let ticketModalOpen = $state(false);
  let ticketSelected  = $state<Issue | null>(null);

  /** 'normal' = merge locally, 'pr' = open PR/MR. */
  let submitAction = $state<'normal' | 'pr'>('normal');

  let createMainDialog = $state<{ show: boolean; branchName: string; fromInitial: boolean }>({
    show: false,
    branchName: 'main',
    fromInitial: false,
  });

  let noDevelopPrompt = $state<{
    show:       boolean;
    kind:       'feature' | 'release' | null;
    baseBranch: string;
  }>({ show: false, kind: null, baseBranch: '' });

  const ackStorageKey = (path: string) => `arbor:gitflow-no-develop-ack:${path}`;

  function hasNoDevelopAck(): boolean {
    if (!tab) return true;
    try { return localStorage.getItem(ackStorageKey(tab.path)) === '1'; }
    catch { return true; }
  }

  function setNoDevelopAck() {
    if (!tab) return;
    try { localStorage.setItem(ackStorageKey(tab.path), '1'); } catch {}
  }

  // ── Inline form state ──────────────────────────────────────────────────────
  type FormKind = 'feature_start' | 'release_start' | 'hotfix_start'
                | 'feature_finish' | 'release_finish' | 'hotfix_finish';
  let openForm = $state<FormKind | null>(null);
  let formName = $state('');
  let formTag  = $state('');

  const showTicketPicker = $derived(
    openForm === 'feature_start' && !!issueTracker
  );
  const ticketPickerUnavailable = $derived(
    openForm === 'feature_start' &&
    config?.require_ticket_branch === true &&
    !issueTracker
  );
  const isForcedPr = $derived(
    (openForm === 'feature_finish' && !!config?.finish.feature_use_pr) ||
    (openForm === 'release_finish' && !!config?.finish.release_use_pr) ||
    (openForm === 'hotfix_finish'  && !!config?.finish.hotfix_use_pr)
  );

  // ── Load data ──────────────────────────────────────────────────────────────
  $effect(() => {
    if (tab) load(tab.id);
    else { status = null; config = null; issueTracker = undefined; }
  });

  async function load(tabId: string) {
    loading = true;
    error   = null;
    try {
      const [s, c, rc] = await Promise.all([
        gitFlowGetStatus(tabId),
        getGitFlowConfig(tabId),
        getRepoConfig(tabId).catch(() => null),
      ]);
      status       = s;
      config       = c;
      issueTracker = rc?.issue_tracker ?? undefined;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // ── Actions ────────────────────────────────────────────────────────────────

  async function initFlow() {
    if (!tab) return;
    busy = true;
    try {
      await gitFlowInit(tab.id);
      await load(tab.id);
      graphStore.refresh();
      uiStore.showToast('Git Flow initialised', 'success');
    } catch (e) {
      const msg = String(e);
      const match = msg.match(/main branch '([^']+)' not found/);
      if (match) {
        createMainDialog = { show: true, branchName: match[1], fromInitial: false };
      } else {
        uiStore.showToast(msg, 'error');
      }
    } finally {
      busy = false;
    }
  }

  async function confirmCreateMain() {
    if (!tab) return;
    busy = true;
    try {
      await gitFlowInitCreateMain(tab.id, createMainDialog.fromInitial);
      createMainDialog = { show: false, branchName: 'main', fromInitial: false };
      await load(tab.id);
      graphStore.refresh();
      uiStore.showToast('Git Flow initialised', 'success');
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    } finally {
      busy = false;
    }
  }

  function dismissCreateMain() {
    createMainDialog = { show: false, branchName: 'main', fromInitial: false };
  }

  async function submitForm(overrideAction?: 'normal' | 'pr') {
    if (!tab || !openForm) return;
    const name = formName.trim();
    if (!name) { uiStore.showToast('Name is required', 'warning'); return; }

    if (openForm === 'feature_start' && config?.require_ticket_branch && issueTracker && !ticketSelected) {
      uiStore.showToast('Select a ticket to name the branch', 'warning');
      return;
    }

    if ((openForm === 'feature_start' || openForm === 'release_start')
        && status && !status.develop_exists && status.main_exists
        && !hasNoDevelopAck()) {
      noDevelopPrompt = {
        show: true,
        kind: openForm === 'feature_start' ? 'feature' : 'release',
        baseBranch: config?.main_branch ?? 'main',
      };
      return;
    }

    const action = overrideAction ?? submitAction;
    const forcePr = action === 'pr';

    busy = true;
    const tabId = tab.id;
    try {
      switch (openForm) {
        case 'feature_start': {
          const r = await gitFlowFeatureStart(tabId, name);
          if (r.fell_back_to_main) {
            uiStore.showToast(`feature '${name}' started from ${r.base_branch}`, 'info');
          } else {
            uiStore.showToast(`feature '${name}' started`, 'success');
          }
          closeForm();
          await load(tabId); graphStore.refresh();
          break;
        }
        case 'feature_finish': {
          const result = await gitFlowFeatureFinish(tabId, name, forcePr);
          if (result.action === 'create_pr') {
            closeForm();
            pendingPr = {
              sourceBranch: result.source_branch,
              targetBranch: result.target_branch,
              title: `Merge feature '${name}' into ${result.target_branch}`,
            };
            await load(tabId); graphStore.refresh();
          } else {
            uiStore.showToast(`feature '${name}' finished`, 'success');
            closeForm(); await load(tabId); graphStore.refresh();
          }
          break;
        }
        case 'release_start': {
          const r = await gitFlowReleaseStart(tabId, name);
          if (r.fell_back_to_main) {
            uiStore.showToast(`release '${name}' started from ${r.base_branch}`, 'info');
          } else {
            uiStore.showToast(`release '${name}' started`, 'success');
          }
          closeForm(); await load(tabId); graphStore.refresh();
          break;
        }
        case 'release_finish': {
          const result = await gitFlowReleaseFinish(tabId, name, formTag, forcePr);
          if (result.action === 'create_pr') {
            closeForm();
            pendingPr = {
              sourceBranch: result.source_branch,
              targetBranch: result.target_branch,
              title: `Release ${name}`,
            };
            await load(tabId); graphStore.refresh();
          } else {
            uiStore.showToast(`release '${name}' finished`, 'success');
            closeForm(); await load(tabId); graphStore.refresh();
          }
          break;
        }
        case 'hotfix_start': {
          await gitFlowHotfixStart(tabId, name);
          uiStore.showToast(`hotfix '${name}' started`, 'success');
          closeForm(); await load(tabId); graphStore.refresh();
          break;
        }

        case 'hotfix_finish': {
          const result = await gitFlowHotfixFinish(tabId, name, formTag, forcePr);
          if (result.action === 'create_pr') {
            closeForm();
            pendingPr = {
              sourceBranch: result.source_branch,
              targetBranch: result.target_branch,
              title: `Hotfix ${name}`,
            };
            await load(tabId); graphStore.refresh();
          } else {
            uiStore.showToast(`hotfix '${name}' finished`, 'success');
            closeForm(); await load(tabId); graphStore.refresh();
          }
          break;
        }
      }
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    } finally {
      busy = false;
    }
  }

  function openFormFor(kind: FormKind, name = '') {
    openForm        = kind;
    formName        = name;
    formTag         = '';
    ticketModalOpen = false;
    ticketSelected  = null;
    if (kind === 'feature_finish') submitAction = config?.finish.feature_pr_default ? 'pr' : 'normal';
    else if (kind === 'release_finish') submitAction = config?.finish.release_pr_default ? 'pr' : 'normal';
    else if (kind === 'hotfix_finish')  submitAction = config?.finish.hotfix_pr_default  ? 'pr' : 'normal';
    else submitAction = 'normal';
  }

  function closeForm() {
    openForm        = null;
    formName        = '';
    formTag         = '';
    ticketModalOpen = false;
    ticketSelected  = null;
  }

  function confirmNoDevelop() {
    setNoDevelopAck();
    noDevelopPrompt = { show: false, kind: null, baseBranch: '' };
    submitForm();
  }

  function dismissNoDevelop() {
    noDevelopPrompt = { show: false, kind: null, baseBranch: '' };
  }

  async function initFromBanner() {
    if (!tab || !status) return;
    if (!status.main_exists) return;
    await initFlow();
  }

  async function quickFinish(
    kind: 'feature' | 'release' | 'hotfix',
    name: string,
    forcePr: boolean,
  ) {
    if (!tab) return;
    busy = true;
    const tabId = tab.id;
    try {
      let result;
      if (kind === 'feature') {
        result = await gitFlowFeatureFinish(tabId, name, forcePr);
      } else if (kind === 'release') {
        result = await gitFlowReleaseFinish(tabId, name, '', forcePr);
      } else {
        result = await gitFlowHotfixFinish(tabId, name, '', forcePr);
      }
      if (result.action === 'create_pr') {
        const titles: Record<string, string> = {
          feature: `Merge feature '${name}' into ${result.target_branch}`,
          release: `Release ${name}`,
          hotfix:  `Hotfix ${name}`,
        };
        pendingPr = {
          sourceBranch: result.source_branch,
          targetBranch: result.target_branch,
          title: titles[kind],
        };
        await load(tabId); graphStore.refresh();
      } else {
        const msgs: Record<string, string> = {
          feature: `feature '${name}' finished`,
          release: `release '${name}' finished`,
          hotfix:  `hotfix '${name}' finished`,
        };
        uiStore.showToast(msgs[kind], 'success');
        await load(tabId); graphStore.refresh();
      }
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    } finally {
      busy = false;
    }
  }

  function onTicketSelected(issue: Issue) {
    ticketSelected  = issue;
    formName        = issue.identifier;
    ticketModalOpen = false;
  }

  function needsTagMsg(kind: FormKind) {
    return kind === 'release_finish' || kind === 'hotfix_finish';
  }

  function formTitle(kind: FormKind) {
    const map: Record<FormKind, string> = {
      feature_start: 'Start Feature', feature_finish: 'Finish Feature',
      release_start: 'Start Release', release_finish: 'Finish Release',
      hotfix_start:  'Start Hotfix',  hotfix_finish:  'Finish Hotfix',
    };
    return map[kind];
  }

  function formPlaceholder(kind: FormKind) {
    if (kind === 'release_start' || kind === 'release_finish') return 'e.g. 1.2.0';
    return 'e.g. my-feature';
  }
</script>

<div class="gitflow-panel">
  <!-- ── Header ── -->
  <div class="panel-header">
    <span class="header-icon"><GitMerge size={14} /></span>
    <span class="header-title">Git Flow</span>
    <Button
      variant="icon"
      size="xs"
      title="Refresh"
      disabled={loading}
      onclick={() => tab && load(tab.id)}
      ariaLabel="Refresh"
    >
      {#snippet iconStart()}
        <RefreshCw size={11} class={loading ? 'spin' : ''} />
      {/snippet}
    </Button>
  </div>

  {#if !tab}
    <div class="muted-state">No repository open.</div>
  {:else if loading}
    <div class="muted-state"><RefreshCw size={12} class="spin" /> Loading…</div>
  {:else if error}
    <div class="state-wrap">
      <Alert variant="error" compact text={error} />
    </div>
  {:else if status}

    <!-- ── Main branch missing — gated init flow ── -->
    {#if !status.main_exists}

      {#if createMainDialog.show}
        <div class="state-wrap">
          <Alert variant="warning" title="Branch not found">
            <p class="card-desc">
              The branch <code>{createMainDialog.branchName}</code> doesn't exist yet.
              Create it automatically and initialise Git Flow?
            </p>
            <RadioGroup
              appearance="card"
              direction="vertical"
              size="sm"
              block
              value={createMainDialog.fromInitial ? 'initial' : 'latest'}
              onchange={(v) => (createMainDialog.fromInitial = v === 'initial')}
              options={[
                { value: 'latest',  label: 'Latest commit',  description: `Point ${createMainDialog.branchName} to HEAD` },
                { value: 'initial', label: 'Initial commit', description: `Point ${createMainDialog.branchName} to the first commit` },
              ]}
            />
            <div class="actions-row">
              <Button variant="ghost" size="sm" onclick={dismissCreateMain} disabled={busy}>Cancel</Button>
              <Button variant="primary" size="sm" loading={busy} onclick={confirmCreateMain}>
                Create &amp; Initialise
              </Button>
            </div>
          </Alert>
        </div>

      {:else}
        <div class="state-wrap">
          <Alert variant="info" title="Git Flow not initialised">
            <p class="card-desc">
              Creates a <code>{config?.develop_branch ?? 'develop'}</code> branch from
              <code>{config?.main_branch ?? 'main'}</code>.
            </p>
            <div class="actions-row">
              <Button variant="primary" size="sm" loading={busy} onclick={initFlow}>
                Initialise Git Flow
              </Button>
            </div>
          </Alert>
        </div>
      {/if}

    {:else}

      <!-- Non-standard flow banner -->
      {#if !status.develop_exists}
        <div class="state-wrap">
          <Alert variant="warning" compact title="Non-standard flow">
            No <code>{config?.develop_branch ?? 'develop'}</code> branch — features and releases will branch from <code>{config?.main_branch ?? 'main'}</code>.
            {#snippet actions()}
              <Button
                variant="secondary"
                size="xs"
                disabled={busy}
                title="Create develop branch and initialise Git Flow"
                onclick={initFromBanner}
              >
                {busy ? '…' : 'Initialise'}
              </Button>
            {/snippet}
          </Alert>
        </div>
      {/if}

      <!-- Start buttons -->
      <div class="action-group">
        <Button variant="ghost" block onclick={() => openFormFor('feature_start')}>
          {#snippet iconStart()}<span class="i-feature"><Plus size={12} /></span>{/snippet}
          New Feature
        </Button>
        <Button variant="ghost" block onclick={() => openFormFor('release_start')}>
          {#snippet iconStart()}<span class="i-release"><Rocket size={12} /></span>{/snippet}
          New Release
        </Button>
        <Button variant="ghost" block onclick={() => openFormFor('hotfix_start')}>
          {#snippet iconStart()}<span class="i-hotfix"><Zap size={12} /></span>{/snippet}
          New Hotfix
        </Button>
      </div>

      <!-- ── Inline form ── -->
      {#if openForm}
        <form class="inline-form" onsubmit={(e) => { e.preventDefault(); submitForm(); }}>
          <button type="submit" class="hidden-submit" tabindex="-1" aria-hidden="true"></button>
          <div class="form-header">
            <span>{formTitle(openForm)}</span>
            <Button variant="icon" size="xs" onclick={closeForm} ariaLabel="Close form">
              {#snippet iconStart()}<X size={11} />{/snippet}
            </Button>
          </div>

          {#if showTicketPicker}
            <div class="form-field">
              <span class="form-label-row">
                <TicketCheck size={11} />
                Ticket
                {#if config?.require_ticket_branch}
                  <Badge variant="tone" tone="warning" size="sm" label="required" />
                {:else}
                  <span class="form-optional">(optional)</span>
                {/if}
              </span>
              <button
                type="button"
                class="ticket-pick-btn"
                onclick={() => (ticketModalOpen = true)}
                disabled={busy}
              >
                {#if ticketSelected}
                  <Badge variant="tone" tone="accent" size="sm" label={ticketSelected.identifier} />
                  <span class="ticket-pick-title truncate">{ticketSelected.title}</span>
                  <span class="ticket-pick-change">change</span>
                {:else}
                  <Search size={10} style="opacity:0.5" />
                  <span class="ticket-search-hint">Search issues…</span>
                {/if}
              </button>
            </div>
            <div class="form-field">
              <span class="form-label">Branch name</span>
              <Input
                size="sm"
                bind:value={formName}
                oninput={() => { if (ticketSelected && formName !== ticketSelected.identifier) ticketSelected = null; }}
                placeholder={ticketSelected ? ticketSelected.identifier : 'e.g. my-feature'}
                disabled={busy}
                autofocus={!ticketSelected}
              />
            </div>

          {:else if ticketPickerUnavailable}
            <Alert variant="warning" compact>
              <TicketCheck size={11} />
              Ticket branch names required, but no issue tracker is configured for this project.
            </Alert>
            <div class="form-field">
              <span class="form-label">Name</span>
              <Input
                size="sm"
                bind:value={formName}
                placeholder={formPlaceholder(openForm)}
                disabled={busy}
                autofocus
              />
            </div>

          {:else}
            <div class="form-field">
              <span class="form-label">{openForm.includes('release') ? 'Version' : 'Name'}</span>
              <Input
                size="sm"
                bind:value={formName}
                placeholder={formPlaceholder(openForm)}
                disabled={busy}
                autofocus
              />
            </div>
          {/if}

          {#if needsTagMsg(openForm)}
            <div class="form-field">
              <span class="form-label">
                Tag message <span class="form-optional">(optional)</span>
              </span>
              <Input
                size="sm"
                bind:value={formTag}
                placeholder="Leave blank for default"
                disabled={busy}
              />
            </div>
          {/if}

          <div class="form-actions">
            <Button variant="ghost" size="sm" onclick={closeForm} disabled={busy}>Cancel</Button>

            {#if isForcedPr}
              <Button variant="primary" size="sm" type="submit" loading={busy} disabled={!formName.trim()}>
                {formTitle(openForm)} → PR/MR
              </Button>
            {:else if openForm === 'feature_finish' || openForm === 'release_finish' || openForm === 'hotfix_finish'}
              <div class="finish-split">
                <SplitButton
                  variant="primary"
                  size="sm"
                  direction="up"
                  loading={busy}
                  disabled={!formName.trim()}
                  onclick={() => submitForm()}
                  onselect={(id) => { submitAction = id as 'normal' | 'pr'; }}
                  options={[
                    { id: 'normal', label: 'Finish normally (merge locally)', icon: GitMerge },
                    { id: 'pr',     label: 'Finish with PR/MR',                icon: GitPullRequest },
                  ]}
                >
                  {#if busy}
                    Working…
                  {:else if submitAction === 'pr'}
                    <GitPullRequest size={11} />{formTitle(openForm)} → PR/MR
                  {:else}
                    {formTitle(openForm)}
                  {/if}
                </SplitButton>
              </div>
            {:else}
              <Button variant="primary" size="sm" type="submit" loading={busy} disabled={!formName.trim()}>
                {formTitle(openForm)}
              </Button>
            {/if}
          </div>
        </form>
      {/if}

      <!-- ── Active branches lists ── -->
      {#if status.active_features.length > 0}
        <FlowBranchSection
          kind="feature"
          title="Features"
          dotClass="feature-dot"
          icon={GitBranch}
          names={status.active_features}
          currentName={status.current_branch_type === 'feature' ? status.current_flow_name : null}
          forcedPr={!!config?.finish.feature_use_pr}
          defaultPr={!!config?.finish.feature_pr_default}
          {busy}
          onFinish={(name, forcePr) => quickFinish('feature', name, forcePr)}
        />
      {/if}

      {#if status.active_releases.length > 0}
        <FlowBranchSection
          kind="release"
          title="Releases"
          dotClass="release-dot"
          icon={Tag}
          names={status.active_releases}
          currentName={status.current_branch_type === 'release' ? status.current_flow_name : null}
          forcedPr={!!config?.finish.release_use_pr}
          defaultPr={!!config?.finish.release_pr_default}
          {busy}
          onFinish={(name, forcePr) => quickFinish('release', name, forcePr)}
        />
      {/if}

      {#if status.active_hotfixes.length > 0}
        <FlowBranchSection
          kind="hotfix"
          title="Hotfixes"
          dotClass="hotfix-dot"
          icon={Zap}
          names={status.active_hotfixes}
          currentName={status.current_branch_type === 'hotfix' ? status.current_flow_name : null}
          forcedPr={!!config?.finish.hotfix_use_pr}
          defaultPr={!!config?.finish.hotfix_pr_default}
          {busy}
          onFinish={(name, forcePr) => quickFinish('hotfix', name, forcePr)}
        />
      {/if}

      {#if status.active_features.length === 0 && status.active_releases.length === 0 && status.active_hotfixes.length === 0}
        <div class="muted-state">No active flow branches.</div>
      {/if}

    {/if}
  {/if}
</div>

<!-- Ticket picker modal -->
{#if ticketModalOpen}
  <TicketPickerModal
    onSelect={onTicketSelected}
    onClose={() => (ticketModalOpen = false)}
  />
{/if}

<!-- PR/MR creation modal -->
{#if pendingPr}
  <CreateMrModal
    currentBranch={pendingPr.sourceBranch}
    initialTargetBranch={pendingPr.targetBranch}
    initialTitle={pendingPr.title}
    onClose={() => pendingPr = null}
    onCreated={() => { pendingPr = null; uiStore.showToast('PR/MR created', 'success'); }}
  />
{/if}

<!-- One-time "no develop" warning -->
{#if noDevelopPrompt.show}
  <Modal onClose={dismissNoDevelop} ariaLabel="No develop branch">
    {#snippet header()}
      <ModalHeader onClose={dismissNoDevelop}>
        <span class="modal-icon"><AlertTriangle size={14} /></span>
        <span class="modal-title">No <code>{config?.develop_branch ?? 'develop'}</code> branch</span>
      </ModalHeader>
    {/snippet}

    <div class="nodev-body">
      <p>
        This repository doesn't have a <code>{config?.develop_branch ?? 'develop'}</code> branch.
        The {noDevelopPrompt.kind} will be created from
        <code>{noDevelopPrompt.baseBranch}</code> instead.
      </p>
      <p class="nodev-hint">
        You can initialise the standard Git Flow (creates <code>{config?.develop_branch ?? 'develop'}</code>
        from <code>{noDevelopPrompt.baseBranch}</code>) any time from the panel banner. This heads-up
        won't be shown again for this project.
      </p>
    </div>

    {#snippet footer()}
      <Button variant="ghost" onclick={dismissNoDevelop} disabled={busy}>Cancel</Button>
      <Button variant="primary" onclick={confirmNoDevelop} loading={busy}>
        Continue on {noDevelopPrompt.baseBranch}
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .gitflow-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    font-family: var(--font-ui-sans);
    font-size: 12px;
    color: var(--text-primary);
    background: var(--bg-base);
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }

  /* ── Header ── */
  .panel-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 6px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .header-icon { color: var(--accent); display: flex; }
  .header-title {
    flex: 1;
    font-weight: 600;
    font-size: 11px;
    letter-spacing: 0.3px;
    color: var(--text-secondary);
    text-transform: uppercase;
  }

  /* ── Empty / muted states ── */
  .muted-state {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 16px 12px;
    color: var(--text-muted);
    font-size: 11px;
    font-style: italic;
  }
  .state-wrap { padding: 10px; }
  .state-wrap :global(p.card-desc) {
    margin: 0;
    font-size: 11px;
    color: var(--text-secondary);
    line-height: 1.45;
  }
  .state-wrap :global(.actions-row) {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 4px;
  }

  /* ── Inline code styling — local to GitFlowPanel & its modal body ── */
  code {
    font-family: var(--font-code);
    background: rgba(255,255,255,0.07);
    padding: 0 4px;
    border-radius: var(--radius-sm);
    font-size: 0.92em;
    color: inherit;
  }

  /* ── Action buttons ── */
  .action-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px 4px;
  }
  /* Override ghost defaults: card-like surface + left-aligned label. */
  .action-group :global(.btn) {
    justify-content: flex-start;
    background: var(--bg-elevated);
    color: var(--text-primary);
  }
  .action-group :global(.btn:hover:not(:disabled)) { background: var(--bg-hover); }
  .i-feature { display: inline-flex; color: #22c55e; }
  .i-release { display: inline-flex; color: var(--warning); }
  .i-hotfix  { display: inline-flex; color: #ef4444; }

  /* ── Inline form ── */
  .inline-form {
    margin: 4px 8px 8px;
    padding: 10px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .form-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    font-size: 11px;
    color: var(--text-secondary);
  }

  .form-field { display: flex; flex-direction: column; gap: 4px; font-size: 11px; color: var(--text-secondary); }
  .form-label,
  .form-label-row {
    font-size: 11px;
    color: var(--text-secondary);
  }
  .form-label-row { display: flex; align-items: center; gap: 4px; }
  .form-optional  { color: var(--text-muted); font-size: 10px; }

  .form-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }

  /* ── Ticket picker button ── */
  .ticket-pick-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 8px;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    min-width: 0;
  }
  .ticket-pick-btn:hover:not(:disabled) { background: var(--bg-hover); border-color: var(--accent); }
  .ticket-pick-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .ticket-pick-title {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .ticket-pick-change {
    flex-shrink: 0;
    font-size: 9px;
    color: var(--text-muted);
    margin-left: auto;
  }
  .ticket-search-hint { color: var(--text-muted); }

  /* Inline form split-button content layout. */
  .finish-split { display: inline-flex; }
  .finish-split :global(.split-main) { gap: 5px; }

  /* Hidden submit — keeps Enter-to-submit working in multi-input forms. */
  .hidden-submit {
    position: absolute;
    width: 1px; height: 1px;
    padding: 0; margin: -1px;
    overflow: hidden; border: 0;
  }

  .truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* ── No-develop modal body ── */
  .modal-icon { color: var(--warning); display: inline-flex; align-items: center; }
  .nodev-body { display: flex; flex-direction: column; gap: 8px; }
  .nodev-body p {
    margin: 0;
    font-size: 11.5px;
    color: var(--text-secondary);
    line-height: 1.5;
  }
  .nodev-hint { color: var(--text-muted) !important; font-size: 10.5px !important; }

  :global(.spin) { animation: spin 0.9s linear infinite; }
  @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
</style>
