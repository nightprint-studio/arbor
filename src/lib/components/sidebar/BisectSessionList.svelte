<script lang="ts">
  import { Search, Play, Pencil, Trash2, Bug, Pause } from 'lucide-svelte';
  import { bisectStore } from '$lib/stores/bisect.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { BisectSession } from '$lib/types/git';
  import InlineEdit from '$lib/components/shared/ui/InlineEdit.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    tabId,
    onResume,
  }: {
    tabId: string;
    onResume: () => void;
  } = $props();

  const sessions = $derived(bisectStore.sessions);

  let renamingId  = $state<string | null>(null);
  let renameValue = $state('');

  function startRename(s: BisectSession) {
    renamingId  = s.id;
    renameValue = s.name;
  }

  async function commitRename(s: BisectSession, value?: string) {
    const msg = (value ?? renameValue).trim();
    renamingId = null;
    if (!msg || msg === s.name) return;
    try {
      await bisectStore.renameSession(tabId, s.id, msg);
    } catch (err) { uiStore.showToast(`${err}`, 'error'); }
  }

  function cancelRename() { renamingId = null; }

  async function handleResume(s: BisectSession) {
    try {
      await bisectStore.resume(tabId, s.id);
      onResume();
      const next = bisectStore.state?.current_hash ?? bisectStore.state?.result_hash;
      if (next) graphStore.scrollToCommit(next);
      uiStore.showToast(`Session "${s.name}" resumed`, 'info');
    } catch (err) { uiStore.showToast(`Resume failed: ${err}`, 'error'); }
  }

  async function handleDelete(s: BisectSession) {
    try {
      await bisectStore.deleteSession(tabId, s.id);
    } catch (err) { uiStore.showToast(`${err}`, 'error'); }
  }

  function formatDate(ms: number): string {
    return new Date(ms).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
  }
</script>

{#if sessions.length === 0}
  <EmptyState message="No saved sessions" />
{:else}
  <ul class="session-list">
    {#each sessions as s (s.id)}
      <li class="session-item" class:completed={s.status === 'completed'}>
        <div class="session-top">
          <span class="status-dot" class:paused={s.status === 'paused'} class:completed={s.status === 'completed'}>
            {#if s.status === 'completed'}<Bug size={10} />{:else}<Pause size={10} />{/if}
          </span>

          {#if renamingId === s.id}
            <InlineEdit bind:value={renameValue} onconfirm={(v) => commitRename(s, v)} oncancel={cancelRename} />
          {:else}
            <span class="session-name truncate" use:tooltip={s.name}>{s.name}</span>
          {/if}

          <div class="session-actions">
            <button class="act-btn resume-btn" onclick={() => handleResume(s)} use:tooltip={s.status === 'paused' ? 'Resume this session' : 'Reload bisect state into graph'}>
              <Play size={10} />
            </button>
            {#if s.result_hash}
              <button class="act-btn goto-btn" onclick={() => graphStore.scrollToCommit(s.result_hash!)} use:tooltip={'Go to result commit'}>
                <Search size={10} />
              </button>
            {/if}
            <button class="act-btn" onclick={() => startRename(s)} use:tooltip={'Rename'}>
              <Pencil size={10} />
            </button>
            <button class="act-btn danger-btn" onclick={() => handleDelete(s)} use:tooltip={'Delete'}>
              <Trash2 size={10} />
            </button>
          </div>
        </div>

        <div class="session-meta">
          <span class="meta-date">{formatDate(s.created_at)}</span>
          {#if s.result_hash}
            <code class="meta-hash result">{s.result_hash.slice(0, 7)}</code>
          {/if}
          <span class="meta-counts">
            <span class="bad-count">{s.bad_hashes.length}✗</span>
            <span class="good-count">{s.good_hashes.length}✓</span>
          </span>
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .session-list {
    list-style: none;
    margin: 0;
    padding: 2px 0;
  }

  .session-item {
    padding: 5px 10px;
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .session-item:last-child { border-bottom: none; }

  .session-top {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .status-dot {
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }
  .status-dot.paused   { color: var(--accent); }
  .status-dot.completed { color: var(--color-bisect); }

  .session-name {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    color: var(--text-primary);
  }

  .session-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity var(--anim-dur-fast, 80ms);
  }
  .session-item:hover .session-actions { opacity: 1; }

  .act-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: var(--radius-sm);
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    padding: 0;
  }
  .act-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .resume-btn { color: var(--accent); }
  .resume-btn:hover { background: color-mix(in srgb, var(--accent) 15%, transparent); color: var(--accent); }
  .goto-btn { color: var(--color-bisect); }
  .goto-btn:hover { background: color-mix(in srgb, var(--color-bisect) 15%, transparent); color: var(--color-bisect); }
  .danger-btn:hover { color: var(--danger, var(--color-bisect)); }

  .session-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 15px;
  }

  .meta-date {
    font-size: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .meta-hash {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--color-bisect);
    background: color-mix(in srgb, var(--color-bisect) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-bisect) 25%, transparent);
    border-radius: var(--radius-sm);
    padding: 0 3px;
  }

  .meta-counts {
    display: flex;
    gap: 4px;
    font-size: 10px;
    margin-left: auto;
  }
  .bad-count  { color: var(--color-bisect); }
  .good-count { color: var(--success); }

  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
