<script lang="ts">
  import Avatar from '../shared/internal/Avatar.svelte';
  import BranchLabel from './BranchLabel.svelte';
  import TicketChip from './TicketChip.svelte';
  import NotesModal from './NotesModal.svelte';
  import FileDiffList from '../diff/FileDiffList.svelte';
  import DiffViewer, { type DiffViewerApi } from '../diff/DiffViewer.svelte';
  import DiffToolbar from '../diff/DiffToolbar.svelte';
  import ResizablePanel from '../layout/ResizablePanel.svelte';
  import { Archive, HardDriveDownload, GitCommit, X, StickyNote, ChevronUp, ChevronDown, Play, CornerDownLeft, Trash2, Link2 } from 'lucide-svelte';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import { applyStashAction, popStashAction, dropStashAction } from '$lib/utils/stash-actions';
  import BottomPanelHeader from '../shared/ui/BottomPanelHeader.svelte';
  import Contribution from '../shared/Contribution.svelte';
  import PluginIcon   from '../plugins/PluginIcon.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { diffStore } from '$lib/stores/diff.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { ticketLinksStore } from '$lib/stores/ticket_links.svelte';
  import { notesStore } from '$lib/stores/notes.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { formatTimestamp, relativeTime } from '$lib/utils/diff-formatter';
  import type { TicketLink } from '$lib/types/git';

  const tab = $derived(tabsStore.activeTab);

  let notesModalOid   = $state<string | null>(null);
  let headerExpanded  = $state(localStorage.getItem('arbor:detail-header-expanded') !== 'false');
  // Imperative handle exposed by the chromeless DiffViewer below — drives
  // the DiffToolbar we render inside the BottomPanelHeader.
  let diffApi = $state<DiffViewerApi | undefined>(undefined);

  function toggleHeader() {
    headerExpanded = !headerExpanded;
    localStorage.setItem('arbor:detail-header-expanded', String(headerExpanded));
  }

  function openNotesModal(oid: string) {
    notesModalOid = oid;
    if (tab) notesStore.load(tab.id, oid);
  }

  // Load notes when selected commit changes
  $effect(() => {
    const d = detail;
    if (d && tab) {
      notesStore.load(tab.id, d.oid);
    }
  });

  function handleChipClick(link: TicketLink) {
    window.dispatchEvent(new CustomEvent('arbor:view-issue', {
      detail: { tracker: link.tracker, ticketId: link.ticket_id },
    }));
  }

  async function handleRemoveLink(sha: string, ticketId: string) {
    if (!tab) return;
    try {
      await ticketLinksStore.removeLink(tab.id, sha, ticketId);
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    }
  }

  const mode   = $derived(graphStore.panelMode);
  const detail = $derived(graphStore.selectedDetail);
  const stash  = $derived(graphStore.selectedStash);
  const files  = $derived(diffStore.files);

  const hasContent = $derived(
    (mode === 'commit' && !!detail) ||
    (mode === 'stash' && !!stash) ||
    (mode === 'workdir' && files.length > 0)
  );

  // Same apply/pop/drop helpers used in the sidebar StashList and the
  // graph's hover bubble — keeps behavior identical (conflict modal,
  // status refresh, graph reload) regardless of where the action fires.
  async function applyStashHere() {
    if (!tab || !stash) return;
    await applyStashAction(tab.id, stash, () => graphStore.refresh());
  }
  async function popStashHere() {
    if (!tab || !stash) return;
    await popStashAction(tab.id, stash, () => graphStore.refresh());
  }
  async function dropStashHere() {
    if (!tab || !stash) return;
    await dropStashAction(tab.id, stash, () => graphStore.refresh());
  }
</script>

