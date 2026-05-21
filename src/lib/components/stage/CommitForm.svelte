<script lang="ts">
  import { FileText, GitCommitHorizontal, Upload } from 'lucide-svelte';
  import SplitButton from '$lib/components/shared/ui/SplitButton.svelte';
  import { tooltipForAction } from '$lib/utils/shortcut';
  import { tooltip } from '$lib/actions/tooltip';
  import Contribution from '$lib/components/shared/Contribution.svelte';
  import PluginIcon   from '$lib/components/plugins/PluginIcon.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { commitChanges, getGitCommitTemplate } from '$lib/ipc/stage';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import { pushBranch } from '$lib/ipc/remote';

  const TEMPLATE_KEY = 'arbor:commit-template';

  let { onCommit }: { onCommit?: () => void } = $props();

  let message          = $state('');
  let amend            = $state(false);
  let committing       = $state(false);
  let textareaEl       = $state<HTMLTextAreaElement | undefined>(undefined);

  // Palette "Commit" / "Amend Last Commit" focus the textarea here.
  $effect(() => {
    function onFocus(e: Event) {
      const ev = e as CustomEvent<{ amend?: boolean }>;
      if (ev.detail?.amend) amend = true;
      textareaEl?.focus();
      // Move caret to the end so template text isn't overwritten on type.
      if (textareaEl) {
        const len = textareaEl.value.length;
        textareaEl.setSelectionRange(len, len);
      }
    }
    window.addEventListener('arbor:focus-commit-form', onFocus);
    return () => window.removeEventListener('arbor:focus-commit-form', onFocus);
  });

  const tab = $derived(tabsStore.activeTab);
  const canCommit = $derived(
    message.trim().length > 0 && (
      (repoStore.status?.staged.length ?? 0) > 0 || amend
    )
  );

  // Resolved template: git native > global localStorage fallback
  let resolvedTemplate = $state('');

  $effect(() => {
    const currentTab = tab;
    if (!currentTab) return;
    (async () => {
      try {
        const git = await getGitCommitTemplate(currentTab.id);
        if (git && git.trim()) {
          resolvedTemplate = git;
          return;
        }
      } catch { /* ignore */ }
      resolvedTemplate = localStorage.getItem(TEMPLATE_KEY) ?? '';
    })();
  });

  // Pre-fill message when template changes and field is empty
  $effect(() => {
    if (resolvedTemplate && !message) {
      message = resolvedTemplate;
    }
  });

  function applyTemplate() {
    if (resolvedTemplate) message = resolvedTemplate;
  }

  async function handleCommit(andPush = false) {
    if (!tab || !canCommit || committing) return;
    committing = true;
    const activeTab = tab;
    try {
      // 1. Commit — if this throws, nothing to refresh.
      let oid: string;
      try {
        oid = await commitChanges(activeTab.id, message.trim(), amend);
      } catch (err) {
        uiStore.showToast(`Commit failed: ${err}`, 'error');
        return;
      }

      // 2. Commit succeeded — reset the form and schedule a graph refresh
      //    that runs regardless of whether the push below succeeds.  This
      //    way, if the push fails (e.g. expired auth token), the user still
      //    sees the new commit and can retry the push from the sidebar.
      message = '';
      amend   = false;
      onCommit?.();

      const refreshUi = async () => {
        // If the user switched tabs while commit/push was in flight, the
        // active tab in the UI is no longer the one we just modified.
        // Trigger-as-usual would refresh the WRONG tab's graph (whatever
        // is in front now), and the manual setGraph would slot stale data
        // into a different tab's view.  Just invalidate the cache for the
        // origin tab so next time the user lands on it the graph reloads
        // fresh — no spurious reload of the current tab.
        if (tabsStore.activeTabId !== activeTab.id) {
          cacheStore.invalidate(activeTab.id);
          return;
        }
        // Same tab — `graphStore.refresh()` triggers loadGraph in
        // CommitGraph's effect (re-running the lane assignment for the
        // new commit list).  No need for the manual getGraph + setGraph
        // round-trip that used to race the effect-driven reload.
        graphStore.refresh();
      };

      // 3. Optional push.
      if (andPush) {
        const branch = tabsStore.activeTab?.currentBranch;
        if (!branch) {
          await refreshUi();
          uiStore.showToast(`Committed ${oid.slice(0, 7)} (push skipped: no branch)`, 'warning');
          return;
        }
        try {
          await pushBranch(activeTab.id, 'origin', `refs/heads/${branch}`);
          await refreshUi();
          uiStore.showToast(`Committed ${oid.slice(0, 7)} and pushed`, 'success');
        } catch (err) {
          await refreshUi();
          uiStore.showToast(`Committed ${oid.slice(0, 7)} — push failed: ${err}`, 'error');
        }
        return;
      }

      await refreshUi();
      uiStore.showToast(`Committed ${oid.slice(0, 7)}`, 'success');
    } finally {
      committing = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      handleCommit();
    }
  }
