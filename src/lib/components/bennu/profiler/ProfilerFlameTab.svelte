<script lang="ts">
  /**
   * The flame graph of the open recording. Click a box to open its source; double-click zooms;
   * the search dims everything that does not match, the class filter takes the other
   * frames out of every stack (the root keeps the whole recording, so what it hides is its remainder); the thread picker narrows
   * to one thread.
   *
   * **What the boxes weigh** is a choice: CPU time (where Java code runs), time **waiting** off the CPU (a query
   * waiting for the database is here, and nowhere in the CPU graph), or bytes **allocated**.
   *
   * With **Heat** on (the default, CPU only) a box is coloured by how much of the run it spent itself, on the
   * same scale as the tables, and everything unremarkable is a neutral grey — so what is hot is
   * what you see. Off, boxes take a colour per package, which is better for telling code apart.
   */
  import { Flame } from 'lucide-svelte';
  import FlameGraph, { type FlameNodeData } from '$lib/components/shared/ui/FlameGraph.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import type { FlameKind, FlameWire } from '$lib/ipc/bennu/profiler';
  import ProfilerFilterBar from './ProfilerFilterBar.svelte';
  import ProfilerLegend from './ProfilerLegend.svelte';
  import { filterFlame, hiddenShare } from './profiler-flame';
  import { fmtBytes, fmtMs } from './profiler-format';
  import { severityColor, severityOf } from './profiler-filter';

  let selected = $state<FlameNodeData | null>(null);
  let heat = $state(true);

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

  const full = $derived(store.flame ? toNode(store.flame) : null);
  /** With a class filter on, the graph is rebuilt from the stacks that pass it (see `profiler-flame`); off, it is the recording as is. */
  const root = $derived.by(() => (full && store.filterActive ? (filterFlame(full, (cls) => store.keep(cls)) as FlameNodeData) : full));
  const hidden = $derived(root && store.filterActive ? Math.round(hiddenShare(root)) : 0);
  const msPer = $derived(store.activeMeta?.ms_per_unit ?? 1);
  const kind = $derived(store.flameKind);
  const view = $derived(store.view);
  /** A box's weight in words: milliseconds of CPU samples, milliseconds blocked, or bytes. */
  const format = $derived.by(() => (kind === 'alloc' ? (v: number) => fmtBytes(v) : kind === 'wait' ? (v: number) => fmtMs(v) : (v: number) => fmtMs(v * msPer)));
  const kinds = $derived([
    { id: 'cpu', label: 'CPU', title: 'Where Java code runs — samples of running threads' },
    { id: 'wait', label: 'Waiting', disabled: !view?.has_waits, title: view?.has_waits ? 'Time blocked off the CPU: sockets, files, parking' : 'This recording has no wait events — record again' },
    { id: 'alloc', label: 'Allocations', disabled: !view?.has_alloc_stacks, title: view?.has_alloc_stacks ? 'Bytes allocated, by who asked for them' : 'This recording has no allocation stacks — record again' },
  ]);
  const threads = $derived(store.view?.threads ?? []);
  const options = $derived([
    { value: '', label: 'All threads' },
    ...threads.map((t) => ({ value: t.name, label: `${t.name} (${fmtMs(t.weight * msPer)})` })),
  ]);

  type Meta = { kind: string; class: string; method: string; file: string | null; line: number | null };

  function activate(n: FlameNodeData) {
    const m = n.meta as Meta | undefined;
    if (!m || (m.kind !== 'method' && m.kind !== 'native') || (!m.class && !m.file)) return;
    void store.goTo({ class: m.class, method: m.method, arity: null, file: m.file, decl_line: m.line });
  }

  const NEUTRAL = 'hsl(215 12% 42%)';
  const total = $derived(Math.max(1, root?.value ?? 1));

  const color = $derived.by(() => {
    const levels = store.thresholds.cpu_self_pct;
    return (n: FlameNodeData): string | undefined => {
      const k = (n.meta as Meta | undefined)?.kind;
      if (k === 'thread') return 'hsl(210 55% 48%)';
      // Heat is a statement about CPU time; the other weights have their own scales (see the tables).
      if (!heat || kind !== 'cpu') return k === 'native' ? 'hsl(0 0% 52%)' : undefined;
      return severityColor(severityOf(((n.self ?? 0) * 100) / total, levels)) ?? NEUTRAL;
    };
  });
</script>

<div class="flame-tab">
  <ProfilerFilterBar searchPlaceholder="Highlight methods…">
    <Tabs
      size="sm"
      variant="pill"
      value={kind}
      ariaLabel="What the boxes weigh"
      items={kinds}
      onSelect={(id) => void store.setFlameKind(id as FlameKind)}
    />
    <Select
      size="sm"
      value={store.thread ?? ''}
      {options}
      disabled={threads.length < 2}
      onchange={(v) => void store.setThread(v || null)}
    />
    <ToggleButton
      pressed={heat && kind === 'cpu'}
      icon={Flame}
      title={heat ? 'Colour by package instead of by heat' : 'Colour boxes by how much time they spent themselves'}
      onclick={() => (heat = !heat)}
    />
    {#if hidden > 0}<span class="hidden-note" title="The filter removes frames from every stack; the root keeps the whole recording">{hidden}% of the recording is outside the filter</span>{/if}
    {#if heat && kind === 'cpu'}<span class="legend"><ProfilerLegend levels={store.thresholds.cpu_self_pct} label="Self time" /></span>{/if}
  </ProfilerFilterBar>
  {#if root}
    <FlameGraph
      {root}
      search={store.query}
      bind:selected
      {color}
      onActivate={activate}
      {format}
      ariaLabel="Flame graph of the recording"
    />
  {/if}
</div>

<style>
  .flame-tab { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .legend { margin-left: auto; }
  .hidden-note { font-size: var(--font-size-xs); color: var(--text-muted); white-space: nowrap; }
</style>
