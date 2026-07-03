<script lang="ts">
  /**
   * BennuGotoModal — the Go-to navigator: quick-open a class (Ctrl+N) or a file
   * (Ctrl+Shift+N) by fuzzy name. Classes come from `bennu_class_index` (a fresh
   * project scan); files are flattened from the project tree. Fully keyboard-driven:
   * type to filter, ↑/↓ to move, Enter to open, Esc to close.
   *
   * A class jumps to its declaration line; a file just opens. Binary files are
   * excluded from the file list (they can't be opened anyway).
   */
  import { FileCode2, Box } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import type { ClassEntry, TreeNode } from '$lib/types/bennu';

  let { onClose }: { onClose: () => void } = $props();

  const mode = $derived(bennuUiStore.navMode);
  const title = $derived(mode === 'class' ? 'Go to Class' : 'Go to File');

  // Row model unified across modes: a primary label, a secondary (path/fqcn), the
  // file to open, and an optional line to jump to.
  interface Row { primary: string; secondary: string; file: string; line: number | null; cls: boolean; }

  let query = $state('');
  let active = $state(0);

  // ── Class source (fetched once on open) ─────────────────────────────────────
  let classes = $state<ClassEntry[]>([]);
  let loading = $state(false);
  $effect(() => {
    if (mode !== 'class') return;
    const root = projectStore.project?.root;
    if (!root) { classes = []; return; }
    loading = true;
    let cancelled = false;
    // Served from the per-root cache (instant after the first index); the cache is
    // invalidated when the index rebuilds, so a fresh open re-fetches the new set.
    void bennuIndexStore.classesForRoot(root)
      .then((c) => { if (!cancelled) { classes = c; loading = false; } })
      .catch(() => { if (!cancelled) { classes = []; loading = false; } });
    return () => { cancelled = true; };
  });

  // ── File source (flattened from the tree) ───────────────────────────────────
  const BINARY = /\.(png|jpe?g|gif|bmp|ico|webp|xcf|psd|pdf|zip|jar|war|ear|class|exe|dll|so|dylib|bin|o|obj|a|lib|7z|gz|tar|rar|mp3|mp4|wav|avi|mov|mkv|ttf|otf|woff2?|eot|db|sqlite)$/i;
  function flattenFiles(node: TreeNode | null, out: { name: string; path: string }[]) {
    if (!node) return;
    if (!node.is_dir) {
      if (!BINARY.test(node.name)) out.push({ name: node.name, path: node.path });
      return;
    }
    for (const c of node.children) flattenFiles(c, out);
  }
  const files = $derived.by(() => {
    if (mode !== 'file') return [];
    const out: { name: string; path: string }[] = [];
    flattenFiles(projectStore.tree, out);
    return out;
  });

  // ── Filtering / ranking ─────────────────────────────────────────────────────
  // Precompute the searchable entries + their lowercased keys ONCE per source change
  // (NOT per keystroke) — the old code re-`toLowerCase()`d every item on every key and
  // sorted thousands of matches with `localeCompare`, which stalls the input on a big
  // project. Now each keystroke only does a cheap `indexOf` over pre-lowercased keys +
  // a plain `<` sort.
  interface Entry {
    primary: string; secondary: string; file: string; line: number | null; cls: boolean;
    k1: string; // primary, lowercased
    k2: string; // secondary, lowercased
  }
  const entries = $derived.by<Entry[]>(() => {
    if (mode === 'class') {
      return classes.map((c) => ({
        primary: c.simple, secondary: c.fqcn, file: c.file, line: c.line, cls: true,
        k1: c.simple.toLowerCase(), k2: c.fqcn.toLowerCase(),
      }));
    }
    return files.map((f) => ({
      primary: f.name, secondary: f.path, file: f.path, line: null, cls: false,
      k1: f.name.toLowerCase(), k2: f.path.toLowerCase(),
    }));
  });

  /** True when every char of `q` appears in order in `h` (camelCase-ish fuzzy). */
  function subseq(h: string, q: string): boolean {
    let qi = 0;
    for (let k = 0; k < h.length && qi < q.length; k++) if (h[k] === q[qi]) qi++;
    return qi === q.length;
  }

  /** Rank an entry against a lowercased query. Matching is anchored on the **name**
   *  (`k1`): a name prefix/substring ranks highest, a path/fqcn (`k2`) substring next,
   *  and a subsequence match is allowed ONLY on the name — never on the full path, which
   *  is what used to flood the list with unrelated files (`stepCategori` subsequence-
   *  matching half the tree). Returns 0 (filtered out) when the name neither contains
   *  nor loosely spells the query and the path doesn't contain it verbatim. */
  function scoreEntry(e: Entry, q: string): number {
    const ni = e.k1.indexOf(q);
    if (ni === 0) return 5; // name prefix
    if (ni > 0) return 4;   // name substring
    if (e.k2.indexOf(q) >= 0) return 3; // path / fqcn substring
    if (subseq(e.k1, q)) return 2;      // fuzzy — name only
    return 0;
  }

  const rows = $derived.by<Row[]>(() => {
    const q = query.trim().toLowerCase();
    const scored: { e: Entry; s: number }[] = [];
    if (!q) {
      for (const e of entries) scored.push({ e, s: 1 });
    } else {
      for (const e of entries) {
        const s = scoreEntry(e, q);
        if (s > 0) scored.push({ e, s });
      }
    }
    scored.sort((a, b) => b.s - a.s || (a.e.k1 < b.e.k1 ? -1 : a.e.k1 > b.e.k1 ? 1 : 0));
    return scored
      .slice(0, 5000)
      .map((x) => ({ primary: x.e.primary, secondary: x.e.secondary, file: x.e.file, line: x.e.line, cls: x.e.cls }));
  });

  // ── Virtualized rendering ────────────────────────────────────────────────────
  // A big legacy project has thousands of files/classes; rendering every matched row
  // to the DOM on each keystroke is what made typing lag. We render only the rows in
  // (and just around) the viewport, with spacer divs standing in for the rest.
  const ROW_H = 30; // px — fixed row height (must match the .row height in CSS)
  const OVERSCAN = 8;
  let listEl = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(0);

  const startIdx = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const endIdx = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN));
  const visibleRows = $derived(rows.slice(startIdx, endIdx));
  const padTop = $derived(startIdx * ROW_H);
  const padBottom = $derived(Math.max(0, (rows.length - endIdx) * ROW_H));

  function onListScroll(e: Event) { scrollTop = (e.currentTarget as HTMLDivElement).scrollTop; }

  /** Keep the keyboard-highlighted row inside the viewport (called on ↑/↓ only, so a
   *  mouse hover never yanks the scroll). */
  function scrollActiveIntoView() {
    const el = listEl;
    if (!el) return;
    const top = active * ROW_H;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW_H > el.scrollTop + el.clientHeight) el.scrollTop = top + ROW_H - el.clientHeight;
  }

  // Reset the highlight + scroll to the top whenever the filtered set changes (depends
  // on `rows` only — NOT on `active` — so hovering a row never resets the scroll).
  $effect(() => {
    void rows;
    active = 0;
    scrollTop = 0;
    if (listEl) listEl.scrollTop = 0;
  });

  function pick(r: Row | undefined) {
    if (!r) return;
    onClose();
    void projectStore.openFile(r.file).then(() => { if (r.line) bennuUiStore.requestGoto(r.line); });
  }

  function onKeydown(e: KeyboardEvent) {
    const n = rows.length;
    if (e.key === 'ArrowDown') { e.preventDefault(); if (n) { active = (active + 1) % n; scrollActiveIntoView(); } }
    else if (e.key === 'ArrowUp') { e.preventDefault(); if (n) { active = (active - 1 + n) % n; scrollActiveIntoView(); } }
    else if (e.key === 'Enter') { e.preventDefault(); pick(rows[active]); }
  }
