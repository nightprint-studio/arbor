<script lang="ts">
  import { GitBranch, FolderOpen, Hash, Lock, Cpu, ExternalLink, Layers, ArrowUp, ArrowDown, FileDiff, Link2 } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import { openPath } from '@tauri-apps/plugin-opener';
  import type { WorktreeInfo } from '$lib/types/git';
  import { uiStore } from '$lib/stores/ui.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    worktree,
    onClose,
    onSwitch,
    onOpenInIde,
  }: {
    worktree: WorktreeInfo;
    onClose: () => void;
    onSwitch: () => void;
    onOpenInIde: (ideId?: string) => void;
  } = $props();

  const PROJECT_TYPE_LABEL: Record<string, string> = {
    rust:         'Rust (Cargo)',
    node_js:      'Node.js (npm/yarn)',
    java_maven:   'Java (Maven)',
    java_gradle:  'Java (Gradle)',
    go:           'Go (go.mod)',
    python:       'Python',
    dot_net:      '.NET',
    cpp:          'C / C++',
    ruby:         'Ruby (Gem)',
    php:          'PHP (Composer)',
    unknown:      'Unknown',
  };

  const PROJECT_TYPE_ICON: Record<string, string> = {
    rust:         '🦀',
    node_js:      '🟩',
    java_maven:   '☕',
    java_gradle:  '☕',
    go:           '🐹',
    python:       '🐍',
    dot_net:      '🔷',
    cpp:          '⚙️',
    ruby:         '💎',
    php:          '🐘',
    unknown:      '📁',
  };

  // Resolve the project label/icon defensively. Falls back to "Unknown / 📁"
  // when project_type is missing, empty, or not in the catalogue — earlier
  // we'd just render the bare snake_case key, which made the row look broken
  // when the backend hadn't computed a type for some reason.
  const projectKey = $derived(
    typeof worktree.project_type === 'string' && worktree.project_type.length > 0
      ? worktree.project_type
      : 'unknown'
  );
  const projectLabel = $derived(PROJECT_TYPE_LABEL[projectKey] ?? PROJECT_TYPE_LABEL.unknown);
  const projectIcon  = $derived(PROJECT_TYPE_ICON[projectKey]  ?? PROJECT_TYPE_ICON.unknown);

  /** Show just the last 2 path segments for display, full on hover. */
  function shortPath(p: string) {
    const parts = p.replace(/\\/g, '/').split('/').filter(Boolean);
    return parts.length <= 2 ? p : '…/' + parts.slice(-2).join('/');
  }

  // Open the worktree directory in the OS file explorer (Explorer on Windows,
  // Finder on macOS, xdg-open on Linux). The opener plugin already has the
  // `opener:allow-open-path` permission for `**` in the default capability,
  // so any absolute path the backend returned is fair game.
  async function openInExplorer() {
    try {
      await openPath(worktree.path);
    } catch (err) {
      uiStore.showToast(`Failed to open folder: ${err}`, 'error');
    }
  }
</script>

