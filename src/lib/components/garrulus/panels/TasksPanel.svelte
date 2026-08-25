<script lang="ts">
  /**
   * Tasks — every checkbox in the vault, in one list.
   *
   * **The scan is offered, not performed.** There is no vault-wide task endpoint,
   * so this reads the notes and looks (see `vault-tasks.ts`), which is one round
   * trip per note. Doing that the moment `Ctrl+J` is pressed would spend a vault's
   * worth of reads on a panel the user may have opened to see something else, so
   * the panel starts by saying what the scan costs and waiting to be told. It is
   * the same reasoning that makes rebuilding the index a button rather than a
   * timer, and it is why the result is described as a snapshot with a count rather
   * than presented as live truth.
   *
   * **Ticking a box writes**, and that is allowed: it happens because a checkbox
   * was clicked. It is also the one write in this panel, it re-reads the note
   * first, and it refuses when the line is no longer the task it was — a stale
   * snapshot must never be able to overwrite an edit made since.
   */
  import { untrack } from 'svelte';
  import { ListTodo, Square, SquareCheckBig, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ChipBar, { type ChipItem } from '$lib/components/shared/ui/ChipBar.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import { noteFolder, noteName } from './note-path';
  import { SCAN_CAP, scanVaultTasks, setTaskState, type TaskScan, type VaultTask } from './vault-tasks';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Open the note a task lives in. Absent while no editor is mounted. */
    onOpenNote?: (id: string) => void;
  }

  let { onOpenNote }: Props = $props();

  type Filter = 'open' | 'done' | 'all';

  let scan = $state<TaskScan | null>(null);
  let tasks = $state<VaultTask[]>([]);
  let running = $state(false);
  let progress = $state<{ done: number; total: number } | null>(null);
  let error = $state<string | null>(null);
  let filter = $state<Filter>('open');
  /** The task whose write is in flight, so only its row goes inert. */
  let writing = $state<string | null>(null);

  let stopRequested = false;

  const key = (task: VaultTask) => `${task.note}:${task.line}`;

  /** Start (or restart) the scan. The dock header's action, and the empty
   *  state's button — one entry point either way. */
  export async function refresh(): Promise<void> {
    if (running || !garrulusVaultStore.isOpen) return;
    running = true;
    stopRequested = false;
    error = null;
    progress = { done: 0, total: 0 };
    try {
      const result = await scanVaultTasks({
        onProgress: (done, total) => (progress = { done, total }),
        cancelled: () => stopRequested,
      });
      scan = result;
      tasks = result.tasks;
    } catch (e) {
      // A vault that cannot answer is a real state, not a vault with no tasks.
      error = String(e);
      scan = null;
      tasks = [];
    } finally {
      running = false;
      progress = null;
    }
  }

  function cancel() {
    stopRequested = true;
  }

  /**
   * A different vault means a different set of notes, so the snapshot is thrown
   * away rather than re-taken: the scan is expensive and the user decides when to
   * spend it. The panel goes back to offering one.
   */
  let lastRoot: string | null = garrulusVaultStore.root;
  $effect(() => {
    const root = garrulusVaultStore.root;
    if (root === lastRoot) return;
    lastRoot = root;
    untrack(() => {
      stopRequested = true;
      scan = null;
      tasks = [];
      error = null;
    });
  });

  // Named `todo` rather than `open`: a local called `open` shadows the global of
  // that name, which is the class of bug CLAUDE.md keeps a rule about.
  const todo = $derived(tasks.filter((t) => !t.done));
  const done = $derived(tasks.filter((t) => t.done));

  const chips = $derived<ChipItem[]>([
    { id: 'open', label: 'To do', count: todo.length, tone: 'accent' },
    { id: 'done', label: 'Done', count: done.length, tone: 'success' },
    { id: 'all', label: 'Everything', count: tasks.length, tone: 'neutral' },
  ]);

  const visible = $derived(filter === 'open' ? todo : filter === 'done' ? done : tasks);

  /** One block per note, in the order the scan returned them — which is the
   *  index's title order, so the list is stable between scans. */
  const groups = $derived.by(() => {
    const byNote = new Map<string, { note: string; title: string; rows: VaultTask[] }>();
    for (const task of visible) {
      const group = byNote.get(task.note)
        ?? { note: task.note, title: task.title, rows: [] };
      group.rows.push(task);
      byNote.set(task.note, group);
    }
    return [...byNote.values()];
  });

  const summary = $derived.by(() => {
    if (!scan) return null;
    const parts = [`${scan.read} note${scan.read === 1 ? '' : 's'} read`];
    if (scan.skipped > 0) parts.push(`${scan.skipped} could not be read`);
    if (scan.capped) parts.push(`stopped at ${SCAN_CAP.toLocaleString()}`);
    if (scan.cancelled) parts.push('stopped early');
    return parts.join(' · ');
  });

  async function toggle(task: VaultTask) {
    if (writing) return;
    writing = key(task);
    try {
      const applied = await setTaskState(task, !task.done);
      if (!applied) {
        toastStore.show(
          `That line has changed in ${noteName(task.note)} since the scan — scan again.`,
          'warning',
        );
        return;
      }
      tasks = tasks.map((t) => (key(t) === key(task) ? { ...t, done: !task.done } : t));
    } catch (e) {
      toastStore.show(`Could not update ${noteName(task.note)}: ${e}`, 'error');
    } finally {
      writing = null;
    }
  }
