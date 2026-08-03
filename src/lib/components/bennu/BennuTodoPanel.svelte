<script lang="ts">
  /**
   * TODO tool window (bottom dock) — every TODO/FIXME/XXX/HACK marker in the project
   * from `bennu_todos`, grouped by file, collapsible, with per-kind filter chips.
   * Clicking a row opens the file and jumps to the line. Fetches on mount + when the
   * project changes; the panel owns its header, its count and its Refresh action.
   */
  import { ListTodo, ChevronRight, ChevronDown, FileCode2, ArrowRight, Copy, RefreshCw } from 'lucide-svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { todos as ipcTodos } from '$lib/ipc/bennu';
  import type { TodoItem } from '$lib/types/bennu';

  const KINDS = ['TODO', 'FIXME', 'XXX', 'HACK'] as const;

  let items = $state<TodoItem[]>([]);
  let loading = $state(false);
  const collapsed = new SvelteSet<string>();
  const hiddenKinds = new SvelteSet<string>();

  let lastRoot: string | null = null;
  $effect(() => {
    const root = projectStore.project?.root ?? null;
    if (root !== lastRoot) { lastRoot = root; void refresh(); }
  });

  export async function refresh() {
    const root = projectStore.project?.root;
    if (!root) { items = []; return; }
    loading = true;
    try {
      items = await ipcTodos(root);
    } catch {
      items = [];
    } finally {
      loading = false;
    }
  }

  const shown = $derived(items.filter((t) => !hiddenKinds.has(t.kind)));
  const byFile = $derived.by(() => {
    const m = new Map<string, TodoItem[]>();
    for (const t of shown) {
      const arr = m.get(t.file);
      if (arr) arr.push(t); else m.set(t.file, [t]);
    }
    return [...m.entries()];
  });

  function baseName(path: string): string { return path.split(/[\\/]/).pop() ?? path; }
  function toggleFile(file: string) { if (collapsed.has(file)) collapsed.delete(file); else collapsed.add(file); }
  function toggleKind(kind: string) { if (hiddenKinds.has(kind)) hiddenKinds.delete(kind); else hiddenKinds.add(kind); }
  function open(t: TodoItem) {
    void projectStore.openFile(t.file).then(() => bennuUiStore.requestGoto(t.line));
  }
  function countOf(kind: string): number { return items.filter((t) => t.kind === kind).length; }

  function copyText(text: string) {
    // Best-effort — clipboard can be denied (permission / focus); swallow.
    void navigator.clipboard?.writeText(text).catch(() => { /* clipboard denied — ignore */ });
  }

  function onRowContextMenu(t: TodoItem, e: MouseEvent) {
    e.preventDefault();
    // Local name avoids shadowing the module-level `items` TODO state.
    const menuItems: MenuItem[] = [
      { id: 'goto', label: 'Go to', icon: ArrowRight },
      { id: 'copy-text', label: 'Copy text', icon: Copy },
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, menuItems, (id) => {
      switch (id) {
        case 'goto':      open(t); break;
        case 'copy-text': copyText(t.text); break;
      }
    });
  }
</script>

<div class="todo">
  <BottomPanelHeader
    title="TODO"
    count={shown.length}
    onClose={() => bennuUiStore.closeBottom()}
  >
    {#snippet icon()}<ListTodo size={13} />{/snippet}
    {#snippet actions()}
      <button
        class="ps-btn"
        type="button"
        use:tooltip={'Refresh'}
        aria-label="Refresh TODOs"
        disabled={loading}
        onclick={() => void refresh()}
      >
        <RefreshCw size={13} />
      </button>
    {/snippet}
  </BottomPanelHeader>

  <div class="todo-bar">
    {#each KINDS as k (k)}
      <button
        class="chip k-{k.toLowerCase()}"
        class:off={hiddenKinds.has(k)}
        type="button"
        onclick={() => toggleKind(k)}
        title={hiddenKinds.has(k) ? `Show ${k}` : `Hide ${k}`}
      >{k} <span class="chip-n">{countOf(k)}</span></button>
    {/each}
  </div>

  {#if loading}
    <div class="state"><Spinner size={13} /> Scanning…</div>
  {:else if shown.length === 0}
    <div class="todo-empty">
      <ListTodo size={20} />
      <EmptyState message={items.length ? 'No markers match the filter.' : 'No TODO / FIXME markers found.'} />
    </div>
  {:else}
    <div class="list">
      {#each byFile as [file, group] (file)}
        <button class="grp" type="button" onclick={() => toggleFile(file)}>
          {#if collapsed.has(file)}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
          <FileCode2 size={12} />
          <span class="grp-name">{baseName(file)}</span>
          <span class="grp-n">{group.length}</span>
        </button>
        {#if !collapsed.has(file)}
          {#each group as t, i (i)}
            <button class="row" type="button" onclick={() => open(t)} oncontextmenu={(e) => onRowContextMenu(t, e)}>
              <span class="tag k-{t.kind.toLowerCase()}">{t.kind}</span>
              <span class="row-text">{t.text || '—'}</span>
              <span class="row-line">{t.line}</span>
            </button>
          {/each}
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .todo { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow: hidden; }
  .todo-bar { display: flex; align-items: center; gap: 5px; padding: 6px 10px; flex-shrink: 0; border-bottom: 1px solid var(--border-subtle); flex-wrap: wrap; }
  .chip {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: var(--font-size-2xs); font-weight: 700; letter-spacing: 0.3px;
    padding: 1px 7px; border-radius: 999px; cursor: pointer;
    background: var(--bg-overlay); border: 1px solid var(--border-subtle); color: var(--text-secondary);
  }
  .chip.off { opacity: 0.4; }
  .chip-n { font-weight: 600; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .k-todo { color: var(--info); }
  .k-fixme { color: var(--warning); }
  .k-xxx { color: var(--error); }
  .k-hack { color: var(--text-secondary); }

  .state { display: flex; align-items: center; gap: 7px; padding: 12px 14px; font-size: var(--font-size-sm); color: var(--text-secondary); }
  .todo-empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; color: var(--text-disabled); }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 3px 0; }
  .grp {
    display: flex; align-items: center; gap: 6px; width: 100%; text-align: left;
    padding: 4px 10px; background: transparent; border: none; cursor: pointer;
    font-size: var(--font-size-xs); color: var(--text-primary); font-family: var(--font-ui-sans);
  }
  .grp:hover { background: var(--bg-hover); }
  .grp :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  .grp-name { font-weight: 500; }
  .grp-n { font-size: var(--font-size-2xs); color: var(--text-muted); }

  .row {
    display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
    padding: 3px 10px 3px 28px; background: transparent; border: none; cursor: pointer;
    font-family: var(--font-ui-sans);
  }
  .row:hover { background: var(--bg-hover); }
  .tag { flex-shrink: 0; font-size: var(--font-size-3xs); font-weight: 700; letter-spacing: 0.3px; min-width: 42px; text-align: center; padding: 0 4px; border-radius: var(--radius-sm); background: var(--bg-overlay); }
  .row-text { flex: 1; min-width: 0; font-size: var(--font-size-xs); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-line { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); flex-shrink: 0; }
</style>
