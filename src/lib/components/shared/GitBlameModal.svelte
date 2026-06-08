<script lang="ts">
  import { AlertCircle, User, Calendar } from 'lucide-svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { getFileBlameStreaming } from '$lib/ipc/diff';
  import { highlight } from '$lib/utils/diff-formatter';
  import type { BlameLine, BlameProgress } from '$lib/types/git';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ProgressBar from './ui/ProgressBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    tabId,
    path,
    onClose,
  }: {
    tabId: string;
    path: string;
    onClose: () => void;
  } = $props();

  // ── State ────────────────────────────────────────────────────────────────────

  let lines      = $state<BlameLine[]>([]);
  let loading    = $state(true);
  let error      = $state<string | null>(null);
  let hoveredOid = $state<string | null>(null);
  // Determinate progress streamed from `git blame --incremental` while the
  // history walk runs. `null` until the first tick (or for the libgit2
  // fallback, which never ticks → the bar stays indeterminate).
  let progress   = $state<BlameProgress | null>(null);

  // Virtualization state — without windowing, files of a few thousand lines
  // freeze the modal for seconds (Prism highlight + DOM cost per row × N),
  // and the spinner reads as "loading forever" since the main thread is busy
  // before the first paint of the table lands.
  let scrollEl  = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(640);

  // ── Load blame ────────────────────────────────────────────────────────────────

  // Monotonic request id. Every (re)load bumps it; only the callbacks for the
  // CURRENT generation are allowed to touch state. This is what lets the
  // spinner reliably clear: the latest request ALWAYS owns `loading`, so a
  // superseded fetch resolving late can't flip it — and, crucially, can't
  // leave it stuck either. (The previous tabId/path-equality guard could fail
  // and strand `loading = true` forever, which read as an eternal spinner.)
  let loadGen = 0;
  // Safety net so a genuinely hung backend (a huge repo where libgit2 blame
  // walks very deep history) surfaces as an actionable error instead of an
  // infinite spinner.
  const BLAME_TIMEOUT_MS = 30_000;

  /** Kick off a blame fetch under a fresh generation. Returns the timeout
   *  handle so the effect can cancel it on re-run; `retry()` lets it self-fire.
   *  Single source of truth for the load lifecycle — both the mount effect and
   *  the Retry button go through here. */
  function runLoad(): ReturnType<typeof setTimeout> {
    const myTabId = tabId;
    const myPath  = path;
    const myGen   = ++loadGen;

    loading   = true;
    error     = null;
    lines     = [];
    progress  = null;
    scrollTop = 0;

    const timer = setTimeout(() => {
      if (myGen !== loadGen) return;
      error   = 'Blame timed out — this file may have very deep history. Retry to try again.';
      loading = false;
    }, BLAME_TIMEOUT_MS);

    getFileBlameStreaming(myTabId, myPath, p => {
      // Drop ticks from a superseded generation; the timer-driven progress
      // bump also keeps the timeout from firing mid-walk on a healthy backend.
      if (myGen === loadGen) progress = p;
    })
      .then(r => { if (myGen === loadGen) lines = r; })
      .catch(e => { if (myGen === loadGen) error = String(e); })
      .finally(() => {
        if (myGen !== loadGen) return;
        clearTimeout(timer);
        loading = false;
      });

    return timer;
  }

  $effect(() => {
    // Read the props so the effect re-runs when either changes.
    void tabId; void path;
    const timer = runLoad();
    // Drop the timer if the effect re-runs (new file) before this fetch settles.
    return () => clearTimeout(timer);
  });

  function retry() { runLoad(); }

  // ── Commit color palette ──────────────────────────────────────────────────────

  const PALETTE = [
    '#4d78cc', // blue
    '#e6a817', // amber
    '#4caf73', // green
    '#c75450', // red
    '#9b6dff', // purple
    '#e67e22', // orange
    '#17a2b8', // teal
    '#e91e8c', // pink
    '#7cb342', // lime
    '#00acc1', // cyan
  ];

  function oidToColor(oid: string): string {
    let h = 0;
    for (let i = 0; i < Math.min(8, oid.length); i++) {
      h = (h * 31 + oid.charCodeAt(i)) >>> 0;
    }
    return PALETTE[h % PALETTE.length];
  }

  // ── Formatting ────────────────────────────────────────────────────────────────

  function formatDate(ts: number): string {
    return new Date(ts * 1000).toLocaleDateString(undefined, {
      year: 'numeric', month: 'short', day: 'numeric',
    });
  }

  function formatRelative(ts: number): string {
    const diff = Math.floor((Date.now() - ts * 1000) / 86_400_000);
    if (diff === 0) return 'today';
    if (diff === 1) return 'yesterday';
    if (diff < 7)   return `${diff}d ago`;
    if (diff < 30)  return `${Math.floor(diff / 7)}w ago`;
    if (diff < 365) return `${Math.floor(diff / 30)}mo ago`;
    return `${Math.floor(diff / 365)}y ago`;
  }

  // ── Navigate to commit in graph ───────────────────────────────────────────────

  function navigateToCommit(oid: string) {
    if (!oid || oid.startsWith('0000000')) return;
    graphStore.scrollToCommit(oid);
    onClose();
  }

  const filename = $derived(path.split('/').pop() ?? path);
  const dirpart  = $derived(path !== filename ? path.slice(0, path.lastIndexOf('/')) : '');

  const progressPct = $derived(
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.done / progress.total) * 100))
      : 0,
  );

  // Row height matches `.blame-row { min-height: 21px }` + 1px of margin on
  // group-start rows (counted in the spacer math so the scroll position never
  // drifts past the actual content). With `white-space: pre` the row height
  // is uniform regardless of line length.
  const ROW_HEIGHT = 22;
  const ROW_BUFFER = 25;

  const visStart = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - ROW_BUFFER));
  const visEnd   = $derived(Math.min(
    lines.length,
    Math.ceil((scrollTop + viewportH) / ROW_HEIGHT) + ROW_BUFFER,
  ));
  const visibleLines = $derived(lines.slice(visStart, visEnd));
  const topSpacer    = $derived(visStart * ROW_HEIGHT);
  const bottomSpacer = $derived(Math.max(0, (lines.length - visEnd) * ROW_HEIGHT));

  function onScroll() {
    if (!scrollEl) return;
    scrollTop = scrollEl.scrollTop;
    viewportH = scrollEl.clientHeight;
  }

  // Initial viewport measure once the scroll container mounts. Without this
  // the first `visEnd` is computed against the fallback `viewportH = 640` and
  // tall modals would briefly under-render before the first scroll event.
  $effect(() => {
    if (scrollEl) {
      viewportH = scrollEl.clientHeight;
    }
  });
