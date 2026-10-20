<script lang="ts">
  /**
   * Corvus's commit box: the shared {@link StageCommitForm} fed from the active tab, plus what
   * only Corvus does around a commit — the git-identity prompt, the graph refresh, push, and
   * the plugin buttons.
   */
  import StageCommitForm, { type CommitRequest } from '$lib/components/shared/internal/stage/StageCommitForm.svelte';
  import Contribution from '$lib/components/corvus/Contribution.svelte';
  import PluginIcon   from '$lib/components/plugins/PluginIcon.svelte';
  import GitIdentityModal from '$lib/components/corvus/GitIdentityModal.svelte';
  import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
  import { repoStore } from '$lib/stores/corvus/repo.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/corvus/graph.svelte';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import { commitConfigStore } from '$lib/stores/corvus/commit_config.svelte';
  import { commitChanges, getGitCommitTemplate } from '$lib/ipc/corvus/stage';
  import { getGitIdentity } from '$lib/ipc/corvus/graph';
  import { pushBranch } from '$lib/ipc/corvus/remote';

  let { onCommit }: { onCommit?: () => void } = $props();

  const tab = $derived(tabsStore.activeTab);

  // Resolved template: git's own commit.template, else the global fallback.
  let template = $state('');
  $effect(() => {
    const current = tab;
    if (!current) return;
    (async () => {
      try {
        const git = await getGitCommitTemplate(current.id);
        if (git && git.trim()) { template = git; return; }
      } catch { /* fall through to the global template */ }
      template = commitConfigStore.templateGlobal;
    })();
  });

  // Git-identity prompt: shown when a commit fails because user.name / user.email are unset.
  // The blocked request is kept so the retry after saving preserves what was asked for.
  let identityRetry = $state<CommitRequest | null>(null);
  // Resolves the pending `onSubmit` once the identity prompt is settled.
  let settleIdentity: ((committed: boolean) => void) | null = null;

  // True when the global git config has no name or no email — the cause of the
  // "config value 'user.name' was not found" commit failure. Probed rather than matched on the
  // error string, which is a wire contract.
  async function isMissingIdentity(): Promise<boolean> {
    try {
      const [name, email] = await getGitIdentity();
      return !name.trim() || !email.trim();
    } catch {
      return false;
    }
  }

  async function refreshUi(originTabId: string) {
    // The user may have switched tabs while the commit / push was in flight. Refreshing "the
    // active tab" would then reload the wrong graph, so only invalidate the origin's cache.
    if (tabsStore.activeTabId !== originTabId) {
      cacheStore.invalidate(originTabId);
      return;
    }
    graphStore.refresh();
  }

  async function submit(req: CommitRequest): Promise<boolean> {
    const origin = tab;
    if (!origin) return false;

    let oid: string;
    try {
      oid = await commitChanges(origin.id, req.message, req.amend);
    } catch (err) {
      if (await isMissingIdentity()) {
        identityRetry = req;
        // Stays pending until the identity modal is saved or cancelled.
        return new Promise<boolean>((resolve) => { settleIdentity = resolve; });
      }
      uiStore.showToast(`Commit failed: ${err}`, 'error');
      return false;
    }

    // The commit landed: the graph refresh runs whether or not the push below succeeds, so a
    // failed push (expired token…) still shows the new commit and can be retried from the sidebar.
    onCommit?.();
    const short = oid.slice(0, 7);

    if (!req.push) {
      await refreshUi(origin.id);
      uiStore.showToast(`Committed ${short}`, 'success');
      return true;
    }
    const branch = tabsStore.activeTab?.currentBranch;
    if (!branch) {
      await refreshUi(origin.id);
      uiStore.showToast(`Committed ${short} (push skipped: no branch)`, 'warning');
      return true;
    }
    try {
      await pushBranch(origin.id, 'origin', `refs/heads/${branch}`);
      await refreshUi(origin.id);
      uiStore.showToast(`Committed ${short} and pushed`, 'success');
    } catch (err) {
      await refreshUi(origin.id);
      uiStore.showToast(`Committed ${short} — push failed: ${err}`, 'error');
    }
    return true;
  }

  async function onIdentitySaved() {
    const req = identityRetry;
    const settle = settleIdentity;
    identityRetry = null;
    settleIdentity = null;
    settle?.(req ? await submit(req) : false);
  }

  function onIdentityCancel() {
    identityRetry = null;
    settleIdentity?.(false);
    settleIdentity = null;
  }
</script>

<StageCommitForm
  stagedCount={repoStore.status?.staged.length ?? 0}
  {template}
  allowPush
  onSubmit={submit}
>
  {#snippet extra()}
    <Contribution point="arbor:commit-form:action">
      {#snippet item({ payload, fire })}
        {@const p = payload as { label: string; icon?: string; action: string }}
        <button type="button" class="commit-plugin-action" onclick={() => fire()}>
          {#if p.icon}<PluginIcon name={p.icon} size={14} />{/if}
          <span>{p.label}</span>
        </button>
      {/snippet}
    </Contribution>
  {/snippet}
</StageCommitForm>

{#if identityRetry}
  <GitIdentityModal onSaved={onIdentitySaved} onCancel={onIdentityCancel} />
{/if}

<style>
  .commit-plugin-action {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .commit-plugin-action:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
