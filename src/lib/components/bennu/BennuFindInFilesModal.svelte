<script lang="ts">
  /**
   * BennuFindInFilesModal — find-in-project as a modal (Ctrl+Shift+F, palette).
   *
   * Replaces the old Search rail tool. Same seam as before: it scans the loaded
   * file sources (every file the editor has opened this session; in demo mode the
   * whole demo project is available) and lists line matches. A real recursive
   * backend grep lands with the language service — the shape stays the same.
   *
   * Keyboard-first: the query input auto-focuses; ↑/↓ move the highlighted hit;
   * Enter opens it (and closes the modal); Esc cancels (Modal owns Esc). Reuses
   * the shared Modal + ModalHeader + SearchBar; no bespoke chrome.
   */
  import { Search, FileCode2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import type { TreeNode } from '$lib/types/bennu';

  let { onClose }: { onClose: () => void } = $props();

  interface Hit { path: string; name: string; line: number; text: string; }

  let query = $state('');
  let sel = $state(0);
  let listEl = $state<HTMLDivElement | null>(null);

  function baseName(p: string): string { return p.split(/[\\/]/).pop() ?? p; }

  function fileNodes(node: TreeNode | null): TreeNode[] {
    if (!node) return [];
    if (!node.is_dir) return [node];
    return node.children.flatMap(fileNodes);
  }
  const files = $derived(fileNodes(projectStore.tree));

  // Ensure every project file's source is available so search covers all of them.
  let ready = $state(false);
  $effect(() => {
    void projectStore.project; // re-arm on project switch
    ready = false;
    const active = projectStore.activeFilePath;
    void Promise.all(files.map((f) => projectStore.openFile(f.path)))
      .then(() => { if (active) projectStore.setActive(active); ready = true; })
      .catch(() => { ready = true; });
  });

  const hits = $derived.by<Hit[]>(() => {
    const q = query.trim().toLowerCase();
    if (q.length < 2 || !ready) return [];
    const out: Hit[] = [];
    for (const f of files) {
      const src = projectStore.sourceOf(f.path);
      if (!src) continue;
      const lines = src.split(/\r?\n/);
      for (let i = 0; i < lines.length; i++) {
        if (lines[i].toLowerCase().includes(q)) {
          out.push({ path: f.path, name: baseName(f.path), line: i + 1, text: lines[i].trim() });
          if (out.length >= 300) return out;
        }
      }
    }
    return out;
  });

  // Keep the selection in-range as results change.
  $effect(() => { if (sel >= hits.length) sel = Math.max(0, hits.length - 1); });

  async function openHit(h: Hit) {
    await projectStore.openFile(h.path);
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
      <SearchBar bind:query placeholder="Find in project…" showRegex={false} showCounter={false} autofocus />
    </div>

    {#if !projectStore.project}
      <EmptyState message="Open a project to search its files." />
    {:else if query.trim().length < 2}
      <EmptyState message="Type at least 2 characters to search." />
    {:else if hits.length === 0}
      <EmptyState message={`No matches for “${query.trim()}”.`} />
    {:else}
      <div class="ff-meta">{hits.length} match{hits.length === 1 ? '' : 'es'}</div>
      <div class="ff-list" bind:this={listEl}>
        {#each hits as h, i (h.path + ':' + h.line + ':' + i)}
          <button
            class="ff-hit"
            class:sel={i === sel}
            data-idx={i}
            onclick={() => openHit(h)}
            onmousemove={() => (sel = i)}
          >
            <span class="ff-icon"><FileCode2 size={13} /></span>
            <span class="ff-body">
              <span class="ff-line-text">{h.text}</span>
              <span class="ff-loc">{h.name}:{h.line}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</Modal>

<style>
  .ff { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .ff-search { padding: 10px 12px 8px; flex-shrink: 0; }
  .ff-meta {
    padding: 4px 14px; font-size: 10.5px; color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle); flex-shrink: 0;
  }
  .ff-list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; }
  .ff-hit {
    display: flex; align-items: flex-start; gap: 8px;
    width: 100%; text-align: left;
    padding: 6px 14px; background: transparent; border: none; cursor: pointer;
  }
  .ff-hit.sel { background: var(--accent-subtle); }
  .ff-hit:hover { background: var(--bg-hover); }
  .ff-hit.sel:hover { background: var(--accent-subtle); }
  .ff-icon { display: flex; color: var(--text-muted); flex-shrink: 0; margin-top: 1px; }
  .ff-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .ff-line-text {
    font-family: var(--font-code); font-size: 11.5px; color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%;
  }
  .ff-loc { font-size: 10px; color: var(--text-disabled); }
</style>
