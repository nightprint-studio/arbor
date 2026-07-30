<script lang="ts">
  /**
   * BennuFindInFilesModal — find-in-project as a modal (Ctrl+Shift+F, palette).
   *
   * Runs the backend recursive grep (`bennu_find_in_files`) **progressively**: each
   * search gets a fresh `searchId`, and results stream back as
   * `arbor://bennu/find-progress` events which we append as they arrive — so a big
   * legacy project fills the list incrementally instead of freezing until the end. A
   * `done` event ends the spinner. Events tagged with a superseded id are ignored, so a
   * newer query never gets clobbered by a slower older scan. Debounced (~250ms). When
   * the BE is absent the call rejects and we render a graceful empty state.
   *
   * Keyboard-first: the query input auto-focuses; ↑/↓ move the highlighted hit
   * (flattened across groups); Enter opens it (and closes the modal); Esc cancels
   * (Modal owns Esc). Rows are grouped by file. The matched substring is emphasised
   * with a <mark>-like span. Replace is intentionally out of scope (no affordance).
   */
  import { Search, FileCode2, FolderTree } from 'lucide-svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { findInFiles } from '$lib/ipc/bennu';
  import type { FindHit } from '$lib/types/bennu';

  let { onClose }: { onClose: () => void } = $props();

  // Seed from the editor selection when opened with one highlighted (bennuUiStore.findInitial),
  // else empty. Read once at mount — the value is set right before the modal opens.
  let query = $state(bennuUiStore.findInitial);
  let regex = $state(false);
  let caseSensitive = $state(false);
  let wholeWord = $state(false);
  // Search scope in a multi-project workspace: the active project only, or every member.
  let scope = $state<'project' | 'workspace'>('project');

  let hits = $state<FindHit[]>([]);
  let loading = $state(false);
  let errored = $state(false);
  let capped = $state(false);
  let sel = $state(0);
  let listEl = $state<HTMLDivElement | null>(null);

  function baseName(p: string): string { return p.split(/[\\/]/).pop() ?? p; }

  // ── Progressive search (streamed via `arbor://bennu/find-progress`) ───────────
  // Each run mints a fresh `currentId`; the event listener appends only the batches
  // tagged with it, so a slower superseded scan can never clobber a newer query.
  let seq = 0;
  let currentId = '';
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  // The BE payload shape (`{ id, hits?, done?, capped? }`).
  interface FindProgress { id: string; hits?: FindHit[]; done?: boolean; capped?: boolean }

  $effect(() => {
    let un: UnlistenFn | undefined;
    void listen<FindProgress>('arbor://bennu/find-progress', (e) => {
      const p = e.payload;
      if (p.id !== currentId) return; // a superseded search — ignore
      if (p.hits && p.hits.length) hits = [...hits, ...p.hits];
      if (p.capped) capped = true;
      if (p.done) loading = false;
    }).then((fn) => { un = fn; });
    return () => { un?.(); };
  });

  function runSearch() {
    const root = projectStore.project?.root;
    const q = query.trim();
    const id = `find-${++seq}`;
    currentId = id;
    hits = [];
    sel = 0;
    capped = false;
    if (!root || q.length < 2) {
      loading = false;
      errored = false;
      return;
    }
    loading = true;
    errored = false;
    // Workspace scope: also scan the OTHER member projects (the BE streams them into the same
    // search). Active-project scope leaves `extraRoots` empty.
    const extraRoots = scope === 'workspace'
      ? projectStore.workspaceProjects.map((p) => p.root).filter((r) => r !== root)
      : [];
    findInFiles(root, q, { regex, caseSensitive, wholeWord, extraRoots }, id).catch(() => {
      if (id !== currentId) return;
      // BE absent / rejected query (e.g. bad regex) → graceful empty state.
      hits = [];
      loading = false;
      errored = true;
    });
  }

  // Re-run on any input change (query text or a toggle), debounced.
  $effect(() => {
    // Touch the deps so the effect re-arms on each change.
    void query; void regex; void caseSensitive; void wholeWord; void scope; void projectStore.project;
    if (debounceTimer !== undefined) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(runSearch, 250);
    return () => { if (debounceTimer !== undefined) clearTimeout(debounceTimer); };
  });

  // ── Grouping (by file) + flat index for keyboard nav ─────────────────────────
  interface Group { file: string; name: string; rows: { hit: FindHit; idx: number }[]; }
  const groups = $derived.by<Group[]>(() => {
    const byFile = new Map<string, Group>();
    hits.forEach((hit, idx) => {
      let g = byFile.get(hit.file);
      if (!g) { g = { file: hit.file, name: baseName(hit.file), rows: [] }; byFile.set(hit.file, g); }
      g.rows.push({ hit, idx });
    });
    return [...byFile.values()];
  });

  // Keep the selection in-range as results change.
  $effect(() => { if (sel >= hits.length) sel = Math.max(0, hits.length - 1); });

  // ── Match highlighting ───────────────────────────────────────────────────────
  // Split the preview around the first match of the query so it can be wrapped in
  // an emphasised span. For regex we do a lenient case-insensitive first-match; a
  // bad pattern just yields no highlight (the row still renders plainly).
  interface Segment { text: string; hit: boolean; }
  function segments(preview: string): Segment[] {
    const q = query.trim();
    if (!q) return [{ text: preview, hit: false }];
    let re: RegExp | null = null;
    try {
      const flags = caseSensitive ? '' : 'i';
      if (regex) {
        re = new RegExp(q, flags);
      } else {
        const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const body = wholeWord ? `\\b${escaped}\\b` : escaped;
        re = new RegExp(body, flags);
      }
    } catch { re = null; }
    if (!re) return [{ text: preview, hit: false }];
    const m = re.exec(preview);
    if (!m || m.index < 0 || m[0].length === 0) return [{ text: preview, hit: false }];
    return [
      { text: preview.slice(0, m.index), hit: false },
      { text: preview.slice(m.index, m.index + m[0].length), hit: true },
      { text: preview.slice(m.index + m[0].length), hit: false },
    ];
  }

  async function openHit(h: FindHit) {
    await projectStore.openFile(h.file);
    bennuUiStore.requestGoto(h.line);
    onClose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      sel = Math.min(sel + 1, hits.length - 1);
      scrollSelIntoView();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      sel = Math.max(sel - 1, 0);
      scrollSelIntoView();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const h = hits[sel];
      if (h) void openHit(h);
    }
  }

  function scrollSelIntoView() {
    queueMicrotask(() => {
      const row = listEl?.querySelector<HTMLElement>(`[data-idx="${sel}"]`);
      row?.scrollIntoView({ block: 'nearest' });
    });
  }

  const hasQuery = $derived(query.trim().length >= 2);
