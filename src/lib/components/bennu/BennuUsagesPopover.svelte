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
  import { SearchCode, FileCode2 } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { bennuRefactorStore } from '$lib/stores/bennu/refactor.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import type { UsageHit } from '$lib/ipc/bennu/nav';
  import { tooltip } from '$lib/actions/tooltip';

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

  const PANEL_W = 640;
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

  /** `type a.b.Foo` → kind `type`, package `a.b.`, name `Foo`: the long qualifier is context, so it
   *  is dimmed and the name leads instead of being the part the ellipsis eats. */
  const titleParts = $derived.by(() => {
    const m = /^(?:([a-z]+)\s+)?(?:([\w.$]*\.))?(.*)$/.exec(label ?? '');
    return { kind: m?.[1] ?? '', qualifier: m?.[2] ?? '', name: m?.[3] ?? label ?? '' };
  });

  const isTestFile = (path: string) => /[\\/]src[\\/]test[\\/]|Test(s|Case)?\.\w+$/.test(path);

  /** The searched identifier — what gets marked inside each preview line. */
  const needle = $derived(/([A-Za-z_$][\w$]*)\W*$/.exec(symbol ?? '')?.[1] ?? '');

  function splitPreview(text: string): [string, string, string] {
    const line = text.trim();
    if (!needle) return [line, '', ''];
    const m = new RegExp(`(?<![\\w$])${needle.replace(/[$]/g, '\\$')}(?![\\w$])`).exec(line);
    return m ? [line.slice(0, m.index), m[0], line.slice(m.index + m[0].length)] : [line, '', ''];
  }

  /** The hits as rows: a header each time the file changes, then its uses. `index` is the flat
   *  position keyboard navigation moves through. */
  type Row = { kind: 'file'; file: string; count: number } | { kind: 'hit'; hit: UsageHit; index: number };
  const rows = $derived.by<Row[]>(() => {
    const out: Row[] = [];
    let current = '';
    hits.forEach((hit, index) => {
      if (hit.file !== current) {
        current = hit.file;
        let count = 0;
        for (let j = index; j < hits.length && hits[j].file === current; j++) count++;
        out.push({ kind: 'file', file: current, count });
      }
      out.push({ kind: 'hit', hit, index });
    });
    return out;
  });

  $effect(() => {
    if (!open || !panelEl) return;
    panelEl.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  });

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
    style="left: {pos.x}px; top: {pos.y}px; width: min({PANEL_W}px, calc(100vw - 16px));"
    onkeydown={onKeydown}
    in:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
    out:fade={{ duration: animStore.dFast }}
  >
    <div class="usages-head">
      <SearchCode size={12} />
      {#if label}
        <span class="uh-title" use:tooltip={label}>
          Usages of
          {#if titleParts.kind}<span class="uh-kind">{titleParts.kind}</span>{/if}
          <code><span class="uh-qual">{titleParts.qualifier}</span><strong>{titleParts.name}</strong></code>
        </span>
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
        {#each rows as row, n (n)}
          {#if row.kind === 'file'}
            <div class="u-filehead" use:tooltip={row.file}>
              <FileCode2 size={11} />
              <span class="u-fname">{baseName(row.file)}</span>
              {#if isTestFile(row.file)}<span class="u-test">test</span>{/if}
              <span class="u-fcount">{row.count}</span>
            </div>
          {:else}
            {@const h = row.hit}
            {@const i = row.index}
            {@const [before, hit, after] = splitPreview(h.preview)}
            <button
              class="usage"
              class:active={i === active}
              type="button"
              role="option"
              aria-selected={i === active}
              data-i={i}
              onmousemove={() => (active = i)}
              onclick={() => jump(h)}
            >
              <span class="u-pos">{h.line}</span>
              <!-- A use found through a generated accessor is spelled as that accessor, not as what
                   was asked about — saying so is what keeps `order.getName()` from reading as a bug
                   in the search when you asked about the field `name`. -->
              {#if h.via}<span class="u-via">{h.via}</span>{/if}
              <span class="u-preview" class:on={i === active}>{before}{#if hit}<mark>{hit}</mark>{/if}{after}</span>
            </button>
          {/if}
        {/each}
      </div>
      <div class="usages-foot">
        <span><Kbd keys={['↑', '↓']} size="sm" /> navigate</span>
        <span><Kbd keys={['Enter']} size="sm" /> open</span>
        <span><Kbd keys={['Esc']} size="sm" /> close</span>
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
  .uh-title code strong { font-weight: 600; }
  .uh-qual { color: var(--text-disabled); }
  .uh-kind { font-size: var(--font-size-2xs); text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-muted); margin: 0 2px; }
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
  .usage { padding-left: 24px; }
  .u-filehead {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 8px 2px; color: var(--text-disabled);
  }
  .u-fname { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-secondary); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .u-fcount { margin-left: auto; font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; color: var(--text-muted); }
  .u-test {
    font-size: var(--font-size-3xs); font-weight: 700; letter-spacing: 0.03em; text-transform: uppercase;
    padding: 0 4px; border-radius: var(--radius-sm);
    color: var(--warning); background: color-mix(in srgb, var(--warning) 16%, transparent);
  }
  .u-pos { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); flex-shrink: 0; min-width: 30px; text-align: right; font-variant-numeric: tabular-nums; }
  .u-preview mark { background: color-mix(in srgb, var(--accent) 22%, transparent); color: var(--text-primary); border-radius: 2px; padding: 0 1px; }
  .usages-foot {
    display: flex; gap: 14px; flex-shrink: 0;
    padding: 5px 10px; border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .usages-foot span { display: inline-flex; align-items: center; gap: 4px; }
  .u-preview { flex: 1; min-width: 0; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .u-preview.on { color: var(--text-primary); }
  .u-via {
    flex: none;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--accent-info, var(--text-muted));
    background: var(--bg-hover);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }
</style>
