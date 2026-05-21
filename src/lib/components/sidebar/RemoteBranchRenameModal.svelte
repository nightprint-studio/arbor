<script lang="ts">
  import { Pencil, Globe, ShieldAlert } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import type { BranchInfo } from '$lib/types/git';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { renameRemoteBranch } from '$lib/ipc/branch';
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
    /** Remote branch in "remote/branch" form (e.g. "origin/develop"). */
    branch: BranchInfo;
    onClose: () => void;
    onRenamed: () => void;
  } = $props();

  const tab = $derived(tabsStore.activeTab);

  // ── Split "origin/develop" into { remote: "origin", shortName: "develop" } ──
  // Done via configured remotes (branch names may contain "/"), with a sane fallback.
  const parts = $derived.by((): { remote: string; shortName: string } => {
    const remoteNames = repoStore.remoteBranches
      .map((b) => {
        const i = b.name.indexOf('/');
        return i >= 0 ? b.name.slice(0, i) : '';
      })
      .filter(Boolean);
    const seen = new Set<string>();
    for (const r of remoteNames) {
      if (seen.has(r)) continue;
      seen.add(r);
      const prefix = r + '/';
      if (branch.name.startsWith(prefix)) {
        const rest = branch.name.slice(prefix.length);
        if (rest) return { remote: r, shortName: rest };
      }
    }
    const i = branch.name.indexOf('/');
    return i >= 0
      ? { remote: branch.name.slice(0, i), shortName: branch.name.slice(i + 1) }
      : { remote: '', shortName: branch.name };
  });

  // svelte-ignore state_referenced_locally
  let newName        = $state(parts.shortName);
  let alsoRenameLocal = $state(true);
  let renaming       = $state(false);
  let step           = $state<'push' | 'delete' | 'local' | null>(null);

  // ── Local branch presence ────────────────────────────────────────────────
  const localExists = $derived.by((): boolean =>
    repoStore.localBranches.some((b) => b.name === parts.shortName)
  );
  const localBranch = $derived.by(() =>
    repoStore.localBranches.find((b) => b.name === parts.shortName) ?? null
  );

  // ── Validation ───────────────────────────────────────────────────────────
  const INVALID_CHARS = /[\x00-\x1f\x7f ~^:?*\[\\]/;
  const DOUBLE_DOT    = /\.\./;
  const TRAILING      = /[./]$/;
  const AT_BRACE      = /@\{/;

  const validationError = $derived.by((): string | null => {
    const v = newName.trim();
    if (!v)                     return 'Name cannot be empty.';
    if (v === parts.shortName)  return null;
    if (v.startsWith('-'))      return 'Name cannot start with a dash.';
    if (v.startsWith('.'))      return 'Name cannot start with a dot.';
    if (INVALID_CHARS.test(v))  return 'Name contains an invalid character ( space ~ ^ : ? * [ \\ ).';
    if (DOUBLE_DOT.test(v))     return 'Name cannot contain "..".';
    if (TRAILING.test(v))       return 'Name cannot end with "." or "/".';
    if (AT_BRACE.test(v))       return 'Name cannot contain "@{".';
    // Conflict with an existing remote ref under the same remote
    const conflict = repoStore.remoteBranches.some(
      (b) => b.name === `${parts.remote}/${v}`,
    );
    if (conflict) return `A remote branch "${parts.remote}/${v}" already exists.`;
    // If we'd also rename local but the target name already exists locally
    if (alsoRenameLocal && localExists) {
      const localConflict = repoStore.localBranches.some((b) => b.name === v);
      if (localConflict) return `A local branch "${v}" already exists.`;
    }
    return null;
  });

  const isUnchanged = $derived(newName.trim() === parts.shortName);
  const canSubmit   = $derived(!isUnchanged && validationError === null && !renaming);

  const stepLabel = $derived.by((): string => {
    switch (step) {
      case 'push':   return 'Pushing new name to remote…';
      case 'delete': return 'Deleting old remote branch…';
      case 'local':  return 'Renaming local branch…';
      default:       return 'Renaming…';
    }
  });

  async function handleRename() {
    if (!canSubmit || !tab) return;
    renaming = true;
    const trimmed = newName.trim();
    step = 'push'; // backend runs push → delete → local in one call; coarse label
    try {
      const willRenameLocal = alsoRenameLocal && localExists;
      const result = await renameRemoteBranch(tab.id, branch.name, trimmed, willRenameLocal);

      const suffix = result.local_renamed
        ? ' (remote + local)'
        : result.local_skipped
          ? ' (remote only — local left untouched)'
          : '';
      uiStore.showToast(
        `Renamed "${branch.name}" → "${result.new_full_name}"${suffix}`,
        'success',
      );
      graphStore.refresh();
      onRenamed();
      onClose();
    } catch (err) {
      uiStore.showToast(`Rename failed (${step ?? 'unknown'} step): ${err}`, 'error');
    } finally {
      renaming = false;
      step = null;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && canSubmit) { e.preventDefault(); handleRename(); }
  }
</script>

<Modal {onClose} width="500px" ariaLabel="Rename remote branch">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Globe size={14} class="header-icon" />
      <span class="modal-title">Rename Remote Branch</span>
    </ModalHeader>
  {/snippet}

  <div class="form-stack">

    <!-- Current name -->
    <FormField label="Current name">
      <div class="current-name">
        <Badge variant="chip" tone="neutral">
          {#snippet icon()}<Globe size={9} />{/snippet}
          {branch.name}
        </Badge>
        {#if localExists}
          <Badge variant="tone" tone="accent" size="sm">
            local "{parts.shortName}" exists{localBranch?.is_head ? ' (HEAD)' : ''}
          </Badge>
        {/if}
      </div>
    </FormField>

    <!-- New short name -->
    <FormField
      label={`New name (remote stays "${parts.remote}")`}
      for="new-remote-branch-name"
      error={!isUnchanged ? validationError : null}
    >
      <Input
        id="new-remote-branch-name"
        bind:value={newName}
        placeholder="e.g. main"
        autofocus
        error={!isUnchanged ? validationError : null}
        onkeydown={handleKeydown}
      />
    </FormField>

    <!-- Also-rename-local toggle (only when a matching local exists) -->
    {#if localExists}
      <div class="local-toggle-row" class:active={alsoRenameLocal}>
        <Toggle bind:checked={alsoRenameLocal} label="Also rename matching local branch" />
        <div class="toggle-sub">
          Rename local <code>{parts.shortName}</code> → <code>{newName.trim() || '…'}</code>
          and re-point its upstream to <code>{parts.remote}/{newName.trim() || '…'}</code>.
        </div>
      </div>
    {/if}

    <!-- ── Warnings ── -->
    <div class="warnings">

      <Alert variant="error">
        <strong>Destructive remote operation — cannot be undone.</strong>
        This will:
        <ol class="step-list">
          <li>Push the current tip of <code>{branch.name}</code> to <code>{parts.remote}/{newName.trim() || '…'}</code></li>
          <li>Delete <code>{branch.name}</code> from the remote server</li>
          {#if alsoRenameLocal && localExists}
            <li>Rename local <code>{parts.shortName}</code> → <code>{newName.trim() || '…'}</code> and update its upstream</li>
          {/if}
        </ol>
        Anyone tracking <code>{branch.name}</code> will have a broken upstream after this.
      </Alert>

      {#if !localExists}
        <Alert variant="info">
          No local branch named <code>{parts.shortName}</code> was found — only the remote ref will be renamed.
        </Alert>
      {/if}

      {#if localExists && !alsoRenameLocal}
        <Alert variant="warning">
          <strong>Local <code>{parts.shortName}</code> will keep its current name.</strong>
          Its upstream <code>{branch.name}</code> will be broken after this rename — you'll need to
          re-set it manually, e.g.
          <code>git branch --set-upstream-to={parts.remote}/{newName.trim() || '{new-name}'} {parts.shortName}</code>
        </Alert>
      {/if}

    </div>

  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose} disabled={renaming}>Cancel</Button>
    <Button
      variant="danger"
      onclick={handleRename}
      disabled={!canSubmit}
      loading={renaming}
      title={isUnchanged ? 'Enter a different name to rename' : undefined}
    >
      {#snippet iconStart()}<ShieldAlert size={12} />{/snippet}
      {renaming ? stepLabel : 'Rename Remote Branch'}
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

  .local-toggle-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: var(--radius-md, 6px);
    border: 1px solid var(--border-subtle);
    background: var(--bg-base);
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .local-toggle-row.active {
    border-color: rgba(120, 160, 220, 0.45);
    background: rgba(77, 120, 204, 0.05);
  }
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

  .warnings {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

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