</script>

<div class="tk">
  {#if scan || running}
    <div class="tk-bar">
      <ChipBar items={chips} selected={filter} size="sm" onSelect={(id) => (filter = id as Filter)} />
      <span class="tk-grow"></span>
      {#if running}
        <span class="tk-progress">
          <Spinner size={11} />
          {#if progress && progress.total > 0}
            {progress.done} / {progress.total}
          {:else}
            listing the notes…
          {/if}
        </span>
        <Button variant="ghost" size="xs" onclick={cancel}>
          {#snippet iconStart()}<X size={11} />{/snippet}
          Stop
        </Button>
      {:else if summary}
        <span class="tk-summary">{summary}</span>
      {/if}
    </div>
  {/if}

  <div class="tk-body">
    {#if !garrulusVaultStore.isOpen}
      <StateBlock tone="neutral" label="Open a vault to look for tasks in it." />
    {:else if error}
      <StateBlock tone="error" label={error} />
    {:else if !scan && !running}
      <StateBlock tone="neutral">
        <div class="tk-offer">
          <p>
            Nothing indexes the vault's checkboxes, so finding them means reading the
            notes — one read each, up to {SCAN_CAP.toLocaleString()}. It changes nothing,
            and the result is a snapshot of the moment you ask for it.
          </p>
          <Button variant="secondary" size="sm" onclick={() => void refresh()}>
            {#snippet iconStart()}<ListTodo size={13} />{/snippet}
            Scan the vault for tasks
          </Button>
        </div>
      </StateBlock>
    {:else if running && tasks.length === 0}
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size={14} />{/snippet}
        <span>Reading the notes…</span>
      </StateBlock>
    {:else if visible.length === 0}
      <StateBlock
        tone={filter === 'open' ? 'success' : 'neutral'}
        label={filter === 'open'
          ? 'No open task anywhere in the vault.'
          : 'Nothing here with that filter.'}
      />
    {:else}
      {#each groups as group (group.note)}
        <div class="tk-group">
          <button
            type="button"
            class="tk-note"
            disabled={!onOpenNote}
            use:tooltip={onOpenNote ? `Open ${group.title}` : 'No editor is attached to this panel yet'}
            onclick={() => onOpenNote?.(group.note)}
          >
            <span class="tk-note-name">{group.title || noteName(group.note)}</span>
            <span class="tk-note-folder">{noteFolder(group.note)}</span>
            <span class="tk-grow"></span>
            <span class="tk-count">{group.rows.length}</span>
          </button>

          {#each group.rows as task (key(task))}
            <div class="tk-row" class:busy={writing === key(task)}>
              <button
                type="button"
                class="tk-box"
                class:on={task.done}
                disabled={writing !== null}
                aria-pressed={task.done}
                use:tooltip={task.done ? 'Mark as not done' : 'Mark as done'}
                onclick={() => void toggle(task)}
              >
                {#if task.done}<SquareCheckBig size={12} />{:else}<Square size={12} />{/if}
              </button>
              <span class="tk-text" class:done={task.done}>{task.text}</span>
              <span class="tk-line">{task.line + 1}</span>
            </div>
          {/each}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .tk {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-base);
  }

  .tk-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .tk-grow { flex: 1; }
  .tk-progress {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
  }
  .tk-summary { font-size: var(--font-size-2xs); color: var(--text-muted); }

  .tk-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .tk-offer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    max-width: 56ch;
  }
  .tk-offer p {
    margin: 0;
    line-height: 1.55;
    text-align: center;
  }

  .tk-note {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    height: 26px;
    padding: 0 12px;
    border: none;
    background: var(--bg-elevated);
    position: sticky;
    top: 0;
    z-index: 1;
    text-align: left;
    cursor: pointer;
    font-family: inherit;
  }
  .tk-note:disabled { cursor: default; }
  .tk-note:not(:disabled):hover { background: var(--bg-hover); }

  .tk-note-name {
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tk-note-folder {
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .tk-count {
    flex: none;
    padding: 0 5px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-muted);
    font-size: var(--font-size-3xs);
    line-height: 15px;
  }

  .tk-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 0 14px;
    min-height: 22px;
  }
  .tk-row:hover { background: var(--bg-hover); }
  .tk-row.busy { opacity: 0.55; }

  .tk-box {
    display: flex;
    align-items: center;
    flex: none;
    padding: 2px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    cursor: pointer;
  }
  .tk-box.on { color: var(--success); }
  .tk-box:not(:disabled):hover { background: var(--bg-overlay); color: var(--text-primary); }

  .tk-text {
    flex: 1;
    min-width: 0;
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-secondary);
    overflow-wrap: anywhere;
  }
  .tk-text.done {
    color: var(--text-disabled);
    text-decoration: line-through;
  }

  .tk-line {
    flex: none;
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
    color: var(--text-disabled);
  }
</style>