</script>

<Modal {onClose} width="620px" height="520px" padBody={false} ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if mode === 'class'}<Box size={14} />{:else}<FileCode2 size={14} />{/if}
      <span class="modal-title">{title}</span>
    </ModalHeader>
  {/snippet}

  <div class="body">
    <div class="search">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div onkeydown={onKeydown} class="search-wrap">
        <Input bind:value={query} placeholder={mode === 'class' ? 'Type a class name…' : 'Type a file name…'} />
      </div>
    </div>

    {#if loading}
      <div class="state"><Spinner size={13} /> {bennuIndexStore.indexing ? 'Indexing project…' : 'Loading classes…'}</div>
    {:else if rows.length === 0}
      <div class="state muted">{query ? 'No matches.' : (mode === 'class' ? 'No classes indexed.' : 'No files.')}</div>
    {:else}
      <div
        class="list"
        role="listbox"
        tabindex="-1"
        aria-label={title}
        bind:this={listEl}
        bind:clientHeight={viewportH}
        onscroll={onListScroll}
      >
        <div style="height:{padTop}px" aria-hidden="true"></div>
        {#each visibleRows as r, i (startIdx + i)}
          {@const gi = startIdx + i}
          <button
            class="row"
            class:active={gi === active}
            type="button"
            role="option"
            aria-selected={gi === active}
            style="height:{ROW_H}px"
            onmousemove={() => (active = gi)}
            onclick={() => pick(r)}
            title={r.secondary}
          >
            <span class="r-icon">{#if r.cls}<Box size={13} />{:else}<FileCode2 size={13} />{/if}</span>
            <span class="r-primary">{r.primary}</span>
            <span class="r-secondary">{r.secondary}</span>
          </button>
        {/each}
        <div style="height:{padBottom}px" aria-hidden="true"></div>
      </div>
    {/if}
  </div>
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .search { padding: 12px 14px 8px; flex-shrink: 0; }
  .search-wrap { display: contents; }

  .state { display: flex; align-items: center; gap: 7px; padding: 14px 16px; font-size: 12px; color: var(--text-secondary); }
  .state.muted { color: var(--text-muted); }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 2px 6px 8px; }
  .row {
    display: flex; align-items: center; gap: 9px;
    width: 100%; text-align: left; box-sizing: border-box; flex-shrink: 0;
    padding: 5px 8px; background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer; font-family: var(--font-ui-sans);
  }
  .row.active { background: var(--bg-selected); }
  .r-icon { display: flex; flex-shrink: 0; color: var(--text-muted); }
  .r-primary { flex-shrink: 0; font-size: 12.5px; color: var(--text-primary); font-weight: 500; }
  .r-secondary { flex: 1; min-width: 0; font-size: 11px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
</style>
