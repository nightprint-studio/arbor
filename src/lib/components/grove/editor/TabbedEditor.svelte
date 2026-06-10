<script lang="ts">
  /**
   * Editor pane: a JetBrains-style tab strip over the open `.grove` files, a
   * code-block toolbar (breadcrumb · goto-line · copy · read-only), and the
   * read-only CodeView. All mocked — files don't save; "new file" appends an
   * untitled tab. The whole pane sits on --bg-base like Arbor's editor card.
   *
   * Imports only shared/ui/ (Tabs) + grove-local code + Arbor's tooltip action.
   */
  import { Hash, FileMusic, BookLock, Copy, ChevronRight } from 'lucide-svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import CodeView from './CodeView.svelte';
  import { groveStore } from '../grove-store.svelte';
  import { MOCK_PROJECT, MOCK_OUTLINE } from '../mock/data';
  import type { GroveFile } from '../mock/types';

  let untitled = $state<GroveFile[]>([]);
  const allFiles = $derived<GroveFile[]>([...MOCK_PROJECT.files, ...untitled]);
  const openFiles = $derived(
    groveStore.openFileIds
      .map(id => allFiles.find(f => f.id === id))
      .filter((f): f is GroveFile => !!f),
  );
  const active = $derived(allFiles.find(f => f.id === groveStore.activeFileId) ?? openFiles[0]);

  const tabs = $derived<TabItem[]>(
    openFiles.map(f => ({
      id: f.id, label: f.name,
      icon: f.library ? BookLock : FileMusic, iconSize: 13, title: f.path,
    })),
  );

  // Breadcrumb segments of the active file path.
  const crumbs = $derived(active ? active.path.split('/') : []);

  // ── Goto-line (Ctrl+G) ──────────────────────────────────────────────────────
  let gotoOpen = $state(false);
  let gotoValue = $state('');
  let flashLine = $state<number | null>(null);
  let gotoInputEl = $state<HTMLInputElement | null>(null);
  let copied = $state(false);

  export function openGoto() {
    gotoOpen = true; gotoValue = '';
    queueMicrotask(() => gotoInputEl?.focus());
  }
  export function newFile() {
    const n = untitled.length + 1;
    const f: GroveFile = {
      id: `f-untitled-${n}`, name: `untitled-${n}.grove`, path: `untitled-${n}.grove`,
      library: false, source: 'cps(0.5)\n\ntracks(\n  track("main", n(c4 e4 g4).inst("synth.pluck")),\n)\n',
    };
    untitled = [...untitled, f];
    groveStore.openFile(f.id);
  }

  function commitGoto() {
    const ln = parseInt(gotoValue.replace(/[^0-9]/g, ''), 10);
    if (!Number.isNaN(ln) && ln > 0) flashLine = ln;
    gotoOpen = false;
  }
  function onGotoKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); commitGoto(); }
    else if (e.key === 'Escape') { e.preventDefault(); gotoOpen = false; }
  }

  function gotoDecl(word: string) {
    const hit = MOCK_OUTLINE.find(o => o.label === word || o.label.startsWith(word + '('));
    if (hit) flashLine = hit.line;
  }

  async function copySource() {
    if (!active) return;
    try { await navigator.clipboard.writeText(active.source); copied = true; setTimeout(() => copied = false, 1200); }
    catch { /* clipboard blocked — mock, ignore */ }
  }
</script>

<div class="ed">
  <!-- Tab strip -->
  <div class="ed-tabs">
    <Tabs
      items={tabs}
      value={active?.id ?? null}
      variant="panel"
      size="sm"
      closable
      overflow
      onSelect={(id) => groveStore.setActiveFile(id)}
      onClose={(id) => groveStore.closeFile(id)}
      onAdd={newFile}
      addLabel="New .grove (Ctrl+N)"
    />
  </div>

  <!-- Code-block toolbar -->
  <div class="ed-toolbar">
    <div class="ed-crumbs">
      {#each crumbs as c, i (i)}
        {#if i > 0}<ChevronRight size={12} class="crumb-sep" />{/if}
        <span class="crumb" class:last={i === crumbs.length - 1}>{c}</span>
      {/each}
    </div>
    <div class="ed-actions">
      <button class="ed-tool" use:tooltip={'Go to line (Ctrl+G)'} aria-label="Go to line" onclick={openGoto}><Hash size={13} /></button>
      <button class="ed-tool" use:tooltip={copied ? 'Copied!' : 'Copy source'} aria-label="Copy source" onclick={copySource}><Copy size={13} /></button>
      <span class="ed-ro" use:tooltip={'Step 0 — editor is read-only (no engine yet)'}>read-only</span>
    </div>
  </div>

  {#if active}
    <CodeView source={active.source} {flashLine} onGotoDecl={gotoDecl} />
  {/if}

  {#if gotoOpen}
    <div class="ed-goto" role="dialog" aria-label="Go to line">
      <Hash size={13} />
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:this={gotoInputEl} bind:value={gotoValue} onkeydown={onGotoKey} onblur={() => gotoOpen = false} placeholder="Line number…" inputmode="numeric" />
    </div>
  {/if}
</div>

<style>
  .ed {
    display: flex; flex-direction: column;
    flex: 1; min-width: 0; min-height: 0;
    background: var(--bg-base);
    position: relative;
  }

  /* Tab strip + toolbar on bg-base (the whole editor pane is one bg-base card,
     matching the arrangement). Hairline dividers separate the chrome rows. */
  .ed-tabs {
    display: flex; align-items: stretch;
    height: 32px; min-height: 32px;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
  }
  .ed-tabs :global(.tabs) { flex: 1; min-width: 0; }

  /* Code-block toolbar — breadcrumb + actions. */
  .ed-toolbar {
    display: flex; align-items: center;
    height: 28px; min-height: 28px;
    padding: 0 8px 0 10px;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
  }
  .ed-crumbs { flex: 1; min-width: 0; display: flex; align-items: center; gap: 2px; overflow: hidden; }
  .crumb { font-size: 11px; color: var(--text-muted); white-space: nowrap; }
  .crumb.last { color: var(--text-secondary); font-weight: 500; }
  :global(.crumb-sep) { color: var(--text-disabled); flex-shrink: 0; }

  .ed-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
  .ed-tool {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 22px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ed-tool:hover { background: var(--bg-hover); color: var(--text-primary); }
  .ed-ro {
    font-size: 10px; color: var(--text-muted);
    text-transform: uppercase; letter-spacing: 0.4px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    padding: 1px 5px; margin-left: 2px;
  }

  .ed-goto {
    position: absolute; top: 64px; right: 14px;
    display: flex; align-items: center; gap: 6px;
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); box-shadow: var(--shadow-popup);
    padding: 6px 8px; color: var(--text-muted); z-index: 20;
  }
  .ed-goto input {
    background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-ui-sans);
    font-size: 12px; width: 120px;
  }
  .ed-goto input::placeholder { color: var(--text-disabled); }
</style>
