<script lang="ts">
  /**
   * BennuUsagesPopover — the Alt+F7 "find usages" popover (IntelliJ "Show Usages").
   *
   * A caret-anchored floating list of the resolved use sites from `bennu_references`,
   * grouped visually by file. Fully keyboard-driven (↑/↓ move, Enter jumps, Esc
   * closes); picking a row opens the target file and jumps to the line. Mirrors
   * BennuIntentionsOverlay's floating/clamping/keyboard shell. State comes from
   * `bennuRefactorStore`; mounted once in BennuWindow.
   */
  import { tick } from 'svelte';
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { SearchCode, CornerDownRight, FileCode2 } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { bennuRefactorStore } from '$lib/stores/bennu/refactor.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import type { UsageHit } from '$lib/ipc/bennu/nav';

  const open = $derived(bennuRefactorStore.usagesOpen);
  const anchor = $derived(bennuRefactorStore.usagesAnchor);
  const loading = $derived(bennuRefactorStore.usagesLoading);
  const hits = $derived(bennuRefactorStore.usagesHits);
  const label = $derived(bennuRefactorStore.usagesLabel);
  const symbol = $derived(bennuRefactorStore.usagesSymbol);
  /** The server this file needs and does not have — see the empty state below. */
  const missingServer = $derived(bennuLspStore.missingServerFor(projectStore.activeFilePath));

  let panelEl = $state<HTMLElement | null>(null);
  let active = $state(0);

  $effect(() => {
    if (open) {
      active = 0;
      tick().then(() => panelEl?.focus());
    }
  });

  const PANEL_W = 460;
  let pos = $state<{ x: number; y: number }>({ x: 0, y: 0 });
  $effect(() => {
    if (!open || !panelEl) return;
    void hits.length; void loading;
    const a = anchor;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const rect = panelEl.getBoundingClientRect();
    let x = a ? a.x : vw / 2 - PANEL_W / 2;
    let y = a ? a.y + 6 : vh / 3;
    x = Math.min(Math.max(8, x), vw - rect.width - 8);
    if (a && y + rect.height > vh - 8) y = Math.max(8, a.y - rect.height - 6);
    else y = Math.min(Math.max(8, y), vh - rect.height - 8);
    pos = { x, y };
  });

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function jump(h: UsageHit) {
    bennuRefactorStore.closeUsages();
    void projectStore.openFile(h.file).then(() => bennuUiStore.requestGoto(h.line));
  }

  function onKeydown(e: KeyboardEvent) {
    e.stopPropagation();
    const n = hits.length;
    switch (e.key) {
      case 'Escape':
        e.preventDefault();
        bennuRefactorStore.closeUsages();
        return;
      case 'ArrowDown':
        if (!n) return;
        e.preventDefault();
        active = (active + 1) % n;
        return;
      case 'ArrowUp':
        if (!n) return;
        e.preventDefault();
        active = (active - 1 + n) % n;
        return;
      case 'Home':
        if (!n) return;
        e.preventDefault();
        active = 0;
        return;
      case 'End':
        if (!n) return;
        e.preventDefault();
        active = n - 1;
        return;
      case 'Enter':
        if (!n) return;
        e.preventDefault();
        if (hits[active]) jump(hits[active]);
        return;
    }
  }
</script>

{#if open}
  <div
    class="usages-backdrop"
    role="presentation"
    onpointerdown={() => bennuRefactorStore.closeUsages()}
    oncontextmenu={(e) => { e.preventDefault(); bennuRefactorStore.closeUsages(); }}
  ></div>

  <div
    bind:this={panelEl}
    class="usages"
    role="listbox"
    tabindex="-1"
    aria-label="Usages"
    style="left: {pos.x}px; top: {pos.y}px; width: {PANEL_W}px;"
    onkeydown={onKeydown}
    in:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
    out:fade={{ duration: animStore.dFast }}
  >
    <div class="usages-head">
      <SearchCode size={12} />
      {#if label}
        <span class="uh-title">Usages of <code>{label}</code></span>
        <span class="uh-count">{hits.length}</span>
      {:else}
        <span class="uh-title">Find usages</span>
      {/if}
    </div>

    {#if loading}
      <div class="usages-state"><Spinner size={13} /> Searching…</div>
    {:else if hits.length === 0}
      <div class="usages-state muted">
        <!--
          "No usages" is a claim about the code, and it must not be made on behalf of a server that
          is not there. A file is routed to its language server whether or not the binary exists —
          deliberately, so a `.ts` never falls through to the Java engine — which means every
          feature behind it answers nothing, convincingly. This says the other thing.
        -->
        {#if missingServer}
          <span><strong>{missingServer.name}</strong> is not installed, so nothing can answer for
          this file. Install it from Settings → Language Servers.</span>
        {:else if symbol}No usages of <code>{symbol}</code> found.{:else}Place the caret on a symbol, then press Alt+F7.{/if}
      </div>
    {:else}
      <div class="usages-list">
        {#each hits as h, i (i)}
          <button
            class="usage"
            class:active={i === active}
            type="button"
            role="option"
            aria-selected={i === active}
            onmousemove={() => (active = i)}
            onclick={() => jump(h)}
            title={h.file}
          >
            <span class="u-icon"><CornerDownRight size={12} /></span>
            <span class="u-file"><FileCode2 size={11} /> {baseName(h.file)}</span>
            <span class="u-pos">{h.line}:{h.col}</span>
            <span class="u-preview" class:on={i === active}>{h.preview}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .usages-backdrop { position: fixed; inset: 0; z-index: calc(var(--z-menu) - 1); background: transparent; }
  .usages {
    position: fixed; z-index: var(--z-menu);
    display: flex; flex-direction: column;
    max-height: 420px;
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); box-shadow: var(--shadow-popup); outline: none;
    overflow: hidden;
  }
  .usages-head {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 10px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-muted);
  }
  .uh-title { flex: 1; min-width: 0; font-size: var(--font-size-xs); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .uh-title code { font-family: var(--font-code); color: var(--text-primary); }
  .uh-count { font-size: var(--font-size-2xs); font-weight: 700; font-variant-numeric: tabular-nums; padding: 0 5px; border-radius: var(--radius-sm); background: var(--bg-overlay); color: var(--text-muted); }

  .usages-state { display: flex; align-items: center; gap: 7px; padding: 12px 14px; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .usages-state.muted { color: var(--text-muted); }
  .usages-state code { font-family: var(--font-code); color: var(--text-secondary); }

  .usages-list { overflow-y: auto; padding: 3px; }
  .usage {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left;
    padding: 4px 8px; background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer; font-family: var(--font-ui-sans);
  }
  .usage.active { background: var(--bg-selected); }
  .u-icon { display: flex; flex-shrink: 0; color: var(--text-disabled); }
  .u-file { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .u-file :global(svg) { color: var(--text-disabled); }
  .u-pos { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); flex-shrink: 0; min-width: 34px; font-variant-numeric: tabular-nums; }
  .u-preview { flex: 1; min-width: 0; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .u-preview.on { color: var(--text-primary); }
</style>
