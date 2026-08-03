<script lang="ts">
  /**
   * Find in project (Ctrl+Shift+F) — the results list beside a **preview of the file**.
   *
   * The list alone answers "where does this string occur"; it does not answer the question
   * you actually opened it with, which is "is THIS the occurrence I meant". A one-line
   * excerpt is not enough to tell four identical `if (rs.next())` apart — the lines around
   * it are. So the selected hit is shown in context on the right, the way IntelliJ does it,
   * and walking the list with ↑/↓ re-reads it as you go. Nothing is opened until you press
   * Enter, which is what makes browsing a hundred hits cheap.
   *
   * Runs the backend recursive grep (`bennu_find_in_files`) **progressively**: each search
   * gets a fresh `searchId`, and results stream back as `arbor://bennu/find-progress` events
   * appended as they arrive — so a big legacy project fills the list incrementally instead
   * of freezing until the end. A `done` event ends the spinner. Events tagged with a
   * superseded id are ignored, so a newer query is never clobbered by a slower older scan.
   * Debounced (~250ms). When the BE is absent the call rejects and we render a graceful
   * empty state.
   *
   * The **file mask** (`*.java`, `*.jsp,*.tag`) filters what the scan returned rather than
   * what it scans: the BE takes no mask, and re-running the walk for a narrowing that a
   * client-side test answers instantly would be slower AND less responsive.
   *
   * Keyboard-first: the query field auto-focuses and keeps focus; ↑/↓ move the highlighted
   * hit (flattened across groups), PageUp/PageDown jump, Enter opens it and closes, Esc
   * cancels (Modal owns Esc). Replace is intentionally out of scope (no affordance).
   */
  import { Search, FileCode2, FolderTree, CornerDownLeft, Filter } from 'lucide-svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { findInFiles, readFile } from '$lib/ipc/bennu';
  import type { FindHit } from '$lib/types/bennu';

  let { onClose }: { onClose: () => void } = $props();

  // Seed from the editor selection when opened with one highlighted (bennuUiStore.findInitial),
  // else empty. Read once at mount — the value is set right before the modal opens.
  let query = $state(bennuUiStore.findInitial);
  let mask = $state('');
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
  // The field keeps the focus throughout: refining a query after looking at the results is the
  // normal case, so the arrows drive the list without ever leaving the input.
  let field = $state<HTMLInputElement | null>(null);
  $effect(() => { field?.focus(); });

  function baseName(p: string): string { return p.split(/[\\/]/).pop() ?? p; }

  /** A path shown relative to the project root — the part that tells two files apart. */
  function relPath(p: string): string {
    const root = projectStore.project?.root?.replace(/\\/g, '/').replace(/\/+$/, '') ?? '';
    const norm = p.replace(/\\/g, '/');
    return root && norm.startsWith(`${root}/`) ? norm.slice(root.length + 1) : norm;
  }

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

  // Re-run on any input change (query text or a toggle), debounced. The mask is NOT a
  // dependency — it filters what came back, so re-scanning for it would be pure waste.
  $effect(() => {
    void query; void regex; void caseSensitive; void wholeWord; void scope; void projectStore.project;
    if (debounceTimer !== undefined) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(runSearch, 250);
    return () => { if (debounceTimer !== undefined) clearTimeout(debounceTimer); };
  });

  // ── File mask ────────────────────────────────────────────────────────────────
  /** `*.java, *.jsp` → a test over the file name. An empty / all-blank mask passes everything. */
  const maskTest = $derived.by<(file: string) => boolean>(() => {
    const parts = mask.split(/[,;\s]+/).map((p) => p.trim()).filter(Boolean);
    if (!parts.length) return () => true;
    const res = parts.map((p) => {
      const body = p.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.');
      return new RegExp(`^${body}$`, 'i');
    });
    return (file: string) => {
      const name = baseName(file);
      return res.some((re) => re.test(name));
    };
  });

  const shown = $derived(hits.filter((h) => maskTest(h.file)));

  // ── Grouping (by file) + flat index for keyboard nav ─────────────────────────
  interface Group { file: string; name: string; dir: string; rows: { hit: FindHit; idx: number }[]; }
  const groups = $derived.by<Group[]>(() => {
    const byFile = new Map<string, Group>();
    shown.forEach((hit, idx) => {
      let g = byFile.get(hit.file);
      if (!g) {
        const rel = relPath(hit.file);
        const cut = rel.lastIndexOf('/');
        g = { file: hit.file, name: baseName(hit.file), dir: cut < 0 ? '' : rel.slice(0, cut), rows: [] };
        byFile.set(hit.file, g);
      }
      g.rows.push({ hit, idx });
    });
    return [...byFile.values()];
  });

  // Keep the selection in-range as results (or the mask) change.
  $effect(() => { if (sel >= shown.length) sel = Math.max(0, shown.length - 1); });

  const current = $derived<FindHit | undefined>(shown[sel]);

  // ── Preview of the selected hit ──────────────────────────────────────────────
  const CONTEXT = 6; // lines shown either side of the match
  /** Files already read, so walking a file's hits re-reads nothing. Cleared per opening. */
  const fileCache = new Map<string, string[]>();
  let previewLines = $state<{ n: number; text: string }[]>([]);
  let previewFile = $state('');
  let previewError = $state(false);

  $effect(() => {
    const hit = current;
    const root = projectStore.project?.root;
    if (!hit || !root) { previewLines = []; previewFile = ''; return; }
    let live = true;
    void (async () => {
      let lines = fileCache.get(hit.file);
      if (!lines) {
        try {
          const res = await readFile(root, hit.file);
          lines = res.text.split(/\r?\n/);
          fileCache.set(hit.file, lines);
        } catch {
          if (!live) return;
          previewError = true;
          previewLines = [];
          previewFile = hit.file;
          return;
        }
      }
      if (!live) return;
      const from = Math.max(0, hit.line - 1 - CONTEXT);
      const to = Math.min(lines.length, hit.line + CONTEXT);
      previewError = false;
      previewFile = hit.file;
      previewLines = lines.slice(from, to).map((text, i) => ({ n: from + i + 1, text }));
    })();
    return () => { live = false; };
  });

  // ── Match highlighting ───────────────────────────────────────────────────────
  // Split a line around the first match of the query so it can be emphasised. For regex we
  // do a lenient case-insensitive first-match; a bad pattern just yields no highlight (the
  // row still renders plainly).
  interface Segment { text: string; hit: boolean; }
  const matcher = $derived.by<RegExp | null>(() => {
    const q = query.trim();
    if (!q) return null;
    try {
      const flags = caseSensitive ? '' : 'i';
      if (regex) return new RegExp(q, flags);
      const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      return new RegExp(wholeWord ? `\\b${escaped}\\b` : escaped, flags);
    } catch {
      return null;
    }
  });

  function segments(text: string): Segment[] {
    const re = matcher;
    if (!re) return [{ text, hit: false }];
    const m = re.exec(text);
    if (!m || m.index < 0 || m[0].length === 0) return [{ text, hit: false }];
    return [
      { text: text.slice(0, m.index), hit: false },
      { text: text.slice(m.index, m.index + m[0].length), hit: true },
      { text: text.slice(m.index + m[0].length), hit: false },
    ];
  }

  async function openHit(h: FindHit) {
    await projectStore.openFile(h.file);
    bennuUiStore.requestGoto(h.line);
    onClose();
  }

  function move(delta: number) {
    if (!shown.length) return;
    sel = Math.min(Math.max(sel + delta, 0), shown.length - 1);
    scrollSelIntoView();
  }

  function onKey(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown': e.preventDefault(); move(1); break;
      case 'ArrowUp': e.preventDefault(); move(-1); break;
      case 'PageDown': e.preventDefault(); move(8); break;
      case 'PageUp': e.preventDefault(); move(-8); break;
      case 'Enter': {
        e.preventDefault();
        if (current) void openHit(current);
        break;
      }
      default: break;
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

<Modal {onClose} width="1000px" height="620px" padBody={false} bodyBorder>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Search size={14} />
      <span class="modal-title">Find in project</span>
    </ModalHeader>
  {/snippet}

  <div class="ff" onkeydown={onKey} role="presentation">
    <div class="ff-search">
      <Search size={15} />
      <input
        bind:this={field}
        bind:value={query}
        class="ff-field"
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="Find in project…"
        aria-label="Find in project"
      />
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

    <div class="ff-sub">
      <span class="ff-mask">
        <Filter size={12} />
        <Input bind:value={mask} placeholder="File mask — *.java, *.jsp" ariaLabel="File mask" />
      </span>
      <span class="ff-sub-spacer"></span>
      {#if hasQuery && shown.length}
        <span class="ff-count">
          {shown.length} match{shown.length === 1 ? '' : 'es'} in {groups.length} file{groups.length === 1 ? '' : 's'}
          {#if shown.length !== hits.length}<span class="ff-count-mask">of {hits.length}</span>{/if}
        </span>
      {/if}
      {#if loading}<span class="ff-live"><Spinner size={11} /> searching…</span>{/if}
      {#if capped}<span class="ff-cap">capped</span>{/if}
    </div>

    {#if !projectStore.project}
      <EmptyState message="Open a project to search its files." />
    {:else if !hasQuery}
      <EmptyState message="Type at least 2 characters to search." />
    {:else if shown.length === 0}
      {#if loading}
        <div class="ff-loading"><Spinner size="sm" label="Searching…" /></div>
      {:else if hits.length && mask.trim()}
        <EmptyState message={`${hits.length} match(es), none in files matching “${mask.trim()}”.`} />
      {:else}
        <EmptyState message={errored ? 'Search is unavailable for this project.' : `No matches for “${query.trim()}”.`} />
      {/if}
    {:else}
      <div class="ff-split">
        <div class="ff-list" bind:this={listEl}>
          {#each groups as g (g.file)}
            <div class="ff-group">
              <div class="ff-group-head" title={g.file}>
                <FileCode2 size={12} />
                <span class="ff-group-name">{g.name}</span>
                {#if g.dir}<span class="ff-group-dir">{g.dir}</span>{/if}
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
                  <span class="ff-line-text">{#each segments(hit.preview) as s, i (i)}{#if s.hit}<mark class="ff-mark">{s.text}</mark>{:else}{s.text}{/if}{/each}</span>
                </button>
              {/each}
            </div>
          {/each}
        </div>

        <div class="ff-preview">
          {#if previewError}
            <p class="ff-pv-note">This file can’t be previewed.</p>
          {:else if previewLines.length && current}
            <div class="ff-pv-head" title={previewFile}>{relPath(previewFile)}</div>
            <div class="ff-pv-body">
              {#each previewLines as l (l.n)}
                <div class="ff-pv-line" class:hit={l.n === current.line}>
                  <span class="ff-pv-n">{l.n}</span>
                  <span class="ff-pv-text">{#each segments(l.text) as s, i (i)}{#if s.hit && l.n === current.line}<mark class="ff-mark">{s.text}</mark>{:else}{s.text}{/if}{/each}</span>
                </div>
              {/each}
            </div>
          {:else}
            <p class="ff-pv-note">Reading…</p>
          {/if}
        </div>
      </div>

      <div class="ff-foot">
        <Kbd keys={["↑"]} size="sm" /><Kbd keys={["↓"]} size="sm" /><span>move</span>
        <span class="ff-foot-open"><CornerDownLeft size={11} /> open</span>
        <span class="ff-sub-spacer"></span>
        <Kbd keys={["Esc"]} size="sm" /><span>close</span>
      </div>
    {/if}
  </div>
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .ff { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  .ff-search {
    display: flex; align-items: center; gap: 8px;
    padding: 11px 14px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .ff-search > :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
  .ff-field {
    flex: 1; min-width: 0;
    background: none; border: none; outline: none;
    color: var(--text-primary); font-size: var(--font-size-lg);
  }
  .ff-field::placeholder { color: var(--text-disabled); }

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
  .ff-tgl.on { background: var(--accent-subtle); border-color: var(--accent); color: var(--accent); }
  .ff-tgl-w { font-size: var(--font-size-2xs); }

  .ff-sub {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 14px; flex-shrink: 0;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .ff-mask { display: flex; align-items: center; gap: 6px; min-width: 0; width: 260px; }
  .ff-mask :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
  .ff-sub-spacer { flex: 1; }
  .ff-count-mask { color: var(--text-disabled); margin-left: 4px; }
  .ff-live { display: inline-flex; align-items: center; gap: 4px; color: var(--accent); }
  .ff-cap { color: var(--warning); }

  .ff-loading { display: flex; align-items: center; justify-content: center; padding: 24px; }

  /* Results left, the selected hit in context right — the split is the point. */
  .ff-split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .ff-list { min-height: 0; overflow-y: auto; padding: 4px 0; border-right: 1px solid var(--border-subtle); }

  .ff-group { padding-bottom: 2px; }
  .ff-group-head {
    display: flex; align-items: baseline; gap: 6px;
    padding: 5px 14px 3px; color: var(--text-secondary);
    font-size: var(--font-size-xs); font-weight: 600;
  }
  .ff-group-head :global(svg) { align-self: center; color: var(--text-muted); flex-shrink: 0; }
  .ff-group-name { flex-shrink: 0; }
  .ff-group-dir {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: var(--font-size-3xs); font-weight: 400;
    color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
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
  .ff-hit.sel { background: var(--bg-selected); }
  .ff-hit:hover { background: var(--bg-hover); }
  .ff-hit.sel:hover { background: var(--bg-selected); }
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

  .ff-preview { min-height: 0; display: flex; flex-direction: column; background: var(--bg-base); }
  .ff-pv-head {
    flex-shrink: 0; padding: 6px 12px;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  .ff-pv-body { flex: 1; min-height: 0; overflow: auto; padding: 6px 0; }
  .ff-pv-line { display: flex; gap: 10px; padding: 0 12px; }
  .ff-pv-line.hit { background: var(--bg-selected); }
  .ff-pv-n {
    flex-shrink: 0; min-width: 34px; text-align: right;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-disabled);
    line-height: 1.6;
  }
  .ff-pv-text {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary);
    line-height: 1.6; white-space: pre; overflow: hidden; text-overflow: ellipsis;
  }
  .ff-pv-note { padding: 14px; font-size: var(--font-size-sm); color: var(--text-muted); }

  .ff-foot {
    display: flex; align-items: center; gap: 6px;
    padding: 7px 12px; flex-shrink: 0;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    font-size: var(--font-size-2xs); color: var(--text-disabled);
  }
  .ff-foot-open { display: inline-flex; align-items: center; gap: 4px; }
</style>
