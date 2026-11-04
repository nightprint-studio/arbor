<script lang="ts">
  /**
   * The answer to a query: the places grouped by file with the selected one in context beside them — or, for a
   * query that groups, a table.
   *
   * The list says where; only the lines around it say whether that is the one you meant, so walking the list
   * re-reads the file beside it and nothing is opened until Enter.
   */
  import { ChevronDown, ChevronRight, FileCode2, TriangleAlert } from 'lucide-svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { untrack } from 'svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import CodePreview from '$lib/components/shared/ui/CodePreview.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { readFile } from '$lib/ipc/bennu';
  import { languageForPath } from '../languages';
  import { bennuSsrStore as store } from '$lib/stores/bennu/ssr.svelte';
  import { groupByFile, stepIndex } from './ssr-model';
  import HoverCard from '$lib/components/shared/ui/HoverCard.svelte';
  import SsrHitCard from './SsrHitCard.svelte';
  import type { SsrHit } from '$lib/ipc/bennu/ssr';

  let { root, onOpen }: { root: string; onOpen: (file: string, line: number) => void } = $props();

  const groups = $derived(groupByFile(store.hits));
  /** The hits in the order the list shows them — what ↑/↓ and F3 walk. */
  const flat = $derived(groups.flatMap((g) => g.hits));
  const report = $derived(store.report);

  const collapsed = new SvelteSet<string>();
  let sel = $state(0);
  /** Clamped where it is read, so a batch landing mid-scan never drags the selection. */
  const selected = $derived(flat.length ? Math.min(sel, flat.length - 1) : -1);
  const current = $derived(selected >= 0 ? flat[selected] : undefined);
  /** The row's identity — the preview keys off this, so a batch arriving does not re-read the file under a
   *  selection that has not moved. */
  const currentKey = $derived(current ? `${current.rel}:${current.range.start}` : '');
  const keyOf = (h: { rel: string; range: { start: number } }) => `${h.rel}:${h.range.start}`;

  /** Move the selection (wrapping), expanding the file it lands in. */
  export function move(delta: number) {
    const next = stepIndex(selected, delta, flat.length);
    if (next < 0) return;
    sel = next;
    const hit = flat[next];
    collapsed.delete(hit.file);
    queueMicrotask(() => document.querySelector(`[data-ssr-key="${CSS.escape(keyOf(hit))}"]`)?.scrollIntoView({ block: 'nearest' }));
  }

  export function openCurrent() {
    if (current) onOpen(current.file, current.line);
  }

  export const hasHits = () => flat.length > 0;

  // ── the hover preview ──────────────────────────────────────────────────────────
  //
  // Shown after a short rest of the pointer, or on demand for the selected row (Space / F2) — never computed
  // per row on the way past. The file is read when the card is about to appear and kept, so a second look at
  // a match of the same file costs nothing.
  const PEEK_DELAY_MS = 450;
  let peek = $state<{ hit: SsrHit; rect: DOMRect; text: string | null } | null>(null);
  let peekTimer: ReturnType<typeof setTimeout> | undefined;
  /** Bumped on every request, so a slow read cannot open a card the pointer has already left. */
  let peekSeq = 0;

  async function textOf(file: string): Promise<string | null> {
    const known = fileCache.get(file);
    if (known !== undefined) return known;
    try {
      const text = (await readFile(root, file)).text;
      fileCache.set(file, text);
      return text;
    } catch {
      return null;
    }
  }

  async function openPeek(hit: SsrHit, el: Element) {
    const mine = ++peekSeq;
    const rect = el.getBoundingClientRect();
    const text = await textOf(hit.file);
    if (mine !== peekSeq) return;
    peek = { hit, rect, text };
  }

  function closePeek() {
    clearTimeout(peekTimer);
    peekSeq++;
    peek = null;
  }

  function restOn(hit: SsrHit, el: Element) {
    clearTimeout(peekTimer);
    peekTimer = setTimeout(() => void openPeek(hit, el), PEEK_DELAY_MS);
  }

  export const peekOpen = () => peek !== null;
  export function dismissPeek() { closePeek(); }

  /** Space / F2 on the selected match: show its preview, or hide it. */
  export function togglePeek() {
    if (peek) { closePeek(); return; }
    const hit = current;
    if (!hit) return;
    const el = document.querySelector(`[data-ssr-key="${CSS.escape(keyOf(hit))}"]`);
    if (el) void openPeek(hit, el);
  }

  // A list that changed under the card (a new search, a selection that moved on) leaves it describing nothing.
  $effect(() => {
    void store.hits;
    untrack(closePeek);
  });
  $effect(() => () => clearTimeout(peekTimer));

  // ── the selected match, and the file around it ───────────────────────────────
  const fileCache = new Map<string, string>();
  let previewText = $state('');
  let previewOf = $state('');

  $effect(() => {
    void currentKey;
    const hit = untrack(() => current);
    if (!hit || !root) { previewText = ''; previewOf = ''; return; }
    let live = true;
    void (async () => {
      let text = fileCache.get(hit.file);
      if (text === undefined) {
        try {
          text = (await readFile(root, hit.file)).text;
          fileCache.set(hit.file, text);
        } catch {
          if (live) { previewText = ''; previewOf = hit.rel; }
          return;
        }
      }
      if (!live) return;
      previewOf = hit.rel;
      previewText = text;
    })();
    return () => { live = false; };
  });
</script>

