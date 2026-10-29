<script lang="ts">
  /**
   * The flame graph of the open recording. Click a box to open its source; double-click zooms;
   * the search dims everything that does not match; the thread picker narrows to one thread.
   */
  import FlameGraph, { type FlameNodeData } from '$lib/components/shared/ui/FlameGraph.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import type { FlameWire } from '$lib/ipc/bennu/profiler';
  import { fmtMs } from './profiler-format';

  let query = $state('');
  let selected = $state<FlameNodeData | null>(null);

  /** The wire tree as the widget's generic nodes; what the click needs rides along in `meta`. */
  function toNode(w: FlameWire): FlameNodeData {
    const cls = w.class ?? '';
    const method = cls && w.name.startsWith(`${cls}.`) ? w.name.slice(cls.length + 1) : w.name;
    return {
      name: w.name,
      value: w.value,
      self: w.self,
      meta: { kind: w.kind, class: cls, method, file: w.file ?? null, line: w.line ?? null },
      children: w.children.map(toNode),
    };
  }

  const root = $derived(store.flame ? toNode(store.flame) : null);
  const msPer = $derived(store.activeMeta?.ms_per_unit ?? 1);
  const threads = $derived(store.view?.threads ?? []);
  const options = $derived([
    { value: '', label: 'All threads' },
    ...threads.map((t) => ({ value: t.name, label: `${t.name} (${fmtMs(t.weight * msPer)})` })),
  ]);

  function activate(n: FlameNodeData) {
    const m = n.meta as { kind: string; class: string; method: string; file: string | null; line: number | null } | undefined;
    if (!m || (m.kind !== 'method' && m.kind !== 'native') || !m.class && !m.file) return;
    void store.goTo({ class: m.class, method: m.method, arity: null, file: m.file, decl_line: m.line });
  }

  const color = (n: FlameNodeData): string | undefined => {
    const k = (n.meta as { kind?: string } | undefined)?.kind;
    return k === 'native' ? 'hsl(0 0% 52%)' : k === 'thread' ? 'hsl(210 55% 48%)' : undefined;
  };
</script>

<div class="flame-tab">
  <div class="bar">
    <div class="search"><SearchBar bind:query showRegex={false} showCounter={false} /></div>
    <Select
      size="sm"
      value={store.thread ?? ''}
      {options}
      disabled={threads.length < 2}
      onchange={(v) => void store.setThread(v || null)}
    />
  </div>
  {#if root}
    <FlameGraph
      {root}
      search={query}
      bind:selected
      {color}
      onActivate={activate}
      format={(v) => fmtMs(v * msPer)}
      ariaLabel="Flame graph of the recording"
    />
  {/if}
</div>

<style>
  .flame-tab { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .bar { display: flex; align-items: center; gap: 8px; padding: 4px 8px; }
  .search { flex: 1; min-width: 0; max-width: 360px; }
</style>