</script>

<div class="commit-form">
  <div class="textarea-wrap">
    <textarea
      bind:this={textareaEl}
      class="message-input"
      placeholder="Commit message (Ctrl+Enter to commit)"
      bind:value={message}
      onkeydown={handleKeydown}
      rows="3"
    ></textarea>
    {#if resolvedTemplate && message !== resolvedTemplate}
      <button
        class="template-btn"
        use:tooltip={'Apply commit template'}
        onclick={applyTemplate}
        tabindex="-1"
      >
        <FileText size={11} />
      </button>
    {/if}
  </div>
  <div class="form-footer">
    <label class="amend-toggle">
      <input type="checkbox" bind:checked={amend} />
      <span>Amend last</span>
    </label>
    <Contribution point="arbor:commit-form:action">
      {#snippet item({ payload, fire })}
        {@const p = payload as { label: string; icon?: string; action: string }}
        <button type="button" class="commit-plugin-action" onclick={() => fire()}>
          {#if p.icon}<PluginIcon name={p.icon} size={14} />{/if}
          <span>{p.label}</span>
        </button>
      {/snippet}
    </Contribution>
    <SplitButton
      label={committing ? 'Committing…' : 'Commit'}
      variant="primary"
      direction="up"
      disabled={!canCommit || committing}
      tooltip={tooltipForAction('Commit', 'commit')}
      options={[
        { id: 'commit',       label: 'Commit',        icon: GitCommitHorizontal },
        { id: 'commit-push',  label: 'Commit & Push', icon: Upload },
      ]}
      onclick={() => handleCommit(false)}
      onselect={(id) => {
        if (id === 'commit')      handleCommit(false);
        else if (id === 'commit-push') handleCommit(true);
      }}
    />
  </div>
</div>

<style>
  .commit-form {
    padding: 8px 8px 6px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .textarea-wrap {
    position: relative;
  }

  .message-input {
    width: 100%;
    resize: none;
    padding: 6px 8px;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    transition: border-color var(--transition-fast);
    box-sizing: border-box;
  }
  .message-input:focus { outline: none; border-color: var(--border-focus); }
  .message-input::placeholder { color: var(--text-disabled); }

  /* Template restore button — top-right corner of textarea */
  .template-btn {
    position: absolute;
    top: 4px;
    right: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    opacity: 0.6;
    transition: opacity var(--transition-fast), color var(--transition-fast);
    padding: 0;
  }
  .textarea-wrap:hover .template-btn,
  .template-btn:focus { opacity: 1; }
  .template-btn:hover { color: var(--accent); border-color: var(--accent); }

  .form-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .amend-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    cursor: pointer;
    user-select: none;
  }

  .amend-toggle input[type="checkbox"] {
    accent-color: var(--accent);
    cursor: pointer;
  }

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
  .commit-plugin-action:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

</style>
