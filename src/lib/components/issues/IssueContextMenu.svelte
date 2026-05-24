<script lang="ts">
  import { ExternalLink, GitBranch, Copy, ArrowRight, Eye } from 'lucide-svelte';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import { linearBranchNameForIssue } from '$lib/ipc/issues';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { Issue } from '$lib/types/issues';

  let { issue, x, y, onClose, onOpenDetail }: {
    issue:        Issue;
    x:            number;
    y:            number;
    onClose:      () => void;
    onOpenDetail: () => void;
  } = $props();

  // Adjust position so menu doesn't overflow the viewport
  const menuW = 200, menuH = 180;
  const left = $derived(Math.min(x, window.innerWidth  - menuW - 8));
  const top  = $derived(Math.min(y, window.innerHeight - menuH - 8));

  async function copyIdentifier() {
    await copyToClipboard(issue.identifier, { successToast: `Copied ${issue.identifier}` });
    onClose();
  }

  async function copyTitle() {
    await copyToClipboard(issue.title, { successToast: 'Title copied' });
    onClose();
  }

  async function copyBranchName() {
    const name = await linearBranchNameForIssue(issue);
    await copyToClipboard(name, { successToast: `Branch name copied: ${name}` });
    onClose();
  }

  async function openBrowser() {
    try { await openUrl(issue.url); } catch { /* ignore */ }
    onClose();
  }

  function openDetail() { onOpenDetail(); }

  // Statuses available from filter options for quick transition
  const statuses = $derived((issuesStore.filterOptions?.statuses ?? []).filter(
    s => s.id !== issue.status.id
  ).slice(0, 4));
</script>

<button type="button" aria-label="Close menu" class="cm-backdrop" onclick={onClose}></button>
<div class="cm" style="left:{left}px; top:{top}px;" role="menu">
  <button class="cm-item" onclick={openDetail}>
    <Eye size={13} /> View details
  </button>
  <div class="cm-sep"></div>
  <button class="cm-item" onclick={openBrowser}>
    <ExternalLink size={13} /> Open in tracker
  </button>
  <button class="cm-item" onclick={copyIdentifier}>
    <Copy size={13} /> Copy {issue.identifier}
  </button>
  <button class="cm-item" onclick={copyTitle}>
    <Copy size={13} /> Copy title
  </button>
  <button class="cm-item" onclick={copyBranchName}>
    <GitBranch size={13} /> Copy branch name
  </button>
  {#if statuses.length > 0}
    <div class="cm-sep"></div>
    <div class="cm-group">Transition to…</div>
    {#each statuses as st}
      <button class="cm-item" onclick={async () => {
        await issuesStore.transitionIssue(issue.id, st.id);
        onClose();
      }}>
        <span class="cm-status-dot" style="background:{st.color}"></span>
        {st.name}
      </button>
    {/each}
  {/if}
</div>

<style>
  .cm-backdrop {
    position: fixed; inset: 0; z-index: var(--z-backdrop);
    background: transparent; border: none; padding: 0; cursor: default;
  }
  .cm {
    position: fixed; z-index: var(--z-tooltip);
    min-width: 200px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 4px;
    box-shadow: 0 8px 28px rgba(0,0,0,0.45);
    font-family: var(--font-ui-sans);
    animation: popIn 80ms cubic-bezier(0.16,1,0.3,1);
  }
  @keyframes popIn { from { opacity:0; transform:scale(0.96); } to { opacity:1; transform:none; } }
  .cm-item {
    display: flex; align-items: center; gap: 8px;
    width: 100%; padding: 6px 10px; text-align: left;
    font-size: 12px; color: var(--text-primary);
    background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer; font-family: var(--font-ui-sans);
    transition: background var(--transition-fast);
  }
  .cm-item:hover { background: var(--bg-hover); }
  .cm-sep { height: 1px; background: var(--border-subtle); margin: 3px 6px; }
  .cm-group {
    padding: 5px 10px 2px;
    font-size: 9px; font-weight: 600; letter-spacing: 0.5px;
    text-transform: uppercase; color: var(--text-muted);
  }
  .cm-status-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
</style>
