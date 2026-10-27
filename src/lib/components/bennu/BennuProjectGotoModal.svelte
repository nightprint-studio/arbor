<script lang="ts">
  /**
   * BennuProjectGotoModal — "Go to project…": find any project by typing.
   *
   * The Ctrl+Tab overlay is for the places you are *in* (open and indexed, newest first). This is
   * for everything else: every project of every workspace, plus the projects you have opened before
   * and that belong to none. One list, fuzzy-matched over the name and the path, so a project in a
   * workspace you have not touched in a month is three keystrokes away rather than a walk through
   * menus.
   *
   * Picking a workspace project switches workspace if needed (the same path as the title-bar tree);
   * picking a recent opens it into the current workspace.
   *
   * Quiet on purpose, like the rest of the editor chrome — but not static: rows come in on a short
   * stagger, and the selection is one highlight that *slides* between rows instead of lighting each
   * one in turn, which is what makes holding ↓ read as movement rather than flicker.
   */
  import { onMount, tick } from 'svelte';
  import { Search, History, CornerDownLeft } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { fuzzyMatchPair, segments, type MatchRange } from '$lib/utils/fuzzy';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { projectSwitcherStore } from '$lib/stores/bennu/project-switcher.svelte';
  import { workspacesStore, wsColorVar } from '$lib/stores/bennu/workspaces.svelte';
  import { listRecentProjects, type RecentProject } from '$lib/ipc/recents';

  let { onClose }: { onClose: () => void } = $props();

  interface Row {
    key: string;
    name: string;
    path: string;
    /** Set for a project that belongs to a workspace. */
    workspace?: { id: string; name: string; colorIdx: number };
    /** The project showing right now. */
    current: boolean;
    openedAt: number;
    rank: number;
    nameRanges: MatchRange[];
    pathRanges: MatchRange[];
  }

  const ROW_H = 52;

  let query = $state('');
  let active = $state(0);
  let recents = $state<RecentProject[]>([]);
  let listEl: HTMLElement | undefined = $state();

  onMount(() => {
    void listRecentProjects()
      .then((all) => { recents = all.filter((r) => r.product === 'bennu'); })
      .catch(() => {});
  });

  /** Paths compare the same however they were written: slashes, trailing slash, and case (the
   *  filesystems this runs on mostly ignore it). */
  function norm(p: string): string {
    return p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
  }

  const activeRoot = $derived(projectStore.project?.root ?? null);
  const activeWorkspace = $derived(workspacesStore.activeId);

  /** Everything findable, before any query: workspace projects, then recents that are in none. */
  const all = $derived.by<Row[]>(() => {
    const members = workspacesStore.allProjects();
    const known = new Set(members.map((m) => norm(m.root)));
    const rows: Row[] = members.map((m) => ({
      key: `${m.workspaceId}|${m.root}`,
      name: m.name,
      path: m.root,
      workspace: { id: m.workspaceId, name: m.workspaceName, colorIdx: m.colorIdx },
      current: m.workspaceId === activeWorkspace && norm(m.root) === norm(activeRoot ?? ''),
      openedAt: 0,
      rank: projectSwitcherStore.recencyRank(m),
      nameRanges: [],
      pathRanges: [],
    }));
    for (const r of recents) {
      if (known.has(norm(r.path))) continue;
      rows.push({
        key: `recent|${r.path}`,
        name: r.name,
        path: r.path,
        current: false,
        openedAt: r.opened_at,
        rank: Number.POSITIVE_INFINITY,
        nameRanges: [],
        pathRanges: [],
      });
    }
    return rows;
  });

  /** With no query: the places you have been, newest first, then the rest of your workspaces, then
   *  recents. With one: best match first, recency only breaking ties. */
  const rows = $derived.by<Row[]>(() => {
    const q = query.trim();
    if (!q) {
      return [...all].sort((a, b) =>
        (a.rank - b.rank) || (a.workspace ? 0 : 1) - (b.workspace ? 0 : 1) || b.openedAt - a.openedAt);
    }
    const scored: { row: Row; score: number }[] = [];
    for (const row of all) {
      const m = fuzzyMatchPair(row.name, row.path, q);
      if (m) scored.push({ row: { ...row, nameRanges: m.nameRanges, pathRanges: m.detailRanges }, score: m.score });
    }
    scored.sort((a, b) => (b.score - a.score) || (a.row.rank - b.row.rank));
    return scored.map((s) => s.row);
  });

  $effect(() => { void rows; active = 0; });

  // Keep the selection on screen; `nearest` so it only scrolls when it has to.
  $effect(() => {
    const i = active;
    void tick().then(() => {
      if (!listEl) return;
      const top = i * ROW_H;
      if (top < listEl.scrollTop) listEl.scrollTop = top;
      else if (top + ROW_H > listEl.scrollTop + listEl.clientHeight) {
        listEl.scrollTop = top + ROW_H - listEl.clientHeight;
      }
    });
  });

  function pick(row: Row | undefined) {
    if (!row) return;
    onClose();
    if (row.workspace) void workspacesStore.switchToProject(row.workspace.id, row.path);
    else void projectStore.openProject(row.path);
  }

  function onKeydown(e: KeyboardEvent) {
    const n = rows.length;
    if (e.key === 'ArrowDown') { e.preventDefault(); if (n) active = (active + 1) % n; }
    else if (e.key === 'ArrowUp') { e.preventDefault(); if (n) active = (active - 1 + n) % n; }
    else if (e.key === 'PageDown') { e.preventDefault(); if (n) active = Math.min(n - 1, active + 6); }
    else if (e.key === 'PageUp') { e.preventDefault(); if (n) active = Math.max(0, active - 6); }
    else if (e.key === 'Enter') { e.preventDefault(); pick(rows[active]); }
  }
