<script lang="ts">
  import {
    ExternalLink, GitBranch, ChevronDown, Send, Loader,
    Calendar, User, Tag, Layers, CircleDot, GitCommit, CornerDownLeft,
    Paperclip, Download, FileText, FileImage, FileArchive, FileVideo, FileAudio, File as FileIcon,
    Pin, PinOff, AlertCircle,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import BrandTile from '$lib/components/shared/internal/BrandTile.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import { renderMarkdown } from '$lib/utils/markdown';
  import { htmlToText, installListAwareCopy } from '$lib/utils/html-to-text';
  import { issuesStore, type IssueProvider } from '$lib/stores/issues.svelte';
  import { branchNameForIssue, jiraDownloadAttachment } from '$lib/ipc/issues';
  import { findCommitsForTicket } from '$lib/ipc/ticket_links';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { relativeTime } from '$lib/utils/diff-formatter';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { Issue, IssueStatus, IssueAttachment } from '$lib/types/issues';
  import type { LinkedCommitRef } from '$lib/types/git';

  /**
   * `provider` is captured at open time and pinned for the lifetime of the
   * modal. This is what makes the dialog self-contained: a Linear ticket
   * keeps talking to Linear (status dropdown, transitions, comments) even
   * when the sidebar has switched to a Jira-configured repo.
   */
  let { issue, provider, onClose, onRestoreFromScratch }: {
    issue: Issue;
    provider: IssueProvider;
    onClose: () => void;
    onRestoreFromScratch?: () => void | Promise<void>;
  } = $props();

  let mainEl = $state<HTMLElement | null>(null);

  // Reformat Ctrl+C clipboard text when the selection contains a list, so
  // bullets (rendered as CSS pseudo-elements) survive the round-trip.
  $effect(() => {
    if (!mainEl) return;
    return installListAwareCopy(mainEl);
  });

  // Per-attachment download state (id → 'idle' | 'downloading' | 'done' | 'error')
  let downloadStates  = $state<Record<string, 'idle' | 'downloading' | 'done' | 'error'>>({});
  // Currently open save picker (null when closed). The picker is the in-app
  // FilePickerModal — never the native OS dialog.
  let pickerForAtt    = $state<IssueAttachment | null>(null);

  let commentBody          = $state('');
  let commentSending       = $state(false);
  let transitioning        = $state(false);
  let statusDropOpen       = $state(false);
  let branchCopied         = $state(false);
  let loadingOptions       = $state(false);
  let linkedCommits        = $state<LinkedCommitRef[]>([]);
  let linkedCommitsLoading = $state(false);

  async function openStatusDrop() {
    if (statusDropOpen) { statusDropOpen = false; return; }
    statusDropOpen = true;
    if (!issuesStore.getFilterOptionsFor(provider)) {
      loadingOptions = true;
      try { await issuesStore.loadFilterOptions(provider); } finally { loadingOptions = false; }
    }
  }

  function labelChipStyle(color: string): string {
    const hex = color.startsWith('#') ? color : `#${color}`;
    if (hex.length < 7) return '';
    const r = parseInt(hex.slice(1, 3), 16) / 255;
    const g = parseInt(hex.slice(3, 5), 16) / 255;
    const b = parseInt(hex.slice(5, 7), 16) / 255;
    const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if (lum < 0.1) {
      return `background:rgba(160,160,160,0.12);color:var(--text-secondary);border:1px solid rgba(160,160,160,0.25)`;
    }
    return `background:${hex}22;color:${hex};border:1px solid ${hex}55`;
  }

  // Use the store's selectedIssue when it matches — that's the full payload
  // (description + comments) installed by `selectAndLoadIssue`, plus any
  // updates from transitions/comments routed through this modal.
  //
  // We deliberately do NOT fall back to `issuesStore.issues[]`: that list is
  // scoped to the sidebar's active provider, so reading from it when the
  // sidebar has switched to a different tracker would pull the wrong record.
  const liveIssue = $derived(
    (issuesStore.selectedIssue?.id === issue.id ? issuesStore.selectedIssue : null) ?? issue
  );

  function statusTypeClass(type: string) {
    if (type === 'completed') return 'st-done';
    if (type === 'started')   return 'st-progress';
    if (type === 'cancelled') return 'st-cancelled';
    return 'st-todo';
  }

  function priorityIcon(p: number): string {
    return ['—', '🔴', '🟠', '🟡', '🔵'][p] ?? '—';
  }

  function timeAgo(iso: string | null | undefined): string {
    if (!iso) return '—';
    const d = new Date(iso);
    if (isNaN(d.getTime())) return '—';
    const s = Math.floor((Date.now() - d.getTime()) / 1000);
    if (s < 60)      return `${s}s ago`;
    if (s < 3600)    return `${Math.floor(s / 60)}m ago`;
    if (s < 86400)   return `${Math.floor(s / 3600)}h ago`;
    if (s < 2592000) return `${Math.floor(s / 86400)}d ago`;
    return d.toLocaleDateString();
  }

  function formatDate(iso: string | null | undefined): string {
    if (!iso) return '—';
    try {
      const d = new Date(iso);
      return isNaN(d.getTime()) ? '—' : d.toLocaleDateString();
    } catch { return '—'; }
  }

  // Distinct status list from the modal's pinned provider — never the
  // sidebar's. A Linear modal must keep showing Linear statuses even after
  // the sidebar has switched to Jira.
  const availableStatuses = $derived(issuesStore.getFilterOptionsFor(provider)?.statuses ?? []);

  async function transitionTo(status: IssueStatus) {
    statusDropOpen = false;
    if (status.id === liveIssue.status.id) return;
    transitioning = true;
    try {
      await issuesStore.transitionIssue(liveIssue.id, status.id, provider);
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    } finally {
      transitioning = false;
    }
  }

  async function sendComment() {
    const body = commentBody.trim();
    if (!body) return;
    commentSending = true;
    try {
      await issuesStore.addComment(liveIssue.id, body, provider);
      commentBody = '';
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    } finally {
      commentSending = false;
    }
  }

  async function createBranch() {
    try {
      const name = await branchNameForIssue(liveIssue);
      if (await copyToClipboard(name, { successToast: `Branch name copied: ${name}`, errorToast: true })) {
        branchCopied = true;
        setTimeout(() => { branchCopied = false; }, 2000);
      }
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    }
  }

  async function openInBrowser() {
    try { await openUrl(liveIssue.url); } catch { /* ignore */ }
  }

  const renderDescription = renderMarkdown;

  /** Returns plain text for clipboard. Markdown bodies pass through as-is
   *  (most users want the source); HTML bodies are walked into a readable
   *  text form — `<ul>` → "- item", `<ol>` → "1. item", paragraphs and
   *  headings get newline boundaries. */
  function bodyAsPlainText(body: string, format: string | null | undefined): string {
    return format === 'html' ? htmlToText(body) : body;
  }

  // Comment composer state. `me` is read from the modal's pinned provider —
  // never the sidebar's authStatus, which may belong to a different tracker.
  const me    = $derived(issuesStore.getFilterOptionsFor(provider)?.me ?? null);
  const dirty = $derived(commentBody.trim().length > 0);

  // Lazy-load filter options for this provider on mount, so the composer
  // avatar and the status dropdown pre-fetch their data instead of waiting
  // for the first user click. Fire-and-forget — failure surfaces via the
  // status dropdown's existing error path.
  $effect(() => {
    if (!issuesStore.getFilterOptionsFor(provider)) {
      void issuesStore.loadFilterOptions(provider);
    }
  });

  // Stable primitive derived so the effect below only re-runs when the identifier
  // string actually changes — not on every object reference change of liveIssue
  // (e.g. when selectAndLoadIssue replaces the light version with the full detail).
  const stableIdentifier = $derived(liveIssue.identifier);

  // ── Linked Commits source-tab resolution ─────────────────────────────────
  // Default: pinned to the tab the issue was opened from (self-contained).
  // The user can unpin to fall back to the current active tab — useful when
  // they're cross-referencing the same ticket across multiple repos.
  // Local state, not persisted: a fresh open of the same issue restarts
  // pinned to source.
  let usingCurrentRepo = $state(false);
  const sourceTabId    = $derived(issuesStore.selectedIssueSourceTab);
  const sourceTab      = $derived(sourceTabId
    ? tabsStore.tabs.find(t => t.id === sourceTabId) ?? null
    : null);
  // The tab we actually run the git lookup against. Null when no tab can
  // be resolved (source missing AND user hasn't fallen back to current).
  const lookupTab      = $derived(usingCurrentRepo ? tabsStore.activeTab : sourceTab);
  // "Source repo not open in this session": only meaningful while pinned.
  const sourceMissing  = $derived(!usingCurrentRepo && sourceTabId !== null && sourceTab === null);

  // Lazy-load commits linked to this issue. Re-runs when the identifier or
  // the lookup tab changes (pin/unpin, tab close).
  $effect(() => {
    const tabId      = lookupTab?.id ?? null;
    const identifier = stableIdentifier; // primitive → effect won't re-run unless ID changes
    if (!tabId) {
      linkedCommits        = [];
      linkedCommitsLoading = false;
      return;
    }

    linkedCommits        = [];
    linkedCommitsLoading = true;
    let cancelled = false;

    findCommitsForTicket(tabId, identifier)
      .then(c => { if (!cancelled) linkedCommits = c; })
      .catch(() => {})
      .finally(() => { if (!cancelled) linkedCommitsLoading = false; });

    return () => { cancelled = true; };
  });

  // Whether to render the Linked Commits section at all. Always render when
  // the source tab is missing (so we can explain why), otherwise keep the
  // original behavior (hide when there's nothing to show).
  const showLinkedCommitsSection = $derived(
    linkedCommitsLoading || linkedCommits.length > 0 || sourceMissing
  );

  function goToCommit(sha: string) {
    // If the linked-commits lookup is running against a tab different from
    // the active one (typical when the modal is parked while the user
    // browses another repo), switch to it first so the graph view actually
    // contains the commit we're about to select.
    const target = lookupTab;
    if (target && target.id !== tabsStore.activeTabId) {
      tabsStore.setActive(target.id);
    }
    graphStore.selectCommit(sha);
    onClose();
  }

  // ── Attachments ──────────────────────────────────────────────────────────
  function attachmentIconFor(mime: string | null, filename: string) {
    const m = (mime ?? '').toLowerCase();
    const ext = filename.toLowerCase().split('.').pop() ?? '';
    if (m.startsWith('image/'))                                  return FileImage;
    if (m.startsWith('video/'))                                  return FileVideo;
    if (m.startsWith('audio/'))                                  return FileAudio;
    if (m === 'application/pdf' || ext === 'pdf')                return FileText;
    if (m === 'application/zip' || ['zip','rar','7z','tar','gz','bz2','xz'].includes(ext)) return FileArchive;
    if (m.startsWith('text/') ||
        ['txt','md','log','csv','json','xml','yaml','yml','toml'].includes(ext))          return FileText;
    return FileIcon;
  }

  function formatBytes(n: number | null): string {
    if (n == null) return '';
    if (n < 1024)         return `${n} B`;
    if (n < 1024 * 1024)  return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 ** 3)    return `${(n / 1024 / 1024).toFixed(1)} MB`;
    return `${(n / 1024 ** 3).toFixed(2)} GB`;
  }

  function suggestedExtensions(filename: string, mime: string | null): string[] | undefined {
    const ext = filename.toLowerCase().split('.').pop();
    if (ext && ext.length <= 8) return [ext];
    if (mime?.startsWith('image/')) {
      const sub = mime.split('/')[1];
      return sub ? [sub] : undefined;
    }
    return undefined;
  }

  /** Open the in-app FilePickerModal (save mode) for this attachment.
   *  No file is fetched until the user confirms a destination. */
  function requestDownload(att: IssueAttachment) {
    if (downloadStates[att.id] === 'downloading') return;
    pickerForAtt = att;
  }

  async function performDownload(destPath: string) {
    const att = pickerForAtt;
    pickerForAtt = null;
    if (!att) return;
    downloadStates = { ...downloadStates, [att.id]: 'downloading' };
    try {
      await jiraDownloadAttachment(att.contentUrl, destPath);
      downloadStates = { ...downloadStates, [att.id]: 'done' };
      uiStore.showToast(`Downloaded ${att.filename}`, 'success');
      setTimeout(() => {
        downloadStates = { ...downloadStates, [att.id]: 'idle' };
      }, 2500);
    } catch (e) {
      downloadStates = { ...downloadStates, [att.id]: 'error' };
      uiStore.showToast(`Download failed: ${e}`, 'error');
      setTimeout(() => {
        downloadStates = { ...downloadStates, [att.id]: 'idle' };
      }, 3000);
    }
  }

  function getCommitRefs(sha: string): { name: string; isCurrent: boolean; type: string }[] {
    // `graphStore.graphData` belongs to whichever tab is active right now.
    // When the linked-commits lookup runs against a different tab (source
    // tab pinned while the user browses another repo), the refs don't
    // apply — skip them instead of showing wrong annotations.
    if (!lookupTab || lookupTab.id !== tabsStore.activeTabId) return [];
    const refs = graphStore.graphData?.nodes.find(n => n.oid === sha)?.refs ?? [];
    return refs.map(r => ({ name: r.name, isCurrent: r.is_current, type: r.ref_type }));
  }

  // Group statuses by type for dropdown
  const statusGroups = $derived((() => {
    const order = ['backlog', 'unstarted', 'started', 'completed', 'cancelled'];
    const groups: Record<string, IssueStatus[]> = {};
    for (const s of availableStatuses) {
      if (!groups[s.statusType]) groups[s.statusType] = [];
      groups[s.statusType].push(s);
    }
    return order.filter(o => groups[o]).map(o => ({ type: o, items: groups[o] }));
  })());
