<script lang="ts">
  import { Pencil, Globe, ShieldAlert } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import type { BranchInfo } from '$lib/types/git';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { renameBranch, deleteRemoteBranches } from '$lib/ipc/branch';
  import { pushBranch } from '$lib/ipc/remote';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';

  let {
    branch,
    onClose,
    onRenamed,
  }: {
    branch: BranchInfo;
    onClose: () => void;
    onRenamed: () => void;
  } = $props();

  const tab = $derived(tabsStore.activeTab);

  // svelte-ignore state_referenced_locally
  let newName          = $state(branch.name);
  let alsoRenameRemote = $state(false);
  let renaming         = $state(false);

  // ── Parse upstream remote ────────────────────────────────────────────────
  const upstreamParts = $derived.by((): { remote: string; remoteBranch: string } | null => {
    if (!branch.upstream) return null;
    const slash = branch.upstream.indexOf('/');
    if (slash === -1) return null;
    return {
      remote:       branch.upstream.slice(0, slash),
      remoteBranch: branch.upstream.slice(slash + 1),
    };
  });

  // ── Validation ───────────────────────────────────────────────────────────
  const INVALID_CHARS = /[\x00-\x1f\x7f ~^:?*\[\\]/;
  const DOUBLE_DOT    = /\.\./;
  const TRAILING      = /[./]$/;
  const AT_BRACE      = /@\{/;

  const validationError = $derived.by((): string | null => {
    const v = newName.trim();
    if (!v)                      return 'Name cannot be empty.';
    if (v === branch.name)       return null;
    if (v.startsWith('-'))       return 'Name cannot start with a dash.';
    if (v.startsWith('.'))       return 'Name cannot start with a dot.';
    if (INVALID_CHARS.test(v))   return 'Name contains an invalid character ( space ~ ^ : ? * [ \\ ).';
    if (DOUBLE_DOT.test(v))      return 'Name cannot contain "..".';
    if (TRAILING.test(v))        return 'Name cannot end with "." or "/".';
    if (AT_BRACE.test(v))        return 'Name cannot contain "@{".';
    return null;
  });

  const isUnchanged = $derived(newName.trim() === branch.name);
  const canSubmit   = $derived(!isUnchanged && validationError === null && !renaming);
  const hasUpstream = $derived(!!branch.upstream && upstreamParts !== null);
  const isHead      = $derived(branch.is_head);
  const hasAhead    = $derived(branch.ahead > 0);

  // ── Steps label for progress display ─────────────────────────────────────
  let renamingStep = $state<'local' | 'push' | 'delete' | null>(null);

  const renamingLabel = $derived.by((): string => {
    switch (renamingStep) {
      case 'local':  return 'Renaming local branch…';
      case 'push':   return 'Pushing new name to remote…';
      case 'delete': return 'Deleting old remote branch…';
      default:       return 'Renaming…';
    }
  });

  // ── Actions ──────────────────────────────────────────────────────────────
  async function handleRename() {
    if (!canSubmit || !tab) return;
    renaming = true;
    const trimmed = newName.trim();
    try {
      // Step 1 — rename local
      renamingStep = 'local';
      await renameBranch(tab.id, branch.name, trimmed);

      // Step 2 — optionally update remote
      if (alsoRenameRemote && hasUpstream) {
        const parts = upstreamParts!;

        // Push the new local name to the same remote
        renamingStep = 'push';
        await pushBranch(tab.id, parts.remote, `refs/heads/${trimmed}`);

        // Delete the old remote branch
        renamingStep = 'delete';
        const failed = await deleteRemoteBranches(tab.id, [`${parts.remote}/${branch.name}`]);
        if (failed.length > 0) {
          uiStore.showToast(
            `Remote branch deleted failed for: ${failed.join(', ')}. The local branch was still renamed.`,
            'warning'
          );
        }
      }

      uiStore.showToast(
        alsoRenameRemote && hasUpstream
          ? `Branch renamed: "${branch.name}" → "${trimmed}" (local + remote)`
          : `Branch renamed: "${branch.name}" → "${trimmed}"`,
        'success'
      );
      graphStore.refresh();
      onRenamed();
      onClose();
    } catch (err) {
      uiStore.showToast(`Rename failed (${renamingStep ?? 'unknown'} step): ${err}`, 'error');
    } finally {
      renaming = false;
      renamingStep = null;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && canSubmit) { e.preventDefault(); handleRename(); }
  }
</script>

<Modal {onClose} width="500px" ariaLabel="Rename branch">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Pencil size={14} class="header-icon" />
      <span class="modal-title">Rename Branch</span>
    </ModalHeader>
  {/snippet}

  <div class="form-stack">

    <!-- Old name display -->
    <FormField label="Current name">
      <div class="current-name">
        <Badge variant="chip" tone="neutral">{branch.name}</Badge>
        {#if isHead}
          <Badge variant="tone" tone="accent" size="sm">HEAD</Badge>
        {/if}
        {#if hasUpstream}
          <Badge variant="tone" tone="neutral" size="sm">
            {#snippet icon()}<Globe size={9} />{/snippet}
            {branch.upstream}
          </Badge>
        {/if}
      </div>
    </FormField>

    <!-- New name input -->
    <FormField
      label="New name"
      for="new-branch-name"
      error={!isUnchanged ? validationError : null}
    >
      <Input
        id="new-branch-name"
        bind:value={newName}
        placeholder="e.g. feature/my-new-name"
        autofocus
        error={!isUnchanged ? validationError : null}
        onkeydown={handleKeydown}
      />
    </FormField>

    <!-- ── Also rename remote toggle (only shown when upstream exists) ── -->
    {#if hasUpstream}
      <div class="remote-toggle-row" class:active={alsoRenameRemote}>
        <Toggle bind:checked={alsoRenameRemote} label="Also rename remote branch" />
        <div class="toggle-sub">
          Push <code>{newName.trim() || '…'}</code> to <code>{upstreamParts?.remote}</code>
          and delete <code>{upstreamParts?.remote}/{branch.name}</code>
        </div>
      </div>
    {/if}

    <!-- ── Contextual warnings ── -->
    <div class="warnings">

      <!-- DANGER: also-rename-remote warning -->
      {#if alsoRenameRemote && hasUpstream}
        <Alert variant="error">
          <strong>Destructive remote operation — cannot be undone.</strong>
          This will:
          <ol class="step-list">
            <li>Rename the local branch to <code>{newName.trim() || '…'}</code></li>
            <li>Push <code>{newName.trim() || '…'}</code> to <code>{upstreamParts?.remote}</code></li>
            <li>Delete <code>{upstreamParts?.remote}/{branch.name}</code> from the remote server</li>
          </ol>
          Anyone tracking <code>{branch.upstream}</code> will have a broken upstream after this.
          Make sure your team is aware before proceeding.
        </Alert>
      {/if}

      <!-- HEAD branch warning -->
      {#if isHead && !alsoRenameRemote}
        <Alert variant="warning">
          <strong>You are renaming the current branch (HEAD).</strong>
          {#if hasUpstream}
            Enable <em>Also rename remote branch</em> above to keep the remote in sync,
            or update it manually after renaming.
          {:else}
            The local branch will be renamed immediately.
          {/if}
        </Alert>
      {/if}

      <!-- Remote tracking warning (upstream exists, remote rename NOT checked) -->
      {#if hasUpstream && !alsoRenameRemote}
        <Alert variant="warning">
          <strong>Remote branch <code>{branch.upstream}</code> will keep its current name.</strong>
          Enable <em>Also rename remote branch</em> above to rename it automatically,
          or update the upstream manually after renaming:<br />
          <code>git branch --set-upstream-to={upstreamParts?.remote}/{'{'}new-name{'}'} {'{'}new-name{'}'}</code>
        </Alert>
      {/if}

      <!-- Unpushed commits -->
      {#if hasAhead && !alsoRenameRemote}
        <Alert variant="info">
          This branch has <strong>{branch.ahead} unpushed commit{branch.ahead !== 1 ? 's' : ''}</strong>.
          After renaming, push under the new name to keep the remote in sync.
        </Alert>
      {/if}

      <!-- Default info note (no upstream, no HEAD, no ahead) -->
      {#if !isHead && !hasUpstream && !hasAhead}
        <Alert variant="info" text="Only the local branch ref is renamed. No remote branches are affected." />
      {/if}

    </div>

  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose} disabled={renaming}>Cancel</Button>
    <Button
      variant={alsoRenameRemote ? 'danger' : 'primary'}
      onclick={handleRename}
      disabled={!canSubmit}
      loading={renaming}
      title={isUnchanged ? 'Enter a different name to rename' : undefined}
    >
      {#snippet iconStart()}
        {#if alsoRenameRemote}
          <ShieldAlert size={12} />
        {:else}
          <Pencil size={12} />
        {/if}
      {/snippet}
      {renaming ? renamingLabel : alsoRenameRemote ? 'Rename + Delete Remote' : 'Rename'}
    </Button>
  {/snippet}
</Modal>

<style>
  :global(.header-icon) { color: var(--accent); }

  .form-stack {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .current-name {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  /* ── Remote toggle row ── */
  .remote-toggle-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: var(--radius-md, 6px);
    border: 1px solid var(--border-subtle);
    background: var(--bg-base);
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .remote-toggle-row.active {
    border-color: rgba(199, 84, 80, 0.45);
    background: rgba(199, 84, 80, 0.05);
  }
  /* Sub-description is rendered below the Toggle widget; indent so it visually
     aligns with the label text (Toggle's track ≈ 32px + 8px gap = 40px). */
  .toggle-sub {
    padding-left: 40px;
    font-size: 10.5px;
    color: var(--text-muted);
    line-height: 1.4;
  }
  .toggle-sub code {
    font-family: var(--font-code);
    font-size: 10px;
    background: rgba(255,255,255,0.07);
    padding: 0 3px;
    border-radius: var(--radius-sm);
  }

  /* ── Warnings ── */
  .warnings {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Rich content rendered via Alert's body — needs in-text styling
     for <strong>, <em>, <code>. Scoped here so Alert stays generic. */
  .warnings :global(strong) { font-weight: 600; color: var(--text-primary); }
  .warnings :global(em)     { font-style: italic; color: var(--text-secondary); }
  .warnings :global(code) {
    font-family: var(--font-code);
    font-size: 10.5px;
    background: rgba(255,255,255,0.07);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
  }

  .step-list {
    margin: 5px 0 5px 16px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .step-list li { line-height: 1.5; }
</style>