</script>

<Modal {onClose} width="620px" height="min(560px, 80vh)" padBody={false} ariaLabel="Go to project">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="pg" onkeydown={onKeydown}>
    <div class="pg-search">
      <Input bind:value={query} placeholder="Go to project…" autofocus ariaLabel="Go to project">
        {#snippet iconStart()}<Search size={14} />{/snippet}
      </Input>
    </div>

    {#if rows.length === 0}
      <EmptyState message={query ? 'No project matches.' : 'No projects yet — open a folder to start one.'} />
    {:else}
      <div class="pg-list" bind:this={listEl} role="listbox" aria-label="Projects">
        <div class="pg-inner" style:height="{rows.length * ROW_H}px">
          <!-- One highlight that glides to the selected row. -->
          <div class="pg-pill" style:transform="translateY({active * ROW_H}px)"></div>
          {#each rows as row, i (row.key)}
            <button
              type="button"
              class="pg-row"
              class:active={i === active}
              role="option"
              aria-selected={i === active}
              style:top="{i * ROW_H}px"
              style:--i={Math.min(i, 10)}
              onmousemove={() => (active = i)}
              onclick={() => pick(row)}
            >
              <Monogram
                name={row.name}
                size={30}
                color={row.workspace ? wsColorVar(row.workspace.colorIdx) : undefined}
              />
              <span class="pg-text">
                <span class="pg-name">
                  {#each segments(row.name, row.nameRanges) as seg, si (si)}<span class:hit={seg.hit}>{seg.text}</span>{/each}
                </span>
                <span class="pg-path">
                  {#each segments(row.path, row.pathRanges) as seg, si (si)}<span class:hit={seg.hit}>{seg.text}</span>{/each}
                </span>
              </span>
              {#if row.current}
                <span class="pg-tag current">current</span>
              {:else if row.workspace}
                <span class="pg-tag">{row.workspace.name}</span>
              {:else}
                <span class="pg-tag recent"><History size={10} /> recent</span>
              {/if}
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <div class="pg-foot">
      <span><Kbd keys={['↑', '↓']} size="sm" /> move</span>
      <span><Kbd keys={['Enter']} size="sm" /> <CornerDownLeft size={10} /> open</span>
      <span><Kbd keys={['Esc']} size="sm" /> close</span>
      <span class="pg-count">{rows.length}</span>
    </div>
  </div>
</Modal>

<style>
  .pg { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .pg-search { padding: 12px 12px 8px; }

  .pg-list { flex: 1; min-height: 0; overflow-y: auto; padding: 0 8px 4px; }
  .pg-inner { position: relative; }

  .pg-pill {
    position: absolute; left: 0; right: 0; top: 0; height: 48px; margin-top: 2px;
    border-radius: var(--radius-md, 8px);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
    transition: transform 140ms cubic-bezier(0.2, 0, 0, 1);
    pointer-events: none;
  }

  .pg-row {
    position: absolute; left: 0; right: 0; height: 52px;
    display: flex; align-items: center; gap: 12px; padding: 0 12px;
    background: none; border: 0; border-radius: var(--radius-md, 8px);
    color: var(--text-primary); text-align: left; cursor: pointer;
    font-family: var(--font-ui-sans);
    animation: pg-in 220ms cubic-bezier(0.2, 0, 0, 1) both;
    animation-delay: calc(var(--i) * 16ms);
  }
  .pg-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .pg-name {
    font-size: var(--font-size-sm); font-weight: 500;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .pg-path {
    font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  .hit { color: var(--accent); font-weight: 700; }

  .pg-tag {
    flex-shrink: 0; max-width: 150px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    padding: 2px 8px; border-radius: 999px; font-size: var(--font-size-3xs);
    color: var(--text-secondary); background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .pg-tag.current { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 35%, transparent); }
  .pg-tag.recent { display: inline-flex; align-items: center; gap: 4px; color: var(--text-muted); }
  .pg-row.active .pg-tag { background: color-mix(in srgb, var(--accent) 12%, var(--bg-overlay)); }

  .pg-foot {
    display: flex; align-items: center; gap: 14px; padding: 8px 14px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .pg-count { margin-left: auto; font-variant-numeric: tabular-nums; }

  @keyframes pg-in {
    from { opacity: 0; transform: translateY(6px); }
    to   { opacity: 1; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .pg-row { animation: none; }
    .pg-pill { transition: none; }
  }
</style>