</script>

<Modal {onClose} width="min(96vw, 1280px)" height="88vh" padBody={false} ariaLabel="Git Blame — {path}">
  {#snippet header()}
    <ModalHeader {onClose}>
      <span class="header-label">Git Blame</span>
      <span class="header-sep">—</span>
      <span class="header-path" use:tooltip={path}>{filename}</span>
      {#if dirpart}
        <span class="header-dir" use:tooltip={path}>{dirpart}</span>
      {/if}
    </ModalHeader>
  {/snippet}

  <div class="blame-scroll" bind:this={scrollEl} onscroll={onScroll}>
    {#if loading}
      <div class="state-overlay loading-blame">
        <span class="loading-title">Walking history…</span>
        <ProgressBar
          indeterminate={!progress || progress.total === 0}
          value={progress?.done ?? 0}
          max={progress?.total || 1}
          height={5}
          ariaLabel="Blame progress"
        />
        {#if progress && progress.total > 0}
          <span class="loading-meta">
            {progress.done.toLocaleString()} / {progress.total.toLocaleString()} lines ({progressPct}%)
          </span>
          {#if progress.currentShort}
            <span class="loading-commit" use:tooltip={progress.currentSummary ?? ''}>
              {progress.currentShort}{#if progress.currentDate} · {formatDate(progress.currentDate)}{/if}
            </span>
          {/if}
        {/if}
      </div>

    {:else if error}
      <div class="state-overlay err">
        <AlertCircle size={18} />
        <span>{error}</span>
        <button class="blame-retry" onclick={retry}>Retry</button>
      </div>

    {:else if lines.length === 0}
      <div class="state-overlay muted">
        <span>No blame data available</span>
      </div>

    {:else}
      <div class="blame-table">
        <div class="blame-spacer" style="height: {topSpacer}px"></div>
        {#each visibleLines as line (line.line_no)}
          {@const color = oidToColor(line.commit_oid)}
          {@const isHovered = hoveredOid === line.commit_oid}
          <div
            class="blame-row"
            class:group-start={line.is_group_start}
            class:hovered={isHovered}
            role="row"
            tabindex="-1"
            onmouseenter={() => hoveredOid = line.commit_oid}
            onmouseleave={() => hoveredOid = null}
          >
            <!-- Line number -->
            <span class="line-no">{line.line_no}</span>

            <!-- Blame gutter -->
            <div class="gutter" style="--commit-color: {color}">
              <div class="gutter-bar"></div>
              {#if line.is_group_start}
                <button
                  class="gutter-oid"
                  style="color: {color}"
                  onclick={() => navigateToCommit(line.commit_oid)}
                  use:tooltip={{ content: `Go to commit ${line.commit_oid}`, description: line.summary }}
                >{line.short_oid}</button>
                <span class="gutter-author" use:tooltip={{ content: line.author_name, description: line.author_email }}>
                  <User size={9} />
                  {line.author_name}
                </span>
                <span class="gutter-date" use:tooltip={formatDate(line.timestamp)}>
                  <Calendar size={9} />
                  {formatRelative(line.timestamp)}
                </span>
              {:else}
                <span class="gutter-continuation"></span>
              {/if}
            </div>

            <!-- Line content with Prism syntax highlighting -->
            <code class="line-content">{@html highlight(line.content, path)}</code>
          </div>
        {/each}
        <div class="blame-spacer" style="height: {bottomSpacer}px"></div>
      </div>
    {/if}
  </div>
</Modal>

<style>
  .header-label {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    flex-shrink: 0;
  }

  .header-sep {
    color: var(--text-disabled);
    flex-shrink: 0;
  }

  .header-path {
    font-family: var(--font-code);
    font-size: 13px;
    color: var(--text-primary);
    font-weight: 500;
    flex-shrink: 0;
  }

  .header-dir {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── Body ── */
  .blame-scroll {
    height: 100%;
    overflow: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
    /* Allow text selection everywhere inside */
    user-select: text;
  }
  .blame-scroll::-webkit-scrollbar { width: 6px; height: 6px; }
  .blame-scroll::-webkit-scrollbar-track { background: transparent; }
  .blame-scroll::-webkit-scrollbar-thumb { background: var(--border); border-radius: var(--radius-sm); }

  /* ── State overlays ── */
  .state-overlay {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    height: 100%;
    color: var(--text-muted);
    font-size: 13px;
    font-family: var(--font-ui-sans);
    flex-direction: column;
  }
  .state-overlay.err   { color: var(--error, #c75450); }
  .state-overlay.muted { color: var(--text-disabled); }

  /* Determinate loading: progress bar + counters, fixed width so the bar
     doesn't stretch the full modal. */
  /* `.state-overlay` is the flex *container* (full width by default), so a
     fixed width here would left-align the whole block — center it with auto
     margins while constraining the bar's width. */
  .loading-blame { gap: 8px; width: 320px; max-width: 80%; margin: 0 auto; }
  .loading-blame :global(.progress-track) { align-self: stretch; }
  .loading-title {
    font-size: 13px;
    color: var(--text-secondary);
    font-weight: 500;
  }
  .loading-meta {
    font-size: 11px;
    color: var(--text-muted);
    font-family: var(--font-code);
  }
  .loading-commit {
    font-size: 11px;
    color: var(--text-disabled);
    font-family: var(--font-code);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .blame-retry {
    margin-top: 2px;
    padding: 5px 14px;
    font-size: 12px;
    font-family: var(--font-ui-sans);
    color: var(--text-secondary);
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .blame-retry:hover { background: var(--bg-hover); }


  /* ── Blame table ── */
  .blame-table {
    display: flex;
    flex-direction: column;
    min-width: max-content;
  }

  /* Top/bottom spacer for windowed virtualization — its height stands in for
     the rows we skipped, so scrollHeight matches the full N×ROW_HEIGHT and
     the scroll-thumb position stays consistent with the line numbers. */
  .blame-spacer {
    flex-shrink: 0;
    width: 1px;
  }

  .blame-row {
    display: flex;
    align-items: stretch;
    min-height: 21px;
    transition: background var(--transition-fast);
  }

  .blame-row.group-start { margin-top: 1px; }

  .blame-row.hovered { background: rgba(255, 255, 255, 0.035); }

  /* ── Line number — not selectable ── */
  .line-no {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    width: 52px;
    padding: 0 10px 0 8px;
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-disabled);
    flex-shrink: 0;
    user-select: none;
    border-right: 1px solid var(--border-subtle);
  }

  /* ── Blame gutter — not selectable ── */
  .gutter {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 250px;
    flex-shrink: 0;
    padding: 1px 10px 1px 0;
    border-right: 1px solid var(--border-subtle);
    overflow: hidden;
    user-select: none;
  }

  .gutter-bar {
    width: 2px;
    align-self: stretch;
    flex-shrink: 0;
    border-radius: 1px;
    background: var(--commit-color);
    opacity: 0.8;
  }

  .gutter-oid {
    font-family: var(--font-code);
    font-size: 11px;
    font-weight: 600;
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    flex-shrink: 0;
    letter-spacing: 0.3px;
    transition: opacity var(--transition-fast);
  }
  .gutter-oid:hover { opacity: 0.65; text-decoration: underline; }

  .gutter-author {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    flex-shrink: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .gutter-date {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    flex-shrink: 0;
    white-space: nowrap;
    margin-left: auto;
  }

  .gutter-continuation { flex: 1; }

  /* ── Line content — selectable, Prism-highlighted ── */
  .line-content {
    flex: 1;
    padding: 1px 16px;
    font-family: var(--font-code);
    font-size: 12.5px;
    color: var(--text-primary);
    white-space: pre;
    line-height: 1.6;
    cursor: text;
    user-select: text;
  }

  /* Inherit Prism token colors from the rest of the app */
  .line-content :global(.token.comment)   { color: var(--syntax-comment,  #6a9955); }
  .line-content :global(.token.string)    { color: var(--syntax-string,   #ce9178); }
  .line-content :global(.token.keyword)   { color: var(--syntax-keyword,  #569cd6); }
  .line-content :global(.token.number)    { color: var(--syntax-number,   #b5cea8); }
  .line-content :global(.token.function)  { color: var(--syntax-function, #dcdcaa); }
  .line-content :global(.token.operator)  { color: var(--syntax-operator, #d4d4d4); }
  .line-content :global(.token.punctuation) { color: var(--syntax-punct,  #d4d4d4); }
  .line-content :global(.token.class-name)  { color: var(--syntax-type,   #4ec9b0); }
  .line-content :global(.token.builtin)     { color: var(--syntax-type,   #4ec9b0); }
</style>