<div class="detail-root">
  <BottomPanelHeader title="Commit">
    {#snippet icon()}<GitCommit size={14} />{/snippet}
    {#snippet children()}
      {#if diffStore.selectedFile && diffApi}
        <DiffToolbar
          file={diffStore.selectedFile}
          selectedCount={diffApi.selectedCount}
          currentChunkIdx={diffApi.currentChunkIdx}
          copyDone={diffApi.copyDone}
          onCopyCode={diffApi.copyCode}
          onOpenFullscreen={diffApi.openFullscreen}
          onPrevChunk={diffApi.prevChunk}
          onNextChunk={diffApi.nextChunk}
          onEncodingChange={() => window.dispatchEvent(new CustomEvent('arbor:reload-diff'))}
        />
      {/if}
    {/snippet}
  </BottomPanelHeader>

  <div class="detail-toolbar">
    {#if hasContent}
      {#if mode === 'commit' && detail}

        {#if headerExpanded}
          <!-- ── Expanded header ── -->
          <div class="panel-header">
            <Avatar name={detail.author.name} email={detail.author.email} size={28} />
            <div class="commit-info">
              <div class="summary-row">
                <span class="commit-summary">{detail.summary}</span>
                {#if detail.refs.length > 0}
                  <div class="refs">
                    {#each detail.refs as ref}
                      <BranchLabel {ref} colorIndex={graphStore.selectedNode?.color_index} />
                    {/each}
                  </div>
                {/if}
                <button class="header-toggle" onclick={toggleHeader} use:tooltip={'Collapse header'}>
                  <ChevronUp size={11} />
                </button>
              </div>
              <div class="commit-meta">
                <span class="author">{detail.author.name}</span>
                <span class="sep">·</span>
                <span class="time" use:tooltip={formatTimestamp(detail.timestamp)}>{relativeTime(detail.timestamp)}</span>
                <span class="sep">·</span>
                <code class="sha">{detail.short_oid}</code>
                <button
                  class="meta-icon-btn"
                  use:tooltip={'Copy arbor:// link to this commit'}
                  onclick={() => tab && copyDeepLink({ kind: 'commit_jump', sha: detail.oid }, tab.id)}
                  aria-label="Copy arbor:// link to this commit"
                >
                  <Link2 size={11} />
                </button>
                <span class="sep">·</span>
                <button
                  class="notes-meta-btn"
                  class:has-notes={notesStore.hasNotes(detail.oid)}
                  use:tooltip={notesStore.hasNotes(detail.oid)
                    ? { content: `${notesStore.noteCount(detail.oid)} note${notesStore.noteCount(detail.oid) !== 1 ? 's' : ''}`, description: 'Click to view' }
                    : 'Add note'}
                  onclick={() => openNotesModal(detail.oid)}
                >
                  <StickyNote size={11} />
                  {#if notesStore.hasNotes(detail.oid)}
                    <span>{notesStore.noteCount(detail.oid)}</span>
                  {:else}
                    <span class="notes-empty-inline">Add note</span>
                  {/if}
                </button>
              </div>
            </div>
          </div>

          {#if detail.body}
            <div class="commit-body">{detail.body}</div>
          {/if}

          <!-- Plugin-contributed inline actions (arbor:commit-detail:action).
               Fired with the active commit oid in ctx.oid. Renders below the
               commit body / above linked tickets so plugin actions sit close
               to the commit metadata. -->
          <div class="plugin-actions-row">
            <Contribution point="arbor:commit-detail:action">
              {#snippet item({ payload, fire })}
                {@const p = payload as { label: string; icon?: string; action: string }}
                <button
                  type="button"
                  class="plugin-commit-action"
                  onclick={() => fire({ oid: graphStore.selectedOid ?? '' })}
                >
                  {#if p.icon}<PluginIcon name={p.icon} size={14} />{/if}
                  <span>{p.label}</span>
                </button>
              {/snippet}
            </Contribution>
          </div>

          {#if ticketLinksStore.isEnabled()}
            {@const detailLinks = ticketLinksStore.getLinks(detail.oid)}
            {#if detailLinks.length > 0}
              <div class="linked-tickets">
                {#each detailLinks as link (link.ticket_id)}
                  <div class="linked-ticket-row">
                    <TicketChip {link} onclick={handleChipClick} />
                    {#if link.source === 'manual'}
                      <button class="unlink-btn" use:tooltip={'Remove link'}
                        onclick={() => handleRemoveLink(detail.oid, link.ticket_id)}>
                        <X size={10} />
                      </button>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
          {/if}

        {:else}
          <!-- ── Collapsed header: single compact line ── -->
          <div class="panel-header panel-header-collapsed">
            <Avatar name={detail.author.name} email={detail.author.email} size={20} />
            <span class="commit-summary-compact">{detail.summary}</span>
            <code class="sha sha-compact">{detail.short_oid}</code>
            {#if detail.refs.length > 0}
              <BranchLabel ref={detail.refs[0]} colorIndex={graphStore.selectedNode?.color_index} />
              {#if detail.refs.length > 1}
                <span class="refs-overflow">+{detail.refs.length - 1}</span>
              {/if}
            {/if}
            <button class="header-toggle header-toggle-expand" onclick={toggleHeader} use:tooltip={'Expand header'}>
              <ChevronDown size={11} />
            </button>
          </div>
        {/if}

      {:else if mode === 'stash' && stash}
        <div class="panel-header panel-header-alt">
          <span class="mode-icon stash-icon"><Archive size={16} /></span>
          <div class="commit-info">
            <div class="commit-summary">{stash.message || `stash@{${stash.index}}`}</div>
            <div class="commit-meta">
              <span class="sha-muted">stash@{'{' + stash.index + '}'}</span>
              <span class="sep">·</span>
              <code class="sha">{stash.oid.slice(0, 8)}</code>
            </div>
          </div>
          <!-- Apply / Pop / Drop — mirrors the sidebar StashList row actions
               and the graph bubble's hover toolbar so the user has the same
               three buttons available wherever they arrived from. -->
          <div class="stash-toolbar">
            <button class="stash-tb-btn" use:tooltip={'Apply stash'} onclick={applyStashHere}>
              <Play size={12} />
            </button>
            <button class="stash-tb-btn" use:tooltip={'Pop stash'} onclick={popStashHere}>
              <CornerDownLeft size={12} />
            </button>
            <button class="stash-tb-btn danger" use:tooltip={'Drop stash'} onclick={dropStashHere}>
              <Trash2 size={12} />
            </button>
          </div>
        </div>

      {:else if mode === 'workdir'}
        <div class="panel-header panel-header-alt">
          <span class="mode-icon workdir-icon"><HardDriveDownload size={16} /></span>
          <div class="commit-info">
            <div class="commit-summary">Working Directory Changes</div>
            <div class="commit-meta">
              <span class="sha-muted">{files.length} file{files.length !== 1 ? 's' : ''} changed</span>
            </div>
          </div>
        </div>
      {/if}
    {/if}
  </div>

  {#if hasContent}
    <!-- Files + Diff side by side -->
    <div class="diff-area">
      <ResizablePanel direction="horizontal" initialSize={220} minSize={120} maxSize={400}>
        <FileDiffList {files} />
      </ResizablePanel>
      <DiffViewer
        file={diffStore.selectedFile}
        path={diffStore.selectedFile?.path}
        onEncodingChange={() => window.dispatchEvent(new CustomEvent('arbor:reload-diff'))}
        chromeless
        bind:api={diffApi}
      />
    </div>
  {:else}
    <div class="placeholder">
      <GitCommit size={28} class="placeholder-icon" />
      <span>Select a commit, stash, or view local changes</span>
    </div>
  {/if}
</div>

{#if notesModalOid}
  {@const modalOid = notesModalOid}
  <NotesModal
    commitOid={modalOid}
    shortOid={modalOid.slice(0, 7)}
    onClose={() => (notesModalOid = null)}
  />
{/if}

<style>
  /* Bottom-panel root: column flex with the BottomPanelHeader at top
     and the diff body below, mirroring the sidebar PanelShell layout. */
  .detail-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
    background: var(--bg-base);
  }

  /* Toolbar slot — holds the dynamic commit/stash/workdir metadata block.
     `flex-shrink: 0` so it never collapses; the diff-area below scrolls. */
  .detail-toolbar {
    flex-shrink: 0;
  }

  /* ── Expanded panel-header ── */
  .panel-header {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px 6px;
    flex-shrink: 0;
  }

  .panel-header-alt {
    align-items: center;
    padding: 8px 12px;
  }

  .mode-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-md);
    flex-shrink: 0;
  }
  .stash-icon   { background: rgba(226,163,53,0.12); color: var(--warning); }
  .workdir-icon { background: rgba(77,120,204,0.12);  color: var(--accent);  }

  /* Stash header toolbar — same three actions as the sidebar StashList
     row + the graph bubble hover toolbar. Compact icon-only buttons so
     they don't compete with the stash summary for space. */
  .stash-toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }
  .stash-tb-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px; height: 22px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .stash-tb-btn:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .stash-tb-btn.danger:hover { color: var(--error); background: var(--error-subtle); }

  .commit-info { flex: 1; min-width: 0; }

  /* Summary + refs + toggle in one row */
  .summary-row {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .commit-summary {
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-md);
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .refs {
    display: flex;
    gap: 3px;
    flex-wrap: wrap;
    justify-content: flex-end;
    flex-shrink: 0;
    max-width: 220px;
  }

  /* Collapse/expand toggle button */
  .header-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-disabled);
    flex-shrink: 0;
    padding: 0;
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .header-toggle:hover { color: var(--text-secondary); background: var(--bg-hover); }

  /* ── Meta row ── */
  .commit-meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    margin-top: 3px;
    color: var(--text-secondary);
  }

  .sep { color: var(--text-disabled); }
  .sha { font-family: var(--font-code); color: var(--text-muted); }
  .sha-muted { color: var(--text-muted); }

  /* Notes button — inline in meta row */
  .notes-meta-btn {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    cursor: pointer;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 10px;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .notes-meta-btn:hover { background: var(--bg-hover); border-color: var(--border); color: var(--text-secondary); }

  /* Tiny icon-only button for the meta row (deep-link copy, …). */
  .meta-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    cursor: pointer;
    color: var(--text-muted);
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .meta-icon-btn:hover {
    background: var(--bg-hover);
    border-color: var(--border-subtle);
    color: var(--text-secondary);
  }

  .notes-meta-btn.has-notes {
    background: rgba(77,120,204,0.10);
    border-color: rgba(77,120,204,0.3);
    color: var(--accent);
  }
  .notes-meta-btn.has-notes:hover { background: rgba(77,120,204,0.18); }
  .notes-empty-inline { color: var(--text-disabled); }

  /* ── Commit body ── */
  .commit-body {
    padding: 0 10px 6px 46px;   /* 46px = avatar(28) + gap(8) + left-pad(10) */
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    white-space: pre-wrap;
    flex-shrink: 0;
    max-height: 48px;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }

  /* ── Plugin-contributed action row (arbor:commit-detail:action) ── */
  .plugin-actions-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 2px 10px 4px 46px;
    flex-shrink: 0;
  }
  /* Hide the row entirely when no plugin contributed — avoids reserving
     vertical space on the very common "no plugins" case. */
  .plugin-actions-row:empty { display: none; }

  /* ── Linked tickets row ── */
  .linked-tickets {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding: 2px 10px 6px 46px;
    flex-shrink: 0;
  }

  .linked-ticket-row {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .unlink-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    padding: 0;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-disabled);
    border-radius: 2px;
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .unlink-btn:hover { color: var(--text-muted); background: var(--bg-hover); }

  /* ── Collapsed header: single compact line ── */
  .panel-header-collapsed {
    align-items: center;
    padding: 5px 8px;
    gap: 6px;
  }

  .commit-summary-compact {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    font-weight: 500;
    color: var(--text-primary);
  }

  .sha-compact {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-disabled);
    flex-shrink: 0;
  }

  .refs-overflow {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border-radius: 999px;
    padding: 0 5px;
    flex-shrink: 0;
  }

  .header-toggle-expand { margin-left: auto; }

  .diff-area {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .placeholder {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
  }

  :global(.placeholder-icon) { color: var(--text-disabled); }

  /* ── Plugin-contributed commit detail actions ───────────────────────────── */
  .plugin-commit-action {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
  }
  .plugin-commit-action:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }
</style>