</script>

<Modal {onClose} width="640px" height="520px" padBody={false} bodyBorder>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Search size={14} />
      <span class="modal-title">Find in project</span>
    </ModalHeader>
  {/snippet}

  <div class="ff" onkeydown={onKey} role="presentation">
    <div class="ff-search">
      <Input
        bind:value={query}
        placeholder="Find in project…"
        clearable
        autofocus
        ariaLabel="Find in project"
      >
        {#snippet iconStart()}<Search size={13} />{/snippet}
      </Input>
      <div class="ff-toggles">
        <button
          type="button"
          class="ff-tgl"
          class:on={caseSensitive}
          aria-pressed={caseSensitive}
          title="Match case"
          onclick={() => (caseSensitive = !caseSensitive)}
        >Aa</button>
        <button
          type="button"
          class="ff-tgl"
          class:on={wholeWord}
          aria-pressed={wholeWord}
          title="Whole word"
          onclick={() => (wholeWord = !wholeWord)}
        ><span class="ff-tgl-w">W</span></button>
        <button
          type="button"
          class="ff-tgl"
          class:on={regex}
          aria-pressed={regex}
          title="Regular expression"
          onclick={() => (regex = !regex)}
        >.*</button>
        {#if projectStore.hasWorkspace}
          <button
            type="button"
            class="ff-tgl"
            class:on={scope === 'workspace'}
            aria-pressed={scope === 'workspace'}
            aria-label="Search scope"
            title={scope === 'workspace'
              ? 'Scope: whole workspace (click to search the active project only)'
              : 'Scope: active project (click to search the whole workspace)'}
            onclick={() => (scope = scope === 'workspace' ? 'project' : 'workspace')}
          ><FolderTree size={13} /></button>
        {/if}
      </div>
    </div>

    {#if !projectStore.project}
      <EmptyState message="Open a project to search its files." />
    {:else if !hasQuery}
      <EmptyState message="Type at least 2 characters to search." />
    {:else if hits.length === 0}
      {#if loading}
        <div class="ff-loading"><Spinner size="sm" label="Searching…" /></div>
      {:else}
        <EmptyState message={errored ? 'Search is unavailable for this project.' : `No matches for “${query.trim()}”.`} />
      {/if}
    {:else}
      <div class="ff-meta">
        {hits.length} match{hits.length === 1 ? '' : 'es'} in {groups.length} file{groups.length === 1 ? '' : 's'}
        {#if loading}<span class="ff-meta-live"><Spinner size={11} /> searching…</span>{/if}
        {#if capped}<span class="ff-meta-cap">· capped</span>{/if}
      </div>
      <div class="ff-list" bind:this={listEl}>
        {#each groups as g (g.file)}
          <div class="ff-group">
            <div class="ff-group-head">
              <FileCode2 size={12} />
              <span class="ff-group-name">{g.name}</span>
              <span class="ff-group-count">{g.rows.length}</span>
            </div>
            {#each g.rows as { hit, idx } (hit.file + ':' + hit.line + ':' + hit.col + ':' + idx)}
              <button
                class="ff-hit"
                class:sel={idx === sel}
                data-idx={idx}
                onclick={() => openHit(hit)}
                onmousemove={() => (sel = idx)}
              >
                <span class="ff-loc">{hit.line}:{hit.col}</span>
                <span class="ff-line-text">{#each segments(hit.preview) as s}{#if s.hit}<mark class="ff-mark">{s.text}</mark>{:else}{s.text}{/if}{/each}</span>
              </button>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</Modal>

<style>
  .ff { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .ff-search {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 12px 8px; flex-shrink: 0;
  }
  .ff-toggles { display: flex; gap: 4px; flex-shrink: 0; }
  .ff-tgl {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 26px; height: 26px; padding: 0 5px;
    font-size: var(--font-size-xs); font-weight: 600; font-family: var(--font-code);
    color: var(--text-muted);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); cursor: pointer;
    transition: all var(--transition-fast);
  }
  .ff-tgl:hover { border-color: var(--border); color: var(--text-secondary); }
  .ff-tgl.on {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
  .ff-tgl-w { font-size: var(--font-size-2xs); }

  .ff-loading { display: flex; align-items: center; justify-content: center; padding: 24px; }

  .ff-meta {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 14px; font-size: var(--font-size-2xs); color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle); flex-shrink: 0;
  }
  .ff-meta-live { display: inline-flex; align-items: center; gap: 4px; color: var(--accent); }
  .ff-meta-cap { color: var(--warning); }
  .ff-list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; }

  .ff-group { padding-bottom: 2px; }
  .ff-group-head {
    display: flex; align-items: center; gap: 6px;
    padding: 5px 14px 3px; color: var(--text-secondary);
    font-size: var(--font-size-xs); font-weight: 600;
  }
  .ff-group-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ff-group-count {
    margin-left: auto; font-size: var(--font-size-3xs); font-weight: 500;
    color: var(--text-disabled);
    background: var(--bg-elevated); border-radius: 99px; padding: 0 6px;
  }

  .ff-hit {
    display: flex; align-items: baseline; gap: 10px;
    width: 100%; text-align: left;
    padding: 4px 14px 4px 30px; background: transparent; border: none; cursor: pointer;
  }
  .ff-hit.sel { background: var(--accent-subtle); }
  .ff-hit:hover { background: var(--bg-hover); }
  .ff-hit.sel:hover { background: var(--accent-subtle); }
  .ff-loc {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-disabled);
    flex-shrink: 0; min-width: 44px;
  }
  .ff-line-text {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0;
  }
  .ff-mark {
    background: var(--accent-subtle); color: var(--accent);
    border-radius: 2px; padding: 0 1px; font-weight: 600;
  }
</style>