{#if report?.groupedBy}
  <!-- Grouped: the table. -->
  <div class="rs-table-wrap">
    <table class="rs-table">
      <thead><tr><th>{report.groupedBy}</th><th>Count</th><th>Files</th></tr></thead>
      <tbody>
        {#each report.rows as row (row.key)}
          <tr>
            <td class="rs-key">{row.key}</td>
            <td class="rs-num">
              {row.count}
              {#if row.unresolved}<span class="rs-unres-n" use:tooltip={'The classpath could not decide a type constraint for these'}>({row.unresolved}?)</span>{/if}
            </td>
            <td class="rs-num">{row.files}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{:else}
  <div class="rs-split">
    <div class="rs-list" role="listbox" aria-label="Matches" onscroll={closePeek}>
      {#each groups as group (group.file)}
        <button class="rs-file" type="button" onclick={() => (collapsed.has(group.file) ? collapsed.delete(group.file) : collapsed.add(group.file))}>
          {#if collapsed.has(group.file)}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
          <FileCode2 size={12} />
          <span class="rs-file-name" use:tooltip={group.rel}>{group.rel}</span>
          <Badge variant="tone" tone="neutral" size="sm" label={String(group.hits.length)} />
        </button>
        {#if !collapsed.has(group.file)}
          {#each group.hits as hit (keyOf(hit))}
            <button
              class="rs-hit"
              class:rs-on={current && keyOf(hit) === currentKey}
              data-ssr-key={keyOf(hit)}
              type="button"
              role="option"
              aria-selected={!!current && keyOf(hit) === currentKey}
              onmousemove={() => { const i = flat.indexOf(hit); if (i >= 0) sel = i; }}
              onmouseenter={(e) => restOn(hit, e.currentTarget)}
              onmouseleave={closePeek}
              onclick={() => { closePeek(); onOpen(hit.file, hit.line); }}
            >
              <span class="rs-line">{hit.line}</span>
              <span class="rs-code"><mark class="rs-match">{hit.preview}</mark></span>
              {#if hit.enclosing}<span class="rs-in">in {hit.enclosing}</span>{/if}
              {#if hit.unresolved}
                <span class="rs-warn" use:tooltip={'A type constraint here could not be decided — the classpath does not reach that far'}>
                  <TriangleAlert size={11} />
                </span>
              {/if}
            </button>
          {/each}
        {/if}
      {/each}
    </div>
    <div class="rs-preview">
      {#if previewText && current}
        <div class="rs-pv-head" use:tooltip={previewOf}>{previewOf}</div>
        <div class="rs-pv-body">
          <!-- The matched bytes are marked, not just the line: a structural pattern can match a fragment of a
               long statement, and banding the whole line would say less than the query did. -->
          <CodePreview
            text={previewText}
            language={languageForPath(current.file)}
            activeLine={current.line}
            markBytes={current.range}
          />
        </div>
      {:else}
        <p class="rs-pv-note">{previewOf ? 'This file can’t be read.' : 'Reading…'}</p>
      {/if}
    </div>
  </div>
{/if}

{#if peek}
  <HoverCard anchor={peek.rect} ariaLabel={store.replacing ? 'What this match becomes' : 'The code around this match'}>
    <SsrHitCard hit={peek.hit} text={peek.text} />
  </HoverCard>
{/if}

<style>
  .rs-split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 5fr) minmax(0, 4fr); }
  .rs-list { min-height: 0; overflow: auto; border-right: 1px solid var(--border-subtle); }
  .rs-file {
    display: flex; align-items: center; gap: 6px; width: 100%;
    position: sticky; top: 0; z-index: 1;
    padding: 4px 10px; text-align: left; cursor: pointer;
    background: var(--bg-elevated); border: none; border-bottom: 1px solid var(--border-subtle);
    color: var(--text-secondary); font-size: var(--font-size-xs);
  }
  .rs-file:hover { background: var(--bg-hover); }
  .rs-file-name {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-2xs); direction: rtl; text-align: left;
  }
  .rs-hit {
    display: flex; align-items: baseline; gap: 8px; width: 100%;
    padding: 3px 10px 3px 26px; text-align: left; cursor: pointer;
    background: none; border: none;
  }
  .rs-hit:hover { background: var(--bg-hover); }
  .rs-hit.rs-on { background: var(--bg-selected); }
  .rs-line {
    flex-shrink: 0; min-width: 34px; text-align: right;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .rs-code {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary);
  }
  /* The match, underlined in the accent — what the query matched, not the line around it. */
  .rs-match {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: inherit; border-radius: 2px;
    text-decoration: underline; text-decoration-color: var(--accent); text-underline-offset: 2px;
  }
  .rs-in { flex-shrink: 0; font-size: var(--font-size-3xs); color: var(--text-disabled); }
  .rs-warn { flex-shrink: 0; display: inline-flex; color: var(--warning); cursor: help; }

  .rs-preview { min-height: 0; display: flex; flex-direction: column; background: var(--bg-base); }
  .rs-pv-head {
    flex-shrink: 0; padding: 5px 10px; border-bottom: 1px solid var(--border-subtle);
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  .rs-pv-body { flex: 1; min-height: 0; }
  .rs-pv-note { padding: 12px; font-size: var(--font-size-xs); color: var(--text-disabled); }

  .rs-table-wrap { flex: 1; min-height: 0; overflow: auto; }
  .rs-table { width: 100%; border-collapse: collapse; font-size: var(--font-size-xs); }
  .rs-table th {
    position: sticky; top: 0; padding: 5px 10px; text-align: left;
    background: var(--bg-elevated); color: var(--text-muted);
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.05em;
    border-bottom: 1px solid var(--border-subtle);
  }
  .rs-table td { padding: 4px 10px; border-bottom: 1px solid var(--border-subtle); }
  .rs-table tbody tr:hover { background: var(--bg-hover); }
  .rs-key { font-family: var(--font-code); color: var(--text-primary); }
  .rs-num { text-align: right; font-family: var(--font-code); color: var(--text-secondary); width: 80px; }
  .rs-unres-n { color: var(--warning); margin-left: 4px; cursor: help; }
</style>
