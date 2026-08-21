<script lang="ts">
  /**
   * BennuFileStructureModal — IntelliJ's "File Structure" popup (Ctrl+F12): a
   * searchable quick-outline of the ACTIVE file. Type to filter, ↑/↓ to move, Enter to
   * jump to the symbol's line, Esc to close.
   *
   * The list source depends on the file type:
   *   • Java  → `javaOutline` (types / methods / fields, with a visibility dot).
   *   • XML / JSP / HTML / POM / … → `markupOutline` flattened (search by element name,
   *     the `keyValue:tag` label; indented by nesting) — the markup equivalent of
   *     IntelliJ's method search, matching on names rather than methods.
   *
   * Jumps happen inside the already-open active file (via the editor's goto relay), so
   * no file open is needed. Virtualized like the Go-to modal (a big struts config or a
   * long class can have many rows).
   */
  import { FileCode2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { kindGlyph } from './symbol-kind-glyph';
  import { fuzzyMatch, segments, type MatchRange } from '$lib/utils/fuzzy';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { javaOutline } from './java-outline';
  import { markupOutline, type MarkupNode } from './markup-outline';
  import { lspDocumentSymbols, type LspSymbol } from '$lib/ipc/bennu/lsp';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const activePath = $derived(projectStore.activeFilePath);
  const source = $derived(projectStore.activeSource);

  interface Item {
    label: string;
    detail?: string;
    line: number;
    kind: string;
    visibility?: string;
    depth: number;
    /** label, lowercased — the filter key, computed once. */
    k: string;
    /** Whether this row *contains* the rows under it — a type, an `impl`, a module.
     *  Drawn heavier, because a list of forty members is unreadable without the divisions
     *  the file itself has. */
    container?: boolean;
  }

  /** The kinds that hold other symbols. A server's vocabulary and Bennu's own, in one set —
   *  the same argument as `symbol-kind-glyph.ts`: a Rust `struct` and a Java `class` are the
   *  same thing to somebody scanning a list. */
  const CONTAINER_KINDS = new Set([
    'class', 'interface', 'enum', 'record', 'struct', 'trait', 'impl', 'object',
    'module', 'namespace', 'group',
  ]);

  function extOf(p: string | null): string {
    if (!p) return '';
    const name = p.split(/[\\/]/).pop() ?? p;
    const dot = name.lastIndexOf('.');
    return dot >= 0 ? name.slice(dot + 1).toLowerCase() : '';
  }
  const MARKUP_EXTS = new Set([
    'jsp', 'jspf', 'tag', 'tagx', 'xml', 'xsd', 'wsdl', 'xsl', 'xslt', 'tld',
    'pom', 'iml', 'fxml', 'svg', 'html', 'htm', 'xhtml',
  ]);

  const items = $derived.by<Item[]>(() => {
    const ext = extOf(activePath);
    if (ext === 'java') {
      return javaOutline(source).map((s) => ({
        label: s.name, detail: s.detail, line: s.line, kind: s.kind,
        visibility: s.visibility, depth: 0, k: s.name.toLowerCase(),
        container: CONTAINER_KINDS.has(s.kind.toLowerCase()),
      }));
    }
    if (MARKUP_EXTS.has(ext)) {
      const out: Item[] = [];
      const walk = (nodes: MarkupNode[], depth: number) => {
        for (const n of nodes) {
          out.push({
            label: n.name, detail: n.detail, line: n.line, kind: 'element', depth,
            k: n.name.toLowerCase(), container: !!n.children?.length,
          });
          if (n.children) walk(n.children, depth + 1);
        }
      };
      walk(markupOutline(source), 0);
      return out;
    }
    // A file a language server owns. Its outline is a round-trip, so it is not derivable here —
    // `lspItems` below fetches it and this returns what has landed.
    return lspItems;
  });

  // ── The server-supplied outline ──────────────────────────────────────────────
  //
  // Fetched rather than scanned, because for a `.rs` there is nothing local to scan. Without this
  // branch the modal opened *empty* on a Rust file: the handler and the IPC wrapper both existed and
  // nothing called them.
  //
  // Keyed on the path and the source, and sequence-numbered, so a slow answer for a file you have
  // left cannot replace the outline of the one you are looking at.
  let lspItems = $state<Item[]>([]);
  let lspSeq = 0;
  $effect(() => {
    const path = activePath;
    const src = source;
    const ext = extOf(path);
    if (!path || ext === 'java' || MARKUP_EXTS.has(ext) || !bennuLspStore.servesFile(path)) {
      lspItems = [];
      return;
    }
    const mine = ++lspSeq;
    void lspDocumentSymbols(path, src)
      .then((syms) => {
        if (mine !== lspSeq) return;
        const out: Item[] = [];
        const walk = (nodes: LspSymbol[], depth: number) => {
          for (const n of nodes) {
            out.push({
              label: n.name,
              detail: n.detail ?? undefined,
              line: n.line,
              kind: n.kind,
              depth,
              k: n.name.toLowerCase(),
              // A server says so directly: a symbol with children is a container, whatever it
              // is called. The kind set is the fallback for the ones that hold nothing yet —
              // an empty `impl` is still an `impl`.
              container: !!n.children?.length || CONTAINER_KINDS.has(n.kind.toLowerCase()),
            });
            if (n.children?.length) walk(n.children, depth + 1);
          }
        };
        walk(syms, 0);
        lspItems = out;
      })
      // Silent: the server may still be loading the workspace, and reopening asks again.
      .catch(() => { if (mine === lspSeq) lspItems = []; });
  });

  let query = $state('');
  let active = $state(0);

  /** A row, plus where the query landed in it. */
  interface Row {
    it: Item;
    ranges: MatchRange[];
  }

  /**
   * The visible rows.
   *
   * Matching is a **subsequence**, the same rule the Go-to navigator uses — `fst` finds
   * `from_state` — and the characters that matched are lit in the row, which is what keeps a
   * loose match legible rather than mysterious.
   *
   * **Declaration order is preserved**, deliberately: this is an outline, and its order is the
   * file's. Sorting by score would answer "which matches best" when the question being asked is
   * "where is it" — and a list that reshuffles as you type loses the one thing an outline has.
   */
  const rows = $derived.by<Row[]>(() => {
    const q = query.trim();
    if (!q) return items.map((it) => ({ it, ranges: [] }));
    const out: Row[] = [];
    for (const it of items) {
      const hit = fuzzyMatch(it.label, q);
      if (hit) {
        out.push({ it, ranges: hit.ranges });
        continue;
      }
      // The signature counts too — `&self` or a parameter's type is a fair thing to look for —
      // but it does not light the label, so no ranges.
      if (it.detail && fuzzyMatch(it.detail, q)) out.push({ it, ranges: [] });
    }
    return out;
  });

  // ── Virtualized list (same windowing as BennuGotoModal) ──────────────────────
  const ROW_H = 28;
  const OVERSCAN = 8;
  let listEl = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(0);
  const startIdx = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const endIdx = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN));
  const visibleRows = $derived(rows.slice(startIdx, endIdx));
  const padTop = $derived(startIdx * ROW_H);
  const padBottom = $derived(Math.max(0, (rows.length - endIdx) * ROW_H));
  function onScroll(e: Event) { scrollTop = (e.currentTarget as HTMLDivElement).scrollTop; }
  function scrollActiveIntoView() {
    const el = listEl;
    if (!el) return;
    const top = active * ROW_H;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW_H > el.scrollTop + el.clientHeight) el.scrollTop = top + ROW_H - el.clientHeight;
  }
  $effect(() => { void rows; active = 0; scrollTop = 0; if (listEl) listEl.scrollTop = 0; });

  function pick(row: Row | undefined) {
    if (!row) return;
    onClose();
    bennuUiStore.requestGoto(row.it.line);
  }
  function onKeydown(e: KeyboardEvent) {
    const n = rows.length;
    if (e.key === 'ArrowDown') { e.preventDefault(); if (n) { active = (active + 1) % n; scrollActiveIntoView(); } }
    else if (e.key === 'ArrowUp') { e.preventDefault(); if (n) { active = (active - 1 + n) % n; scrollActiveIntoView(); } }
    else if (e.key === 'Enter') { e.preventDefault(); pick(rows[active]); }
  }

  const VIS_COLOR: Record<string, string> = {
    public: 'var(--success)', protected: 'var(--warning)',
    private: 'var(--error)', package: 'var(--text-disabled)',
  };