</script>

<Modal
  {onClose}
  width="min(1180px, 92vw)" height="90vh"
  padBody={false}
  ariaLabel={liveIssue.title}
  minimizable
  parkId={`issue-${liveIssue.id}`}
  parkTitle={`${liveIssue.identifier} · ${liveIssue.title}`}
  parkIcon={CircleDot}
  {onRestoreFromScratch}
>
  {#snippet header()}
    <ModalHeader {onClose}>
      <BrandTile brand={provider} size={14} tileSize={22} />
      <button
        type="button"
        class="modal-identifier modal-identifier-btn"
        use:tooltip={'Click to copy'}
        onclick={() => copyToClipboard(liveIssue.identifier, { successToast: `Copied ${liveIssue.identifier}`, errorToast: true })}
      >{liveIssue.identifier}</button>
      {#if liveIssue.team}
        <span class="modal-team">{liveIssue.team.name}</span>
      {/if}
      {#snippet actions()}
        <button class="hdr-btn" onclick={openInBrowser} use:tooltip={'Open in tracker'}>
          <ExternalLink size={14} />
        </button>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <!-- Body: sidebar + main -->
  <div class="idm-body">

      <!-- ── Left sidebar: metadata ── -->
      <aside class="modal-sidebar">

        <!-- Status (with dropdown to transition) -->
        <div class="meta-section">
          <div class="meta-label"><CircleDot size={11} /> Status</div>
          <div class="status-dropdown-wrap">
            <button
              class="status-btn {statusTypeClass(liveIssue.status.statusType)}"
              onclick={openStatusDrop}
              disabled={transitioning}
            >
              <span class="status-dot-sm" style="background:{liveIssue.status.color}"></span>
              {#if transitioning}<Loader size={11} class="spin" />{/if}
              {liveIssue.status.name}
              <ChevronDown size={11} />
            </button>
            {#if statusDropOpen}
              <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => (statusDropOpen = false)}></button>
              <div class="status-drop">
                {#if loadingOptions}
                  <div class="drop-empty"><Loader size={12} class="spin" /> Loading…</div>
                {:else if statusGroups.length === 0}
                  <div class="drop-empty">No statuses available</div>
                {:else}
                  {#each statusGroups as grp}
                    <div class="drop-group">{grp.type}</div>
                    {#each grp.items as st}
                      <button
                        class="drop-item"
                        class:drop-item-active={st.id === liveIssue.status.id}
                        onclick={() => transitionTo(st)}
                      >
                        <span class="status-dot-sm" style="background:{st.color}"></span>
                        {st.name}
                      </button>
                    {/each}
                  {/each}
                {/if}
              </div>
            {/if}
          </div>
        </div>

        <!-- Priority -->
        <div class="meta-section">
          <div class="meta-label">Priority</div>
          <div class="meta-value">
            <span class="priority-icon">{priorityIcon(liveIssue.priority)}</span>
            {liveIssue.priorityLabel}
          </div>
        </div>

        <!-- Assignee -->
        <div class="meta-section">
          <div class="meta-label"><User size={11} /> Assignee</div>
          <div class="meta-value">
            {#if liveIssue.assignee}
              <div class="user-row">
                {#if liveIssue.assignee.avatarUrl}
                  <img class="user-avatar" src={liveIssue.assignee.avatarUrl} alt={liveIssue.assignee.displayName} />
                {:else}
                  <span class="user-avatar-ph">{(liveIssue.assignee.displayName ?? liveIssue.assignee.email ?? '?')[0]}</span>
                {/if}
                <span>{liveIssue.assignee.displayName}</span>
              </div>
            {:else}
              <span class="meta-empty">Unassigned</span>
            {/if}
          </div>
        </div>

        <!-- Labels -->
        {#if liveIssue.labels.length > 0}
          <div class="meta-section">
            <div class="meta-label"><Tag size={11} /> Labels</div>
            <div class="meta-labels">
              {#each liveIssue.labels as lbl}
                <span class="meta-label-chip" style={labelChipStyle(lbl.color)}>
                  {lbl.name}
                </span>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Project -->
        {#if liveIssue.project}
          <div class="meta-section">
            <div class="meta-label"><Layers size={11} /> Project</div>
            <div class="meta-value">
              {#if liveIssue.project.color}
                <span class="project-dot" style="background:{liveIssue.project.color}"></span>
              {/if}
              {liveIssue.project.name}
            </div>
          </div>
        {/if}

        <!-- Cycle -->
        {#if liveIssue.cycle}
          <div class="meta-section">
            <div class="meta-label">Cycle</div>
            <div class="meta-value">{liveIssue.cycle.name || `#${liveIssue.cycle.number}`}</div>
          </div>
        {/if}

        <!-- Due date -->
        {#if liveIssue.dueDate}
          <div class="meta-section">
            <div class="meta-label"><Calendar size={11} /> Due</div>
            <div class="meta-value">{formatDate(liveIssue.dueDate)}</div>
          </div>
        {/if}

        <!-- Estimate -->
        {#if liveIssue.estimate != null}
          <div class="meta-section">
            <div class="meta-label">Estimate</div>
            <div class="meta-value">{liveIssue.estimate} pts</div>
          </div>
        {/if}

        <!-- Dates -->
        <div class="meta-section">
          <div class="meta-label">Created</div>
          <div class="meta-value meta-small">{timeAgo(liveIssue.createdAt)}</div>
        </div>
        <div class="meta-section">
          <div class="meta-label">Updated</div>
          <div class="meta-value meta-small">{timeAgo(liveIssue.updatedAt)}</div>
        </div>

        <!-- Actions -->
        <div class="sidebar-actions">
          <button class="action-btn" onclick={openInBrowser}>
            <ExternalLink size={12} /> Open in tracker
          </button>
          <button class="action-btn" onclick={createBranch} use:tooltip={'Copy suggested branch name'}>
            <GitBranch size={12} />
            {branchCopied ? 'Copied!' : 'Copy branch name'}
          </button>
        </div>
      </aside>

      <!-- ── Main area: title + description + comments ── -->
      <main class="modal-main" bind:this={mainEl}>
        <div class="issue-title-row">
          <h1 class="issue-title">{liveIssue.title}</h1>
          <span class="issue-title-copy">
            <CopyButton
              value={liveIssue.title}
              variant="icon"
              title="Copy title"
              toastSuccess="Title copied"
            />
          </span>
        </div>

        {#if issuesStore.detailLoading && !liveIssue.description}
          <div class="issue-desc-skeleton">
            <div class="sk-line" style="width:85%"></div>
            <div class="sk-line" style="width:68%"></div>
            <div class="sk-line" style="width:77%"></div>
          </div>
        {:else if liveIssue.description}
          <div class="issue-description" class:rich-html={liveIssue.descriptionFormat === 'html'}>
            <div class="copy-overlay">
              <CopyButton
                value={() => bodyAsPlainText(liveIssue.description ?? '', liveIssue.descriptionFormat)}
                variant="icon"
                title="Copy description"
                toastSuccess="Description copied"
              />
            </div>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
            {@html liveIssue.descriptionFormat === 'html'
              ? liveIssue.description
              : renderDescription(liveIssue.description)}
          </div>
        {:else}
          <p class="issue-no-desc">No description.</p>
        {/if}

        <!-- Attachments -->
        {#if liveIssue.attachments && liveIssue.attachments.length > 0}
          <div class="attachments-section">
            <div class="attachments-header">
              <Paperclip size={13} />
              Attachments
              <span class="attachments-count">{liveIssue.attachments.length}</span>
            </div>

            <div class="attachments-grid">
              {#each liveIssue.attachments as att (att.id)}
                {@const Icon  = attachmentIconFor(att.mimeType, att.filename)}
                {@const state = downloadStates[att.id] ?? 'idle'}
                {@const isImage = (att.mimeType ?? '').startsWith('image/')}
                <button
                  class="attachment"
                  class:att-downloading={state === 'downloading'}
                  class:att-done={state === 'done'}
                  class:att-error={state === 'error'}
                  onclick={() => requestDownload(att)}
                  disabled={state === 'downloading'}
                  use:tooltip={'Click to download'}
                >
                  <div class="att-icon-wrap" class:att-icon-image={isImage}>
                    {#if isImage && att.thumbnailUrl}
                      <Icon size={14} class="att-icon-corner" />
                    {:else}
                      <Icon size={18} />
                    {/if}
                  </div>
                  <div class="att-meta">
                    <div class="att-filename" use:tooltip={att.filename}>{att.filename}</div>
                    <div class="att-sub">
                      {#if att.size != null}<span>{formatBytes(att.size)}</span>{/if}
                      {#if att.mimeType}
                        {#if att.size != null}<span class="att-sub-dot">·</span>{/if}
                        <span class="att-mime">{att.mimeType}</span>
                      {/if}
                    </div>
                  </div>
                  <div class="att-action">
                    {#if state === 'downloading'}
                      <Loader size={13} class="spin" />
                    {:else if state === 'done'}
                      <span class="att-action-done">✓</span>
                    {:else}
                      <Download size={13} />
                    {/if}
                  </div>
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Linked Commits (lazy-loaded, pinned to source tab by default) -->
        {#if showLinkedCommitsSection}
          <div class="commits-section">
            <div class="commits-header">
              <GitCommit size={13} />
              Linked Commits
              {#if linkedCommitsLoading}
                <Loader size={11} class="spin commits-spin" />
              {:else if !sourceMissing}
                <span class="commits-count">{linkedCommits.length}</span>
              {/if}

              <!-- Pin indicator on the right side of the header.
                   Three states: pinned to a still-open source, unpinned
                   (using current repo with a "repin" affordance if the
                   source is back), source missing (no toggle here — the
                   call-to-action lives in the explanation card below). -->
              {#if !sourceMissing}
                <span class="commits-pin-info">
                  {#if usingCurrentRepo}
                    <span class="commits-pin-label" use:tooltip={'Looking up against the currently active repo'}>
                      <PinOff size={10} /> Current repo
                    </span>
                    {#if sourceTab}
                      <button
                        class="commits-pin-btn"
                        onclick={() => (usingCurrentRepo = false)}
                        use:tooltip={`Repin to ${sourceTab.name}`}
                      >
                        <Pin size={10} /> Repin
                      </button>
                    {/if}
                  {:else if sourceTab}
                    <span class="commits-pin-label" use:tooltip={`Pinned to ${sourceTab.name}`}>
                      <Pin size={10} /> {sourceTab.name}
                    </span>
                  {/if}
                </span>
              {/if}
            </div>

            {#if sourceMissing}
              <div class="commits-missing">
                <AlertCircle size={13} class="commits-missing-icon" />
                <div class="commits-missing-body">
                  <div class="commits-missing-title">Source repo not open</div>
                  <div class="commits-missing-hint">
                    This ticket was opened from a tab that's no longer in this session,
                    so we can't search its history for linked commits.
                  </div>
                  {#if tabsStore.activeTab}
                    <button
                      class="commits-missing-action"
                      onclick={() => (usingCurrentRepo = true)}
                    >
                      <PinOff size={11} />
                      Use current repo ({tabsStore.activeTab.name})
                    </button>
                  {/if}
                </div>
              </div>
            {:else if !linkedCommitsLoading}
              {#each linkedCommits as commit (commit.sha)}
                {@const refs = getCommitRefs(commit.sha)}
                <button class="linked-commit" onclick={() => goToCommit(commit.sha)}>
                  <div class="lc-top">
                    <code class="lc-sha">{commit.short_oid}</code>
                    {#if commit.source !== 'manual'}
                      <span class="lc-badge">auto</span>
                    {/if}
                    {#if refs.length > 0}
                      {#each refs.slice(0, 2) as ref}
                        <span class="lc-ref" class:lc-ref-current={ref.isCurrent}
                              class:lc-ref-tag={ref.type === 'tag'}
                              class:lc-ref-remote={ref.type === 'remote_branch'}>
                          {ref.name}
                        </span>
                      {/each}
                      {#if refs.length > 2}
                        <span class="lc-ref lc-ref-more">+{refs.length - 2}</span>
                      {/if}
                    {/if}
                  </div>
                  <div class="lc-summary">{commit.summary}</div>
                  <div class="lc-meta">
                    <span class="lc-author">{commit.author_name}</span>
                    <span class="lc-dot">·</span>
                    <span class="lc-time">{relativeTime(commit.timestamp)}</span>
                  </div>
                </button>
              {/each}
            {/if}
          </div>
        {/if}

        <!-- Comments -->
        <div class="comments-section">
          <div class="comments-header">
            Comments
            {#if !issuesStore.detailLoading && liveIssue.commentCount > 0}
              <span class="comments-count">{liveIssue.commentCount}</span>
            {/if}
          </div>

          {#each liveIssue.comments as comment (comment.id)}
            <div class="comment">
              <div class="comment-header">
                {#if comment.user?.avatarUrl}
                  <img class="comment-avatar" src={comment.user.avatarUrl} alt={comment.user.displayName} />
                {:else if comment.user}
                  <span class="comment-avatar-ph">{(comment.user.displayName ?? comment.user.email ?? '?')[0]}</span>
                {/if}
                <span class="comment-author">{comment.user?.displayName ?? 'Unknown'}</span>
                <span class="comment-time">{timeAgo(comment.createdAt)}</span>
                <span class="comment-spacer"></span>
                <span class="comment-copy">
                  <CopyButton
                    value={() => bodyAsPlainText(comment.body, comment.bodyFormat)}
                    variant="icon"
                    title="Copy comment"
                    toastSuccess="Comment copied"
                  />
                </span>
              </div>
              <div class="comment-body" class:rich-html={comment.bodyFormat === 'html'}>
                <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                {@html comment.bodyFormat === 'html'
                  ? comment.body
                  : renderDescription(comment.body)}
              </div>
            </div>
          {/each}

          <!-- Add comment -->
          <div class="comment-composer" class:is-dirty={dirty}>
            {#if me?.avatarUrl}
              <img class="composer-avatar" src={me.avatarUrl} alt={me.displayName} />
            {:else}
              <span class="composer-avatar-ph">
                {(me?.displayName ?? me?.email ?? '?')[0]?.toUpperCase()}
              </span>
            {/if}

            <div class="composer-body">
              <textarea
                class="composer-input"
                placeholder="Write a comment…"
                bind:value={commentBody}
                rows={dirty ? 4 : 2}
                onkeydown={(e) => {
                  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    sendComment();
                  }
                }}
              ></textarea>

              <div class="composer-footer">
                <span class="composer-hint">
                  <kbd class="kbd">Ctrl</kbd>
                  <span class="kbd-plus">+</span>
                  <kbd class="kbd"><CornerDownLeft size={10} /></kbd>
                  <span class="hint-text">to submit</span>
                </span>

                <div class="composer-actions">
                  {#if dirty}
                    <button
                      class="composer-cancel"
                      onclick={() => (commentBody = '')}
                      disabled={commentSending}
                      use:tooltip={'Discard'}
                    >
                      Cancel
                    </button>
                  {/if}
                  <button
                    class="composer-send"
                    onclick={sendComment}
                    disabled={commentSending || !dirty}
                  >
                    {#if commentSending}
                      <Loader size={12} class="spin" />
                      Sending…
                    {:else}
                      <Send size={12} />
                      Comment
                    {/if}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </main>
  </div>
</Modal>

{#if pickerForAtt}
  <FilePickerModal
    mode="save"
    title={`Save attachment — ${pickerForAtt.filename}`}
    initialFilename={pickerForAtt.filename}
    extensions={suggestedExtensions(pickerForAtt.filename, pickerForAtt.mimeType)}
    onConfirm={performDownload}
    onCancel={() => (pickerForAtt = null)}
  />
{/if}

<style>
  /* ── Header content ─────────────────────────────────────────────────────── */
  .modal-identifier { font-family: var(--font-code); font-size: 12px; color: var(--text-muted); user-select: text; }
  .modal-identifier-btn {
    background: transparent;
    border: 1px solid transparent;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .modal-identifier-btn:hover {
    background: var(--bg-hover);
    color: var(--text-secondary);
    border-color: var(--border-subtle);
  }
  .modal-team {
    font-size: 10px; color: var(--text-muted);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    padding: 1px 6px; border-radius: var(--radius-sm);
  }
  .hdr-btn {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; border: none; background: transparent;
    color: var(--text-muted); border-radius: var(--radius-sm); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .hdr-btn:hover { background: var(--bg-hover); color: var(--text-secondary); }

  /* ── Body layout ─────────────────────────────────────────────────────────── */
  .idm-body {
    display: flex;
    height: 100%;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
    font-family: var(--font-ui-sans);
  }

  /* ── Left sidebar ────────────────────────────────────────────────────────── */
  .modal-sidebar {
    width: 220px; flex-shrink: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow-y: auto; padding: 12px 12px 16px;
    display: flex; flex-direction: column; gap: 12px;
  }
  .meta-section { display: flex; flex-direction: column; gap: 4px; }
  .meta-label {
    display: flex; align-items: center; gap: 4px;
    font-size: 10px; font-weight: 600; letter-spacing: 0.04em;
    text-transform: uppercase; color: var(--text-muted);
  }
  .meta-value {
    font-size: 12px; color: var(--text-primary);
    display: flex; align-items: center; gap: 5px;
  }
  .meta-empty { color: var(--text-muted); font-style: italic; }
  .meta-small { font-size: 11px; color: var(--text-muted); }

  /* Status dropdown */
  .status-dropdown-wrap { position: relative; }
  .status-btn {
    display: flex; align-items: center; gap: 5px;
    padding: 4px 8px; font-size: 11px; font-weight: 500;
    background: var(--bg-overlay); border: 1px solid var(--border);
    border-radius: var(--radius-md); cursor: pointer;
    font-family: var(--font-ui-sans); color: var(--text-primary);
    transition: background var(--transition-fast);
    width: 100%;
  }
  .status-btn:hover { background: var(--bg-hover); }
  .status-btn:disabled { opacity: 0.6; cursor: default; }
  .st-done     { color: var(--success); }
  .st-progress { color: var(--accent); }
  .st-cancelled { color: var(--text-muted); }
  .status-dot-sm { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .drop-backdrop { position: fixed; inset: 0; z-index: 599; background: transparent; border: none; padding: 0; cursor: default; }
  .status-drop {
    position: absolute; top: calc(100% + 4px); left: 0; right: 0; z-index: 600;
    background: var(--bg-overlay); border: 1px solid var(--border);
    border-radius: var(--radius-md); padding: 4px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    max-height: 220px; overflow-y: auto;
  }
  .drop-group {
    padding: 5px 8px 2px;
    font-size: 9px; font-weight: 600; letter-spacing: 0.5px;
    text-transform: uppercase; color: var(--text-muted);
  }
  .drop-item {
    display: flex; align-items: center; gap: 6px;
    width: 100%; padding: 5px 8px; text-align: left;
    font-size: 11px; font-family: var(--font-ui-sans);
    color: var(--text-primary); background: transparent; border: none;
    border-radius: var(--radius-sm); cursor: pointer;
    transition: background var(--transition-fast);
  }
  .drop-item:hover { background: var(--bg-hover); }
  .drop-item-active { color: var(--accent); }

  .priority-icon { font-size: 13px; }

  .user-row { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .user-avatar { width: 18px; height: 18px; border-radius: 50%; object-fit: cover; }
  .user-avatar-ph {
    width: 18px; height: 18px; border-radius: 50%;
    background: var(--accent-subtle); color: var(--accent);
    font-size: 9px; font-weight: 700;
    display: flex; align-items: center; justify-content: center;
  }

  .meta-labels { display: flex; flex-wrap: wrap; gap: 3px; }
  .meta-label-chip { font-size: 10px; font-weight: 500; padding: 2px 6px; border-radius: var(--radius-sm); }

  .project-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

  .sidebar-actions { display: flex; flex-direction: column; gap: 5px; margin-top: 4px; }
  .action-btn {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 8px; font-size: 11px; font-weight: 500;
    background: var(--bg-overlay); border: 1px solid var(--border);
    border-radius: var(--radius-md); color: var(--text-secondary);
    cursor: pointer; font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast);
    width: 100%; text-align: left;
  }
  .action-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* ── Main content ────────────────────────────────────────────────────────── */
  .modal-main {
    flex: 1; min-width: 0; overflow-y: auto; padding: 20px 22px 24px;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }

  .issue-title-row {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0 0 16px;
  }
  .issue-title {
    font-size: 16px; font-weight: 600; color: var(--text-primary);
    line-height: 1.4; margin: 0;
    flex: 1; min-width: 0;
    user-select: text;
  }
  .issue-title-copy {
    opacity: 0;
    transition: opacity var(--transition-fast);
    flex-shrink: 0;
  }
  .issue-title-row:hover .issue-title-copy { opacity: 1; }

  .issue-description {
    position: relative;
    font-size: 12.5px; line-height: 1.75; color: var(--text-secondary);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 14px 18px;
    margin-bottom: 20px;
    user-select: text;
  }
  .issue-description :global(*) { user-select: text; }
  .copy-overlay {
    position: absolute;
    top: 6px; right: 6px;
    opacity: 0;
    transition: opacity var(--transition-fast);
    z-index: 2;
  }
  /* Inside the overlay, suppress the inherited "user-select: text" from
     the rule above — otherwise the button mousedown starts a selection
     instead of firing its click. */
  .copy-overlay,
  .copy-overlay :global(*) { user-select: none; }
  .issue-description:hover .copy-overlay,
  .issue-description:focus-within .copy-overlay { opacity: 1; }
  .issue-description::before {
    content: ''; position: absolute; left: 0; top: 8px; bottom: 8px;
    width: 2px; border-radius: 2px;
    background: var(--accent-subtle);
  }
  /* Prism code blocks */
  :global(.issue-description .md-pre),
  :global(.comment-body .md-pre) {
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); padding: 10px 12px; margin: 8px 0;
    overflow-x: auto; font-family: var(--font-code); font-size: 11px;
    line-height: 1.6;
  }
  :global(.issue-description .md-code),
  :global(.comment-body .md-code) {
    font-family: var(--font-code); font-size: 11px; background: none; padding: 0;
  }
  :global(.issue-description .md-inline-code),
  :global(.comment-body .md-inline-code) {
    font-family: var(--font-code); font-size: 11px;
    background: var(--bg-overlay); padding: 1px 5px; border-radius: var(--radius-sm);
    color: var(--accent); border: 1px solid var(--border-subtle);
  }
  :global(.issue-description strong),
  :global(.comment-body strong) { color: var(--text-primary); font-weight: 600; }
  :global(.issue-description em),
  :global(.comment-body em) { color: var(--text-secondary); }
  :global(.issue-description .md-h1) { font-size: 15px; font-weight: 700; color: var(--text-primary); margin: 12px 0 6px; display: block; }
  :global(.issue-description .md-h2) { font-size: 13px; font-weight: 600; color: var(--text-primary); margin: 10px 0 4px; display: block; border-bottom: 1px solid var(--border-subtle); padding-bottom: 3px; }
  :global(.issue-description .md-h3) { font-size: 12px; font-weight: 600; color: var(--text-secondary); margin: 8px 0 4px; display: block; }
  :global(.issue-description .md-p),
  :global(.comment-body .md-p) { margin: 2px 0; }
  :global(.issue-description .md-spacer),
  :global(.comment-body .md-spacer) { height: 6px; }
  :global(.issue-description .md-bq),
  :global(.comment-body .md-bq) {
    border-left: 3px solid var(--accent-subtle); padding: 4px 10px;
    color: var(--text-muted); font-style: italic; margin: 6px 0;
    background: var(--bg-overlay); border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  }
  :global(.issue-description .md-ul),
  :global(.issue-description .md-ol),
  :global(.comment-body .md-ul),
  :global(.comment-body .md-ol) { padding-left: 18px; margin: 4px 0; }
  :global(.issue-description .md-ul li),
  :global(.issue-description .md-ol li),
  :global(.comment-body .md-ul li),
  :global(.comment-body .md-ol li) { margin: 2px 0; }
  :global(.issue-description .md-hr),
  :global(.comment-body .md-hr) { border: none; border-top: 1px solid var(--border-subtle); margin: 10px 0; }
  :global(.issue-description .md-link),
  :global(.comment-body .md-link) { color: var(--accent); cursor: pointer; }

  /* ── Raw-HTML passthrough (GitHub-style bodies, Jira HTML excerpts) ──── */
  :global(.issue-description p),
  :global(.comment-body p) { margin: 2px 0; }
  :global(.issue-description h1),
  :global(.comment-body h1) { font-size: 15px; font-weight: 700; color: var(--text-primary); margin: 12px 0 6px; }
  :global(.issue-description h2),
  :global(.comment-body h2) { font-size: 13px; font-weight: 600; color: var(--text-primary); margin: 10px 0 4px; border-bottom: 1px solid var(--border-subtle); padding-bottom: 3px; }
  :global(.issue-description h3),
  :global(.comment-body h3) { font-size: 12px; font-weight: 600; color: var(--text-secondary); margin: 8px 0 4px; }
  :global(.issue-description h4), :global(.issue-description h5), :global(.issue-description h6),
  :global(.comment-body h4), :global(.comment-body h5), :global(.comment-body h6)
    { font-size: 12px; font-weight: 600; color: var(--text-secondary); margin: 6px 0 3px; }
  :global(.issue-description:not(.rich-html) ul:not(.md-ul)),
  :global(.issue-description:not(.rich-html) ol:not(.md-ol)),
  :global(.comment-body:not(.rich-html) ul:not(.md-ul)),
  :global(.comment-body:not(.rich-html) ol:not(.md-ol)) { padding-left: 22px; margin: 6px 0; }
  :global(.issue-description:not(.rich-html) ul:not(.md-ul) li),
  :global(.issue-description:not(.rich-html) ol:not(.md-ol) li),
  :global(.comment-body:not(.rich-html) ul:not(.md-ul) li),
  :global(.comment-body:not(.rich-html) ol:not(.md-ol) li) { margin: 3px 0; padding-left: 2px; }
  :global(.issue-description blockquote:not(.md-bq)),
  :global(.comment-body blockquote:not(.md-bq)) {
    border-left: 3px solid var(--accent-subtle); padding: 4px 12px;
    margin: 6px 0 6px 4px; background: var(--bg-overlay);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0; color: var(--text-secondary);
  }
  :global(.issue-description code),
  :global(.comment-body code) {
    font-family: var(--font-code); font-size: 11px;
    background: var(--bg-overlay); padding: 1px 5px; border-radius: var(--radius-sm);
    color: var(--accent); border: 1px solid var(--border-subtle);
  }
  :global(.issue-description pre code),
  :global(.comment-body pre code) { background: none; border: none; padding: 0; color: inherit; }

  :global(.issue-description details),
  :global(.comment-body details) {
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    margin: 8px 0; background: var(--bg-overlay); overflow: hidden;
  }
  :global(.issue-description details > summary),
  :global(.comment-body details > summary) {
    cursor: pointer; padding: 6px 10px;
    font-weight: 600; color: var(--text-primary);
    list-style: none; display: flex; align-items: center; gap: 6px;
    user-select: none;
  }
  :global(.issue-description details > summary::-webkit-details-marker),
  :global(.comment-body details > summary::-webkit-details-marker) { display: none; }
  :global(.issue-description details > summary::before),
  :global(.comment-body details > summary::before) {
    content: '▶'; font-size: 9px; color: var(--text-muted);
    transition: transform 120ms ease; display: inline-block;
  }
  :global(.issue-description details[open] > summary::before),
  :global(.comment-body details[open] > summary::before) { transform: rotate(90deg); }
  :global(.issue-description details[open] > summary),
  :global(.comment-body details[open] > summary) {
    border-bottom: 1px solid var(--border-subtle);
    background: color-mix(in srgb, var(--bg-elevated) 50%, transparent);
  }
  :global(.issue-description details > *:not(summary)),
  :global(.comment-body details > *:not(summary)) { padding: 0 12px; }
  :global(.issue-description details > *:not(summary):first-of-type),
  :global(.comment-body details > *:not(summary):first-of-type) { padding-top: 8px; }
  :global(.issue-description details > *:not(summary):last-child),
  :global(.comment-body details > *:not(summary):last-child) { padding-bottom: 8px; }
  :global(.issue-description details details),
  :global(.comment-body details details) { margin: 6px 0; }

  /* ── Rich HTML (Jira renderedFields) ─────────────────────────────────────
     Bodies arriving pre-rendered from Jira carry the `.rich-html` class. */
  :global(.rich-html) {
    color: var(--text-primary);
    font-size: 12.5px;
    line-height: 1.65;
  }
  :global(.rich-html) > :global(*:first-child) { margin-top: 0 !important; }
  :global(.rich-html) > :global(*:last-child)  { margin-bottom: 0 !important; }
  :global(.rich-html p) { margin: 0 0 8px; }

  /* Headings — clean hierarchy, no border noise */
  :global(.rich-html h1),
  :global(.rich-html h2),
  :global(.rich-html h3),
  :global(.rich-html h4),
  :global(.rich-html h5),
  :global(.rich-html h6) {
    color: var(--text-primary);
    font-weight: 600;
    line-height: 1.3;
    margin: 18px 0 8px;
    letter-spacing: -0.01em;
  }
  :global(.rich-html h1) { font-size: 17px; font-weight: 700; }
  :global(.rich-html h2) { font-size: 15px; }
  :global(.rich-html h3) { font-size: 13.5px; }
  :global(.rich-html h4) { font-size: 12.5px; color: var(--text-primary); }
  :global(.rich-html h5),
  :global(.rich-html h6) { font-size: 12px; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.05em; }

  /* Inline marks */
  :global(.rich-html strong),
  :global(.rich-html b) { color: var(--text-primary); font-weight: 600; }
  :global(.rich-html em),
  :global(.rich-html i) { font-style: italic; }
  :global(.rich-html u)   { text-decoration: underline; text-decoration-color: var(--accent-subtle); text-underline-offset: 2px; }
  :global(.rich-html s),
  :global(.rich-html strike),
  :global(.rich-html del) { text-decoration: line-through; color: var(--text-muted); }
  :global(.rich-html sub) { vertical-align: sub; font-size: 0.78em; }
  :global(.rich-html sup) { vertical-align: super; font-size: 0.78em; }

  /* Links — clean underline on hover only */
  :global(.rich-html a) {
    color: var(--accent);
    text-decoration: none;
    transition: color var(--transition-fast);
  }
  :global(.rich-html a:hover) { text-decoration: underline; text-underline-offset: 2px; }

  /* Lists — custom markers using CSS counters (works on Chromium/WebKit/Firefox)
     so we get accent-colored, monospaced numbers and dot-style bullets that
     don't look like default browser lists. */
  :global(.rich-html ul),
  :global(.rich-html ol) {
    list-style: none;
    padding-left: 0;
    margin: 6px 0 12px;
  }
  :global(.rich-html ol) { counter-reset: ol-counter; }

  :global(.rich-html ul li),
  :global(.rich-html ol li) {
    position: relative;
    padding-left: 22px;
    margin: 4px 0;
  }
  :global(.rich-html li > p) { margin: 0; }
  :global(.rich-html li > p + p) { margin-top: 4px; }

  /* Bullets — small filled circle in accent */
  :global(.rich-html ul > li)::before {
    content: '';
    position: absolute;
    left: 8px; top: 0.7em;
    width: 5px; height: 5px;
    border-radius: 50%;
    background: var(--accent);
    transform: translateY(-50%);
  }
  :global(.rich-html ul ul > li)::before {
    background: transparent;
    border: 1.5px solid var(--accent);
    width: 6px; height: 6px;
  }
  :global(.rich-html ul ul ul > li)::before {
    background: var(--text-muted);
    border: none;
    width: 4px; height: 4px;
    border-radius: 1px;
  }

  /* Numbered — accent monospace number */
  :global(.rich-html ol > li) { counter-increment: ol-counter; padding-left: 28px; }
  :global(.rich-html ol > li)::before {
    content: counter(ol-counter) '.';
    position: absolute;
    left: 0; top: 0;
    min-width: 22px;
    text-align: right;
    padding-right: 4px;
    font-family: var(--font-code);
    font-size: 0.92em;
    font-weight: 600;
    color: var(--accent);
    line-height: 1.65;
  }
  :global(.rich-html ol ol) { counter-reset: ol-counter-2; }
  :global(.rich-html ol ol > li) { counter-increment: ol-counter-2; }
  :global(.rich-html ol ol > li)::before {
    content: counter(ol-counter-2, lower-alpha) '.';
    color: color-mix(in srgb, var(--accent) 75%, var(--text-muted));
  }

  /* Nested list spacing */
  :global(.rich-html li > ul),
  :global(.rich-html li > ol) { margin: 4px 0 4px; }

  /* Task / checkbox lists (Jira renders them with input[type=checkbox]) */
  :global(.rich-html ul.contains-task-list),
  :global(.rich-html ul.task-list) { list-style: none; padding-left: 4px; }
  :global(.rich-html li.task-list-item) { display: flex; gap: 6px; align-items: baseline; }
  :global(.rich-html input[type="checkbox"]) {
    accent-color: var(--accent);
    margin-right: 2px;
  }

  /* Blockquote */
  :global(.rich-html blockquote) {
    border-left: 3px solid var(--accent);
    padding: 8px 14px;
    color: var(--text-primary);
    margin: 12px 0;
    background: color-mix(in srgb, var(--accent) 10%, var(--bg-overlay));
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  }
  :global(.rich-html blockquote p) { margin: 4px 0; color: var(--text-secondary); }
  :global(.rich-html blockquote p:first-child) { margin-top: 0; }
  :global(.rich-html blockquote p:last-child)  { margin-bottom: 0; }

  /* Inline code */
  :global(.rich-html code) {
    font-family: var(--font-code);
    font-size: 0.92em;
    background: color-mix(in srgb, var(--accent) 10%, var(--bg-overlay));
    color: var(--accent);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--accent) 18%, transparent);
  }

  /* Code blocks — slightly raised "card" inside the description container */
  :global(.rich-html pre) {
    position: relative;
    background:
      linear-gradient(180deg,
        color-mix(in srgb, var(--accent) 4%, var(--bg-overlay)) 0%,
        var(--bg-overlay) 100%);
    border: 1px solid color-mix(in srgb, var(--accent) 14%, var(--border));
    border-radius: var(--radius-md);
    padding: 12px 14px;
    margin: 12px 0;
    overflow-x: auto;
    font-family: var(--font-code);
    font-size: 11.5px;
    line-height: 1.6;
    color: var(--text-primary);
    box-shadow: inset 0 1px 0 color-mix(in srgb, white 4%, transparent);
  }
  :global(.rich-html pre code) {
    background: none;
    border: none;
    padding: 0;
    color: inherit;
    font-size: inherit;
    border-radius: 0;
  }

  /* Horizontal rule */
  :global(.rich-html hr) {
    border: none;
    border-top: 1px solid var(--border-subtle);
    margin: 14px 0;
  }

  /* Tables */
  :global(.rich-html table) {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    margin: 12px 0;
    font-size: 11.5px;
    border: 1px solid color-mix(in srgb, var(--accent) 14%, var(--border));
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  :global(.rich-html th),
  :global(.rich-html td) {
    border-bottom: 1px solid var(--border-subtle);
    border-right:  1px solid var(--border-subtle);
    padding: 7px 11px;
    text-align: left;
    vertical-align: top;
  }
  :global(.rich-html th:last-child),
  :global(.rich-html td:last-child) { border-right: none; }
  :global(.rich-html tr:last-child td) { border-bottom: none; }
  :global(.rich-html th) {
    background: color-mix(in srgb, var(--accent) 8%, var(--bg-overlay));
    color: var(--text-primary);
    font-weight: 600;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  :global(.rich-html tbody tr:hover td) {
    background: color-mix(in srgb, var(--accent) 4%, transparent);
  }
  :global(.rich-html tr:nth-child(even) td) {
    background: color-mix(in srgb, var(--bg-overlay) 50%, transparent);
  }

  /* Images */
  :global(.rich-html img) {
    max-width: 100%;
    height: auto;
    border-radius: var(--radius-sm);
    margin: 6px 0;
  }

  /* Jira "panel" macros (info/warning/note/error/success) — Server/DC only. */
  :global(.rich-html div.panel),
  :global(.rich-html div.aui-message) {
    border-left: 3px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 6%, var(--bg-overlay));
    padding: 10px 14px;
    margin: 10px 0;
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  }
  :global(.rich-html div.panel.warningPanel),
  :global(.rich-html div.aui-message.warning) {
    border-left-color: var(--warning);
    background: color-mix(in srgb, var(--warning) 8%, var(--bg-overlay));
  }
  :global(.rich-html div.panel.errorPanel),
  :global(.rich-html div.aui-message.error) {
    border-left-color: var(--error);
    background: color-mix(in srgb, var(--error) 8%, var(--bg-overlay));
  }
  :global(.rich-html div.panel.successPanel),
  :global(.rich-html div.aui-message.success) {
    border-left-color: var(--success);
    background: color-mix(in srgb, var(--success) 8%, var(--bg-overlay));
  }
  :global(.rich-html div.panel.notePanel),
  :global(.rich-html div.aui-message.note) {
    border-left-color: var(--color-tag);
    background: color-mix(in srgb, var(--color-tag) 8%, var(--bg-overlay));
  }
  :global(.rich-html div.panel .panelHeader),
  :global(.rich-html div.panel .panelContent) { display: block; }
  :global(.rich-html div.panel .panelHeader)  { font-weight: 600; margin-bottom: 4px; color: var(--text-primary); }

  /* Jira status lozenge */
  :global(.rich-html span.status-lozenge),
  :global(.rich-html span.aui-lozenge) {
    display: inline-block;
    padding: 1px 7px;
    border-radius: 99px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    vertical-align: 1px;
  }

  /* Mentions / user references */
  :global(.rich-html a.user-hover),
  :global(.rich-html a.confluence-userlink) {
    display: inline-block;
    padding: 0 6px;
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: 99px;
    font-weight: 500;
    text-decoration: none;
  }
  :global(.rich-html a.user-hover:hover),
  :global(.rich-html a.confluence-userlink:hover) {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    text-decoration: none;
  }

  /* Issue-key links (auto-linked by Jira: e.g. "ARB-123") */
  :global(.rich-html a.issue-link),
  :global(.rich-html a.jira-issue-macro) {
    font-family: var(--font-code);
    font-size: 0.92em;
    background: var(--accent-subtle);
    color: var(--accent);
    padding: 0 5px;
    border-radius: var(--radius-sm);
    text-decoration: none;
  }

  /* Comment body — when rendering rich HTML, give it a slightly raised card
     with breathing room so internal panels/code blocks have room to live. */
  .comment-body.rich-html {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    padding: 10px 14px;
  }
  .issue-no-desc {
    font-size: 12px; color: var(--text-muted); font-style: italic;
    padding: 12px 16px; border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); background: var(--bg-elevated);
    margin-bottom: 20px;
  }
  /* Description loading skeleton */
  .issue-desc-skeleton {
    display: flex; flex-direction: column; gap: 8px;
    padding: 14px 16px; margin-bottom: 20px;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .sk-line {
    height: 10px; border-radius: var(--radius-sm);
    background: linear-gradient(90deg, var(--bg-overlay) 25%, var(--bg-hover) 50%, var(--bg-overlay) 75%);
    background-size: 200% 100%;
    animation: sk-shimmer 1.4s infinite;
  }
  @keyframes sk-shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
  .drop-empty {
    display: flex; align-items: center; gap: 6px; justify-content: center;
    padding: 10px 8px; font-size: 11px; color: var(--text-muted); font-style: italic;
  }

  /* ── Comments ────────────────────────────────────────────────────────────── */
  .comments-section { display: flex; flex-direction: column; gap: 12px; }
  .comments-header {
    display: flex; align-items: center; gap: 6px;
    font-size: 12px; font-weight: 600; color: var(--text-secondary);
  }
  .comments-count {
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    font-size: 10px; color: var(--text-muted);
    padding: 0 5px; border-radius: 99px;
  }
  .comment { display: flex; flex-direction: column; gap: 5px; }
  .comment-header {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; color: var(--text-muted);
  }
  .comment-avatar { width: 18px; height: 18px; border-radius: 50%; object-fit: cover; }
  .comment-avatar-ph {
    width: 18px; height: 18px; border-radius: 50%;
    background: var(--accent-subtle); color: var(--accent);
    font-size: 9px; font-weight: 700;
    display: flex; align-items: center; justify-content: center;
  }
  .comment-author { font-weight: 500; color: var(--text-secondary); }
  .comment-time { color: var(--text-muted); font-size: 10px; }
  .comment-body {
    font-size: 12px; line-height: 1.6; color: var(--text-secondary);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); padding: 8px 10px;
    user-select: text;
  }
  .comment-body :global(*) { user-select: text; }
  .comment-spacer { flex: 1; }
  .comment-copy   { opacity: 0; transition: opacity var(--transition-fast); }
  .comment-copy,
  .comment-copy :global(*) { user-select: none; }
  .comment:hover .comment-copy,
  .comment:focus-within .comment-copy { opacity: 1; }

  /* ── Comment composer ─────────────────────────────────────────────────── */
  .comment-composer {
    display: flex; align-items: flex-start; gap: 10px;
    margin-top: 4px;
  }
  .composer-avatar,
  .composer-avatar-ph {
    width: 28px; height: 28px; border-radius: 50%; flex-shrink: 0;
    margin-top: 2px;
  }
  .composer-avatar { object-fit: cover; }
  .composer-avatar-ph {
    display: flex; align-items: center; justify-content: center;
    background: var(--accent-subtle); color: var(--accent);
    font-size: 11px; font-weight: 700;
    border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
  }

  .composer-body {
    flex: 1; min-width: 0;
    display: flex; flex-direction: column;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    transition: border-color var(--transition-fast),
                box-shadow    var(--transition-fast),
                background    var(--transition-fast);
  }
  .composer-body:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
    background: var(--bg-base);
  }
  .comment-composer.is-dirty .composer-body {
    background: var(--bg-base);
  }

  .composer-input {
    width: 100%;
    border: none;
    background: transparent;
    resize: none;
    padding: 10px 12px 6px;
    font-size: 12.5px;
    font-family: var(--font-ui-sans);
    color: var(--text-primary);
    line-height: 1.55;
    outline: none;
    transition: padding var(--transition-fast);
  }
  .composer-input::placeholder { color: var(--text-muted); font-style: italic; }

  .composer-footer {
    display: flex; align-items: center; justify-content: space-between;
    padding: 6px 8px 6px 12px;
    border-top: 1px solid var(--border-subtle);
    background: color-mix(in srgb, var(--bg-overlay) 60%, transparent);
  }

  .composer-hint {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: 10px; color: var(--text-muted);
    user-select: none;
  }
  .kbd {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 18px; height: 18px; padding: 0 4px;
    font-family: var(--font-code);
    font-size: 9.5px; font-weight: 600;
    color: var(--text-secondary);
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
  }
  .kbd-plus  { color: var(--text-disabled); font-size: 10px; margin: 0 1px; }
  .hint-text { margin-left: 4px; }

  .composer-actions { display: flex; align-items: center; gap: 6px; }
  .composer-cancel {
    padding: 5px 10px;
    font-size: 11px; font-weight: 500;
    color: var(--text-secondary);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .composer-cancel:hover  { background: var(--bg-hover); color: var(--text-primary); }
  .composer-cancel:disabled { opacity: 0.5; cursor: default; }

  .composer-send {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 5px 12px;
    font-size: 11px; font-weight: 600;
    background: var(--accent);
    color: var(--bg-base);
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    box-shadow: 0 1px 0 color-mix(in srgb, black 20%, transparent),
                inset 0 1px 0 color-mix(in srgb, white 18%, transparent);
    transition: background var(--transition-fast), transform 80ms ease;
  }
  .composer-send:hover:not(:disabled) {
    background: var(--accent-hover);
    transform: translateY(-1px);
  }
  .composer-send:active:not(:disabled) { transform: translateY(0); }
  .composer-send:disabled {
    opacity: 0.5; cursor: default;
    background: var(--bg-hover);
    color: var(--text-muted);
    box-shadow: none;
  }


  /* ── Attachments ─────────────────────────────────────────────────────────── */
  .attachments-section {
    display: flex; flex-direction: column; gap: 6px;
    margin-bottom: 20px;
  }
  .attachments-header {
    display: flex; align-items: center; gap: 6px;
    font-size: 12px; font-weight: 600; color: var(--text-secondary);
    margin-bottom: 2px;
  }
  .attachments-count {
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    font-size: 10px; color: var(--text-muted);
    padding: 0 5px; border-radius: 99px;
  }

  .attachments-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 6px;
  }

  .attachment {
    display: grid;
    grid-template-columns: 32px 1fr auto;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    text-align: left;
    font-family: var(--font-ui-sans);
    color: var(--text-primary);
    transition:
      background var(--transition-fast),
      border-color var(--transition-fast),
      transform 80ms ease,
      box-shadow var(--transition-fast);
    width: 100%;
    overflow: hidden;
  }
  .attachment:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 6%, var(--bg-elevated));
    border-color: color-mix(in srgb, var(--accent) 30%, var(--border));
    transform: translateY(-1px);
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.18);
  }
  .attachment:active:not(:disabled) { transform: translateY(0); }
  .attachment:disabled { cursor: default; opacity: 0.85; }

  .att-icon-wrap {
    width: 32px; height: 32px;
    display: flex; align-items: center; justify-content: center;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--accent);
    flex-shrink: 0;
  }
  .att-icon-wrap.att-icon-image {
    background: color-mix(in srgb, var(--accent) 12%, var(--bg-overlay));
  }
  :global(.att-icon-corner) { color: var(--accent); }

  .att-meta { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .att-filename {
    font-size: 12px; font-weight: 500; color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .att-sub {
    display: flex; align-items: center; gap: 5px;
    font-size: 10px; color: var(--text-muted);
    white-space: nowrap; overflow: hidden;
  }
  .att-sub-dot { color: var(--text-disabled); }
  .att-mime    { font-family: var(--font-code); font-size: 9.5px; opacity: 0.85;
                 overflow: hidden; text-overflow: ellipsis; }

  .att-action {
    display: flex; align-items: center; justify-content: center;
    width: 26px; height: 26px;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .attachment:hover:not(:disabled) .att-action {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }
  .attachment.att-downloading .att-action { color: var(--accent); }
  .attachment.att-done        .att-action { color: var(--success); }
  .att-action-done            { font-size: 14px; font-weight: 700; line-height: 1; }
  .attachment.att-error {
    border-color: color-mix(in srgb, var(--error) 40%, var(--border));
    background: color-mix(in srgb, var(--error) 6%, var(--bg-elevated));
  }
  .attachment.att-error .att-action { color: var(--error); }

  /* ── Linked Commits ──────────────────────────────────────────────────────── */
  .commits-section {
    display: flex; flex-direction: column; gap: 5px;
    margin-bottom: 20px;
  }
  .commits-header {
    display: flex; align-items: center; gap: 6px;
    font-size: 12px; font-weight: 600; color: var(--text-secondary);
    margin-bottom: 2px;
  }
  .commits-count {
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    font-size: 10px; color: var(--text-muted);
    padding: 0 5px; border-radius: 99px;
  }
  :global(.commits-spin) { color: var(--text-muted); }

  /* ── Pin indicator (Linked Commits) ──────────────────────────────────────── */
  .commits-pin-info {
    margin-left: auto;
    display: inline-flex; align-items: center; gap: 6px;
  }
  .commits-pin-label {
    display: inline-flex; align-items: center; gap: 3px;
    font-size: 10px; font-weight: 500;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
    max-width: 180px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .commits-pin-btn {
    display: inline-flex; align-items: center; gap: 3px;
    font-size: 10px; font-weight: 500;
    color: var(--text-secondary);
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
    cursor: pointer;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .commits-pin-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }

  /* ── Source-tab-missing card (Linked Commits) ────────────────────────────── */
  .commits-missing {
    display: flex; align-items: flex-start; gap: 10px;
    padding: 10px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-left: 3px solid color-mix(in srgb, var(--warning, #fbbf24) 60%, var(--border));
    border-radius: var(--radius-md);
  }
  :global(.commits-missing-icon) { color: var(--warning, #fbbf24); flex-shrink: 0; margin-top: 1px; }
  .commits-missing-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .commits-missing-title {
    font-size: 12px; font-weight: 600; color: var(--text-primary);
  }
  .commits-missing-hint {
    font-size: 11px; color: var(--text-muted); line-height: 1.5;
  }
  .commits-missing-action {
    align-self: flex-start;
    display: inline-flex; align-items: center; gap: 5px;
    margin-top: 4px;
    padding: 4px 10px;
    font-size: 11px; font-weight: 500;
    background: var(--accent-subtle);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: var(--radius-md);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .commits-missing-action:hover {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }

  .linked-commit {
    display: flex; flex-direction: column; gap: 3px;
    width: 100%; text-align: left; padding: 8px 10px;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); cursor: pointer;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .linked-commit:hover {
    background: var(--bg-overlay);
    border-color: var(--border);
  }
  .lc-top {
    display: flex; align-items: center; gap: 5px; flex-wrap: wrap;
  }
  .lc-sha {
    font-family: var(--font-code); font-size: 10.5px;
    color: var(--text-muted); flex-shrink: 0;
  }
  .lc-badge {
    font-size: 9px; font-weight: 600; letter-spacing: 0.3px;
    padding: 1px 5px; border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    text-transform: uppercase; flex-shrink: 0;
  }
  .lc-ref {
    font-size: 9.5px; font-weight: 500; padding: 1px 5px; border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--text-muted) 10%, transparent);
    color: var(--text-muted);
    border: 1px solid color-mix(in srgb, var(--text-muted) 20%, transparent);
    max-width: 140px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .lc-ref-current {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 32%, transparent);
  }
  .lc-ref-tag {
    background: color-mix(in srgb, var(--color-tag) 14%, transparent);
    color: var(--color-tag);
    border-color: color-mix(in srgb, var(--color-tag) 30%, transparent);
  }
  .lc-ref-remote { font-style: italic; opacity: 0.85; }
  .lc-ref-more   { font-style: italic; }
  .lc-summary {
    font-size: 12px; font-weight: 500; color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .lc-meta {
    display: flex; align-items: center; gap: 4px;
    font-size: 10.5px; color: var(--text-muted);
  }
  .lc-dot { color: var(--text-disabled); }
  .lc-author { color: var(--text-muted); }
  .lc-time { color: var(--text-disabled); }
</style>
