<script lang="ts">
  import { Loader, AlertCircle, User, Calendar } from 'lucide-svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { getFileBlame } from '$lib/ipc/diff';
  import { highlight } from '$lib/utils/diff-formatter';
  import type { BlameLine } from '$lib/types/git';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
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

  // ── Load blame ────────────────────────────────────────────────────────────────

  $effect(() => {
    loading = true;
    error   = null;
    lines   = [];
    getFileBlame(tabId, path)
      .then(r => { lines = r; })
      .catch(e => { error = String(e); })
      .finally(() => { loading = false; });
  });

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

  <div class="blame-scroll">
    {#if loading}
      <div class="state-overlay">
        <Loader size={20} class="spin" />
        <span>Loading blame…</span>
      </div>

    {:else if error}
      <div class="state-overlay err">
        <AlertCircle size={18} />
        <span>{error}</span>
      </div>

    {:else if lines.length === 0}
      <div class="state-overlay muted">
        <span>No blame data available</span>
      </div>

    {:else}
      <div class="blame-table">
        {#each lines as line (line.line_no)}
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

  :global(.spin) { animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* ── Blame table ── */
  .blame-table {
    display: flex;
    flex-direction: column;
    min-width: max-content;
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
