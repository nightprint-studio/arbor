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
  }

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
      }));
    }
    if (MARKUP_EXTS.has(ext)) {
      const out: Item[] = [];
      const walk = (nodes: MarkupNode[], depth: number) => {
        for (const n of nodes) {
          out.push({ label: n.name, detail: n.detail, line: n.line, kind: 'tag', depth, k: n.name.toLowerCase() });
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

  const rows = $derived.by<Item[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((it) => it.k.includes(q) || (it.detail?.toLowerCase().includes(q) ?? false));
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

  function pick(it: Item | undefined) {
    if (!it) return;
    onClose();
    bennuUiStore.requestGoto(it.line);
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

<Modal {onClose} width="560px" height="480px" padBody={false} ariaLabel="File structure">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FileCode2 size={14} />
      <span class="modal-title">File structure</span>
      {#if activePath}<span class="hdr-file">{activePath.split(/[\\/]/).pop()}</span>{/if}
    </ModalHeader>
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
        {#each visibleRows as it, i (startIdx + i)}
          {@const gi = startIdx + i}
          <button
            class="row"
            class:active={gi === active}
            type="button"
            role="option"
            aria-selected={gi === active}
            style="height:{ROW_H}px; padding-left:{8 + it.depth * 12}px"
            onmousemove={() => (active = gi)}
            onclick={() => pick(it)}
            title={it.detail}
          >
            {#if it.visibility}
              <span class="dot" style="background:{VIS_COLOR[it.visibility] ?? 'var(--text-disabled)'}"></span>
            {/if}
            <span class="r-label">{it.label}</span>
            {#if it.detail}<span class="r-detail">{it.detail}</span>{/if}
            <span class="r-kind">{it.kind}</span>
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

  .fs { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .search { padding: 10px 12px 8px; flex-shrink: 0; }
  .state { padding: 14px 16px; font-size: var(--font-size-sm); color: var(--text-muted); }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 2px 6px 8px; }
  .row {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left; box-sizing: border-box; flex-shrink: 0;
    padding: 4px 8px; background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer; font-family: var(--font-ui-sans);
  }
  .row.active { background: var(--bg-selected); }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .r-label { font-size: var(--font-size-sm); color: var(--text-primary); flex-shrink: 0; white-space: nowrap; }
  .r-detail {
    flex: 1; min-width: 0; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .r-kind { flex-shrink: 0; font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.3px; color: var(--text-disabled); }
</style>
