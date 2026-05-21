<script lang="ts">
  import { GitPullRequest, ChevronDown, Wand2, Info } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { createMr, getMrCapabilities } from '$lib/ipc/mr';
  import { listLocalBranches, listRemoteBranches } from '$lib/ipc/branch';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import type { CreateMrParams } from '$lib/types/mr';
  import type { BranchInfo } from '$lib/types/git';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';

  let { onClose, onCreated, currentBranch = '', initialTargetBranch = '', initialTitle = '' }: {
    onClose:              () => void;
    onCreated:            () => void;
    currentBranch?:       string;
    /** Pre-fill the target branch (e.g. when triggered by Git Flow finish). */
    initialTargetBranch?: string;
    /** Pre-fill the PR/MR title (e.g. when triggered by Git Flow finish). */
    initialTitle?:        string;
  } = $props();

  const tabId = $derived(tabsStore.activeTabId ?? '');

  // Form state
  // svelte-ignore state_referenced_locally
  let title        = $state(initialTitle);
  let description  = $state('');
  // svelte-ignore state_referenced_locally
  let sourceBranch = $state(currentBranch);
  // svelte-ignore state_referenced_locally
  let targetBranch = $state(initialTargetBranch);
  let isDraft      = $state(false);
  let autoMerge    = $state(false);
  // Merge-time flags armed alongside auto-merge. Squash defaults off (preserve
  // history); delete-branch defaults on (clean up the feature branch after the
  // automatic merge fires). Both are only forwarded to the backend when
  // auto-merge itself is active — without auto-merge the user picks them at
  // merge time from the detail modal instead.
  let autoMergeSquash = $state(false);
  let autoMergeDelete = $state(true);
  let labels       = $state('');
  let submitting   = $state(false);

  // Branch lists
  let localBranches  = $state<BranchInfo[]>([]);
  let remoteBranches = $state<BranchInfo[]>([]);
  let branchesLoading = $state(true);

  // Auto-merge capability (probed on mount; defaults to allowed)
  let autoMergeSupported = $state(true);
  let autoMergeReason    = $state<string | null>(null);
  const autoMergeDisabled = $derived(isDraft || !autoMergeSupported);
  const autoMergeHint = $derived(
    !autoMergeSupported && autoMergeReason
      ? autoMergeReason
      : isDraft
        ? 'Drafts cannot be auto-merged — uncheck "Create as draft" first.'
        : null,
  );

  const allBranches = $derived(
    (() => {
      const local  = localBranches.map(b => b.name);
      const remote = remoteBranches
        .map(b => b.name.replace(/^origin\//, ''))
        .filter(n => !local.includes(n));
      return [...local, ...remote];
    })()
  );

  const canSubmit = $derived(
    title.trim().length > 0 &&
    sourceBranch.trim().length > 0 &&
    targetBranch.trim().length > 0 &&
    sourceBranch !== targetBranch
  );

  function buildBranchItems(current: string, setter: (v: string) => void): DropdownItem[] {
    const items: DropdownItem[] = allBranches.map(b => ({
      kind:    'item',
      id:      b,
      label:   b,
      active:  current === b,
      onclick: () => setter(b),
    }));
    // Surface a sticky entry for an externally-supplied value not in the list.
    if (current && !allBranches.includes(current)) {
      items.unshift({
        kind:    'item',
        id:      current,
        label:   current,
        active:  true,
        onclick: () => setter(current),
      });
    }
    return items;
  }
  const sourceItems = $derived(buildBranchItems(sourceBranch, v => sourceBranch = v));
  const targetItems = $derived(buildBranchItems(targetBranch, v => targetBranch = v));

  onMount(async () => {
    // Probe auto-merge support in parallel; never throws (backend swallows errors).
    getMrCapabilities(tabId).then(caps => {
      autoMergeSupported = caps.autoMergeSupported;
      autoMergeReason    = caps.autoMergeReason;
      if (!caps.autoMergeSupported) autoMerge = false;
    }).catch(() => { /* keep permissive defaults */ });

    branchesLoading = true;
    try {
      const [local, remote] = await Promise.all([
        listLocalBranches(tabId),
        listRemoteBranches(tabId).catch(() => [] as BranchInfo[]),
      ]);
      localBranches  = local;
      remoteBranches = remote;

      // Set sensible defaults
      if (!sourceBranch) {
        const head = local.find(b => b.is_head) ?? local[0];
        if (head) sourceBranch = head.name;
      }
      // Pick a common default target only when not pre-filled
      if (!targetBranch) {
        const preferred = ['main', 'master', 'develop', 'dev'];
        for (const p of preferred) {
          if (allBranches.includes(p) && p !== sourceBranch) {
            targetBranch = p;
            break;
          }
        }
        if (!targetBranch && allBranches.length > 0) {
          targetBranch = allBranches.find(b => b !== sourceBranch) ?? '';
        }
      }
    } catch { /* ignore */ } finally {
      branchesLoading = false;
    }
  });

  async function submit() {
    if (!canSubmit) return;
    // Snapshot the tab so a mid-flight tab switch doesn't redirect the PR
    // creation to a different repository.
    const mrTabId = tabId;
    submitting = true;
    const params: CreateMrParams = {
      title:        title.trim(),
      description:  description.trim() || null,
      sourceBranch: sourceBranch.trim(),
      targetBranch: targetBranch.trim(),
      isDraft,
      labels:       labels.split(',').map(l => l.trim()).filter(Boolean),
      // Squash & delete-branch are normally picked at merge time from the
      // detail modal — but if auto-merge is armed the MR will close itself,
      // so we capture the choices here and forward them to the backend.
      squash:       autoMerge ? autoMergeSquash : false,
      deleteBranch: autoMerge ? autoMergeDelete : false,
      autoMerge,
    };
    try {
      const mr = await createMr(mrTabId, params);
      uiStore.showToast(`${isDraft ? 'Draft ' : ''}PR #${mr.number} created`, 'success');
      onCreated();
      onClose();
    } catch (e: any) {
      uiStore.showToast(String(e), 'error');
    } finally {
      submitting = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) submit();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Modal {onClose} width="580px" ariaLabel="Create Pull Request">
  {#snippet header()}
    <ModalHeader {onClose}>
      <GitPullRequest size={14} />
      <span class="modal-title">Create Pull / Merge Request</span>
    </ModalHeader>
  {/snippet}

  <div class="form-body">
    <!-- Title -->
    <FormField label="Title" required for="mr-title">
      <Input
        id="mr-title"
        placeholder="Summary of your changes"
        bind:value={title}
        autofocus
      />
    </FormField>

    <!-- Branch selects -->
    <div class="field-row">
      <div class="field-branch">
        <FormField label="Source branch" required>
        <div class="select-wrap">
          <Dropdown
            position="fixed"
            direction="down"
            matchTriggerWidth
            searchable={allBranches.length > 12}
            searchPlaceholder="Filter branches…"
            items={sourceItems}
            loading={branchesLoading}
          >
            {#snippet trigger({ open, toggle })}
              <button
                class="field-select"
                onclick={toggle}
                disabled={branchesLoading}
                type="button"
                aria-haspopup="listbox"
                aria-expanded={open}
              >
                <span class="field-select-label">{branchesLoading ? 'Loading…' : (sourceBranch || '— pick a branch —')}</span>
                <ChevronDown size={12} />
              </button>
            {/snippet}
          </Dropdown>
        </div>
        </FormField>
      </div>

      <div class="branch-arrow">→</div>

      <div class="field-branch">
        <FormField label="Target branch" required error={!!sourceBranch && !!targetBranch && sourceBranch === targetBranch ? 'Source and target must be different' : null}>
        <div class="select-wrap">
          <Dropdown
            position="fixed"
            direction="down"
            matchTriggerWidth
            searchable={allBranches.length > 12}
            searchPlaceholder="Filter branches…"
            items={targetItems}
            loading={branchesLoading}
          >
            {#snippet trigger({ open, toggle })}
              <button
                class="field-select"
                onclick={toggle}
                disabled={branchesLoading}
                type="button"
                aria-haspopup="listbox"
                aria-expanded={open}
              >
                <span class="field-select-label">{branchesLoading ? 'Loading…' : (targetBranch || '— pick a branch —')}</span>
                <ChevronDown size={12} />
              </button>
            {/snippet}
          </Dropdown>
        </div>
        </FormField>
      </div>
    </div>

    <!-- Description -->
    <FormField label="Description" for="mr-desc">
      <textarea
        id="mr-desc"
        class="field-textarea"
        placeholder="Describe what this PR does, why the changes are needed, and any relevant context…"
        rows="4"
        bind:value={description}
      ></textarea>
    </FormField>

    <!-- Labels -->
    <FormField label="Labels" for="mr-labels">
      <Input
        id="mr-labels"
        placeholder="bug, enhancement, documentation (comma-separated)"
        bind:value={labels}
      />
    </FormField>

    <!-- Checkboxes -->
    <div class="checks-group">

      <label class="check-row">
        <input type="checkbox" class="check-input" bind:checked={isDraft} />
        <div class="check-body">
          <span class="check-title">Create as draft</span>
          <span class="check-hint">Signals work in progress — cannot be merged until marked ready.</span>
        </div>
      </label>

      <label class="check-row" class:is-disabled={autoMergeDisabled} use:tooltip={autoMergeHint ?? ''}>
        <input
          type="checkbox"
          class="check-input"
          bind:checked={autoMerge}
          disabled={autoMergeDisabled}
        />
        <div class="check-body">
          <span class="check-title">
            <Wand2 size={11} style="margin-right:4px;vertical-align:-1px" />
            Enable auto-merge
            {#if !autoMergeSupported}
              <Badge variant="tone" tone="warning" size="sm" label="not allowed" />
            {/if}
          </span>
          {#if !autoMergeSupported && autoMergeReason}
            <span class="check-hint warn">{autoMergeReason}</span>
          {:else}
            <span class="check-hint">
              Merge automatically once required checks pass
              <span class="sep">·</span>
              <em>GitHub</em> needs branch protection to permit it
              <span class="sep">·</span>
              <em>GitLab</em> uses <code>merge when pipeline succeeds</code>.
              A notification is posted if auto-merge can't be armed.
            </span>
          {/if}
        </div>
      </label>

      {#if autoMerge && autoMergeSupported}
        <!-- Sub-checks: only meaningful when auto-merge is armed, because the
             MR will close itself and the detail-modal flags would never be
             reached otherwise. -->
        <label class="check-row sub-check">
          <input type="checkbox" class="check-input" bind:checked={autoMergeSquash} />
          <div class="check-body">
            <span class="check-title">Squash commits</span>
            <span class="check-hint">Combine all commits on the source branch into one.</span>
          </div>
        </label>

        <label class="check-row sub-check">
          <input type="checkbox" class="check-input" bind:checked={autoMergeDelete} />
          <div class="check-body">
            <span class="check-title">Delete source branch after merge</span>
            <span class="check-hint">
              Removes the remote branch once auto-merge fires.
              <span class="sep">·</span>
              <em>GitHub</em> also needs <code>Automatically delete head branches</code> enabled at repo level.
            </span>
          </div>
        </label>
      {/if}

    </div>

    {#if !autoMerge}
      <p class="merge-note">
        <Info size={12} class="merge-note-icon" />
        <span>
          Squash &amp; delete-source-branch are chosen at merge time from the
          MR's detail view — enable auto-merge above to set them here instead.
        </span>
      </p>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose} disabled={submitting}>Cancel</Button>
    <Button variant="primary" onclick={submit} disabled={!canSubmit || submitting} loading={submitting}>
      {isDraft ? 'Create draft' : 'Create PR'}
    </Button>
  {/snippet}
</Modal>

<style>
  /* ── Body ─────────────────────────────────────────────────────────────────── */
  .form-body {
    display: flex;
    flex-direction: column;
    gap: 13px;
  }

  /* ── Fields ───────────────────────────────────────────────────────────────── */
  /* Background mirrors `--bg-input` (same token used by Input.svelte) so the
     textarea reads as part of the same form-control family as the Title /
     Labels rows above — earlier `--bg-overlay` made it pop as a pale grey
     card against the modal chrome, which felt out of place. */
  .field-textarea {
    width: 100%;
    resize: vertical;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    padding: 8px 10px;
    box-sizing: border-box;
    outline: none;
    transition: border-color var(--transition-fast);
    line-height: 1.55;
  }
  .field-textarea::placeholder { color: var(--text-disabled); }
  .field-textarea:focus { border-color: var(--border-focus); }

  /* ── Branch row ───────────────────────────────────────────────────────────── */
  .field-row {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }
  .field-branch { flex: 1; min-width: 0; }
  .branch-arrow {
    font-size: 14px;
    color: var(--text-muted);
    padding-top: 22px;
    flex-shrink: 0;
  }

  /* ── Select ───────────────────────────────────────────────────────────────── */
  .select-wrap { display: flex; align-items: center; }
  .select-wrap :global(.dd-root) { width: 100%; }
  .field-select {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 11px;
    padding: 7px 10px;
    outline: none;
    cursor: pointer;
    text-align: left;
    transition: border-color var(--transition-fast);
  }
  .field-select:hover,
  .field-select[aria-expanded='true'] { border-color: var(--accent); }
  .field-select:disabled { opacity: 0.5; cursor: default; }
  .field-select-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── Checkboxes group ─────────────────────────────────────────────────────── */
  .checks-group {
    display: flex;
    flex-direction: column;
    gap: 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .check-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    cursor: pointer;
    user-select: none;
    transition: background var(--transition-fast);
    border-bottom: 1px solid var(--border-subtle);
  }
  .check-row:last-child { border-bottom: none; }
  .check-row:hover { background: var(--bg-hover); }
  .check-row.is-disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .check-row.is-disabled:hover { background: transparent; }
  .check-input {
    width: 14px;
    height: 14px;
    margin-top: 1px;
    accent-color: var(--accent);
    cursor: pointer;
    flex-shrink: 0;
  }
  .check-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .check-title {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-primary);
  }
  .check-hint {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.4;
  }
  .check-hint code {
    font-family: var(--font-code);
    font-size: 10px;
    background: var(--bg-overlay);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
    color: var(--accent);
  }
  .check-hint em {
    font-style: normal;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .check-hint .sep {
    color: var(--text-disabled);
    margin: 0 3px;
  }
  .check-hint.warn {
    color: var(--text-secondary);
  }
  .check-title :global(.badge) {
    margin-left: 6px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }

  /* Sub-options shown only when auto-merge is armed. Indent + slightly
     muted left stripe to telegraph the "modifies the row above" relation. */
  .sub-check {
    padding-left: 28px;
    background: var(--bg-overlay);
    position: relative;
  }
  .sub-check::before {
    content: '';
    position: absolute;
    left: 16px;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--border);
  }

  /* Inline informational note — no boxed card, just an accent left stripe.
     The previous design used `--bg-overlay` which lit up as a pale grey block
     against the modal background; switching to a transparent body keeps the
     density of the form while still telegraphing "this is an aside". */
  .merge-note {
    margin: 0;
    padding: 2px 0 2px 10px;
    font-size: 11px;
    color: var(--text-secondary);
    background: transparent;
    border: none;
    border-left: 2px solid var(--accent);
    border-radius: 0;
    line-height: 1.5;
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
  .merge-note :global(.merge-note-icon) {
    color: var(--accent);
    flex-shrink: 0;
    margin-top: 2px;
  }

</style>