<Modal {onClose} width="440px" ariaLabel="Worktree info">
  {#snippet header()}
    <ModalHeader {onClose}>
      {#snippet actions()}
        {#if worktree.branch}
          <Button
            variant="icon"
            size="sm"
            title="Copy arbor:// worktree link"
            ariaLabel="Copy arbor:// worktree link"
            onclick={() => {
              const tabId = tabsStore.activeTabId;
              if (tabId && worktree.branch) {
                void copyDeepLink({ kind: 'branch_worktree', branch: worktree.branch }, tabId);
              }
            }}
          >
            {#snippet iconStart()}<Link2 size={14} />{/snippet}
          </Button>
        {/if}
      {/snippet}
      <span class="modal-icon"><Layers size={15} /></span>
      <span class="modal-title">Workspace Info</span>
      <span class="badges">
        {#if worktree.is_main}
          <Badge variant="tone" tone="stash" size="md">main</Badge>
        {/if}
        {#if worktree.is_current}
          <Badge variant="tone" tone="accent" size="md">current</Badge>
        {/if}
        {#if worktree.is_locked}
          <Badge variant="tone" tone="neutral" size="md">
            {#snippet icon()}<Lock size={10} />{/snippet}
            locked
          </Badge>
        {/if}
      </span>
    </ModalHeader>
  {/snippet}

  <div class="info-rows">
    <!-- Path -->
    <div class="info-row">
      <span class="info-label"><FolderOpen size={13} /> Path</span>
      <span class="info-value info-path-row">
        <span class="info-path" use:tooltip={worktree.path}>{shortPath(worktree.path)}</span>
        <CopyButton
          value={worktree.path}
          variant="icon"
          title="Copy path to clipboard"
          toastSuccess="Worktree path copied"
        />
      </span>
    </div>

    <!-- Branch -->
    <div class="info-row">
      <span class="info-label"><GitBranch size={13} /> Branch</span>
      <span class="info-value">
        {#if worktree.branch}
          <Badge variant="tone" tone="accent">{worktree.branch}</Badge>
        {:else}
          <span class="text-muted">detached HEAD</span>
        {/if}
      </span>
    </div>

    <!-- Commit SHA -->
    <div class="info-row">
      <span class="info-label"><Hash size={13} /> Commit</span>
      <span class="info-value mono">
        {#if worktree.head_sha}
          <span use:tooltip={worktree.head_sha}>{worktree.head_short}</span>
        {:else}
          <span class="text-muted">—</span>
        {/if}
      </span>
    </div>

    <!-- Project type -->
    <div class="info-row">
      <span class="info-label"><Cpu size={13} /> Project</span>
      <span class="info-value">
        <span class="project-type">
          <span class="project-icon">{projectIcon}</span>
          <span class="project-label">{projectLabel}</span>
        </span>
      </span>
    </div>

    <!-- Status divider -->
    <div class="info-divider"></div>

    <!-- Ahead / Behind -->
    <div class="info-row">
      <span class="info-label status-label">Sync</span>
      <span class="info-value">
        {#if worktree.ahead === 0 && worktree.behind === 0}
          <Badge variant="tone" tone="success">Up to date</Badge>
        {:else}
          <span class="status-chips">
            {#if worktree.ahead > 0}
              <Badge variant="tone" tone="info">
                {#snippet icon()}<ArrowUp size={10} />{/snippet}
                {worktree.ahead}
              </Badge>
            {/if}
            {#if worktree.behind > 0}
              <Badge variant="tone" tone="stash">
                {#snippet icon()}<ArrowDown size={10} />{/snippet}
                {worktree.behind}
              </Badge>
            {/if}
          </span>
        {/if}
      </span>
    </div>

    <!-- Local changes -->
    <div class="info-row">
      <span class="info-label status-label">Changes</span>
      <span class="info-value">
        {#if worktree.changes_count === 0}
          <Badge variant="tone" tone="success">Clean</Badge>
        {:else}
          <Badge variant="tone" tone="tag">
            {#snippet icon()}<FileDiff size={10} />{/snippet}
            {worktree.changes_count} file{worktree.changes_count !== 1 ? 's' : ''}
          </Badge>
        {/if}
      </span>
    </div>
  </div>

  {#snippet footer()}
    {#if !worktree.is_current}
      <Button variant="primary" onclick={onSwitch}>
        {#snippet iconStart()}<GitBranch size={13} />{/snippet}
        Switch here
      </Button>
    {/if}
    <Button
      variant="secondary"
      onclick={openInExplorer}
      title="Reveal the worktree directory in the OS file manager"
    >
      {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
      Open in Explorer
    </Button>
    <Button variant="secondary" onclick={() => onOpenInIde()}>
      {#snippet iconStart()}<ExternalLink size={13} />{/snippet}
      Open in IDE
    </Button>
  {/snippet}
</Modal>

<style>
  .modal-icon {
    color: var(--accent);
    display: flex;
    align-items: center;
  }

  .badges {
    display: flex;
    gap: 5px;
    align-items: center;
    margin-left: auto;
  }

  .info-rows {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  .info-row {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 24px;
  }

  .info-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-secondary);
    width: 92px;
    flex-shrink: 0;
  }
  .info-label :global(svg) { color: var(--text-muted); }

  .info-value {
    flex: 1;
    min-width: 0;
    font-size: 12.5px;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    overflow: hidden;
  }
  /* Keep simple text values single-line with ellipsis without clobbering
     wrappers like .project-type / .status-chips / Badge / .info-path-row
     that need to stay flex. */
  .info-value > :global(:not(.project-type):not(.status-chips):not(.badge):not(.info-path-row)) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .info-path-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }
  .info-path {
    font-family: var(--font-code);
    font-size: 11.5px;
    color: var(--text-secondary);
    cursor: default;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }
  .mono {
    font-family: var(--font-code);
    font-size: 12px;
  }

  .project-type {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    padding: 2px 8px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--text-primary);
    line-height: 1.2;
  }

  .project-icon {
    font-size: 13px;
    line-height: 1;
  }
  .project-label {
    color: var(--text-primary);
    font-weight: 500;
  }

  .text-muted {
    color: var(--text-muted);
    font-style: italic;
  }

  .info-divider {
    height: 1px;
    background: var(--border-subtle);
    margin: 2px 0;
  }

  .status-label {
    width: 92px;
  }

  .status-chips {
    display: flex;
    align-items: center;
    gap: 5px;
  }
</style>