</script>

<Modal {onClose} width="600px" height="520px" padBody={false} ariaLabel="File structure">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FileCode2 size={14} />
      <span class="modal-title">File structure</span>
      {#if activePath}<span class="hdr-file">{activePath.split(/[\\/]/).pop()}</span>{/if}
      <span class="hdr-count">{rows.length}{#if query.trim()} of {items.length}{/if}</span>
    </ModalHeader>
  {/snippet}

  {#snippet footer()}
    <div class="fs-foot">
      <span><Kbd keys={['↑', '↓']} size="sm" /> move</span>
      <span><Kbd keys={['Enter']} size="sm" /> go to</span>
      <span><Kbd keys={['Esc']} size="sm" /> close</span>
    </div>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fs" onkeydown={onKeydown}>
    <div class="search">
      <Input bind:value={query} placeholder="Filter…" autofocus ariaLabel="Filter file structure" />
    </div>

    {#if items.length === 0}
      <EmptyState message="No structure for this file." />
    {:else if rows.length === 0}
      <div class="state">No matches.</div>
    {:else}
      <div
        class="list"
        role="listbox"
        tabindex="-1"
        aria-label="File structure"
        bind:this={listEl}
        bind:clientHeight={viewportH}
        onscroll={onScroll}
      >
        <div style="height:{padTop}px" aria-hidden="true"></div>
        {#each visibleRows as row, i (startIdx + i)}
          {@const gi = startIdx + i}
          {@const it = row.it}
          {@const glyph = kindGlyph(it.kind)}
          <button
            class="row"
            class:active={gi === active}
            class:container={it.container}
            type="button"
            role="option"
            aria-selected={gi === active}
            style="height:{ROW_H}px"
            onmousemove={() => (active = gi)}
            onclick={() => pick(row)}
            title={it.detail ? `${it.kind} · ${it.detail}` : it.kind}
          >
            <!-- One rail per level of nesting. The padding alone said how deep a row was and
                 not *what it was under*, which on an outline of forty members is the whole
                 question — the same device the project tree uses. -->
            {#each { length: it.depth } as _, d (d)}
              <span class="rail" aria-hidden="true"></span>
            {/each}
            <span class="r-icon" style="color:{glyph.color}">
              <glyph.icon size={13} {...glyph.props ?? {}} />
            </span>
            {#if it.visibility}
              <span class="dot" style="background:{VIS_COLOR[it.visibility] ?? 'var(--text-disabled)'}"></span>
            {/if}
            <span class="r-label">
              {#each segments(it.label, row.ranges) as seg, si (si)}<span
                class:hit={seg.hit}>{seg.text}</span>{/each}
            </span>
            {#if it.detail}<span class="r-detail">{it.detail}</span>{/if}
          </button>
        {/each}
        <div style="height:{padBottom}px" aria-hidden="true"></div>
      </div>
    {/if}
  </div>
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-file { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted); }
  /* Pushed to the far end of the header row, where a count belongs: it is about the list, not
     about the file, and next to the name it read as part of it. */
  .hdr-count {
    margin-left: auto; font-size: var(--font-size-3xs); font-variant-numeric: tabular-nums;
    color: var(--text-disabled);
  }

  .fs { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .search { padding: 10px 12px 8px; flex-shrink: 0; }
  .state { padding: 14px 16px; font-size: var(--font-size-sm); color: var(--text-muted); }

  .fs-foot {
    display: flex; align-items: center; gap: 14px;
    font-size: var(--font-size-3xs); color: var(--text-muted);
  }
  .fs-foot span { display: inline-flex; align-items: center; gap: 4px; }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 2px 6px 8px; }
  .row {
    display: flex; align-items: center; gap: 7px;
    width: 100%; text-align: left; box-sizing: border-box; flex-shrink: 0;
    padding: 0 8px; background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer; font-family: var(--font-ui-sans);
  }
  .row.active { background: var(--bg-selected); }
  /* See `SymbolKindIcon.svelte`: a lettered ring's colours are picked to read against the panel,
     not against a selection fill. On the active row it takes the row's colour instead. */
  .row.active .r-icon { --jki-color: currentColor; }

  /* One per level of nesting, drawn where the indent used to be empty space. A hairline and
     not a heavier rule: forty of them down a long outline is a texture, and a texture that
     shouts competes with the names, which are what is being read. */
  .rail {
    flex: 0 0 12px; align-self: stretch; margin-right: -3px;
    border-left: 1px solid var(--border-subtle, var(--border));
    opacity: 0.55;
  }
  .row:first-child .rail { border-left-color: transparent; }

  .r-icon { display: inline-flex; align-items: center; flex-shrink: 0; }
  .dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
  .r-label { font-size: var(--font-size-sm); color: var(--text-primary); flex-shrink: 0; white-space: nowrap; }
  /* A type, an `impl`, a module — the divisions the file itself has. Weight rather than a
     colour or a rule: it separates without adding a third thing to look at, and it survives a
     selected row, where a background tint would not. */
  .row.container .r-label { font-weight: 650; }
  /* The matched characters, and the only lit thing in the row. Same device as the Go-to
     navigator, so a subsequence match reads the same wherever it is offered. */
  .r-label :global(.hit) { color: var(--accent); font-weight: 700; }
  .r-detail {
    flex: 1; min-width: 0; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
</style>
