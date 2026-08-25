<script lang="ts" module>
  /**
   * Whether the legend shows when the window opens.
   *
   * Module-level, so it is remembered for the session rather than persisted: it starts **open**,
   * because the marks are conventions and a reader meeting them for the first time has no way to
   * decode them — and it stays closed once you have closed it, because by then you know. A setting on
   * disk would be the wrong weight for "I have read the legend".
   */
  let legendWanted = true;
</script>

<script lang="ts">
  /**
   * The **module graph** — which of the project's own crates (or Maven modules) depend on which.
   *
   * ## Why a window and not a panel
   *
   * It is *wide*: a twenty-crate workspace is six columns of boxes, and a 280px side panel would
   * reduce it to a list — which the Dependencies panel already is, and better. You come here with a
   * question ("who still uses this", "what does touching this rebuild", "why does cargo say there is
   * a cycle"), read the answer, and leave. That is a dialog.
   *
   * ## Three views of one graph, because they answer different questions
   *
   * The **list** is the keyboard surface and the index: sortable, filterable, and the only way to
   * find a crate by name in a picture of forty. The **drawing** is the shape — layers, fan-out, and
   * the cycles, which no list can show. The **detail panel** is the two things a box cannot hold: the
   * numbers, and the neighbours as rows you can walk into.
   *
   * Selecting is one act across all three: the list moves the drawing, the drawing moves the list,
   * and either fills the detail panel. Anything else and the reader has to keep track of which one is
   * "current".
   *
   * ## Layers read left to right
   *
   * Dependents on the left, the foundation on the right, so a chain reads like the sentence
   * describing it — and **every arrow in a healthy graph points right**. A leftward arrow is a cycle,
   * which is a property of the layout rather than a legend to consult (see `module-graph-layout.ts`).
   *
   * ## Two ways to narrow it, and they are not the same
   *
   * **Solo** filters — the selected module's world, everything else gone, columns recomputed. A
   * **search** dims — the matches stay lit and the rest recedes without moving. They answer different
   * questions ("show me this crate's world" against "where is the one called something-like-parser"),
   * and both can be on at once. There used to be a third, a dim-focus mode, which was solo's question
   * answered the search's way: two overlapping opacity systems that multiplied into a picture where
   * nothing was legible.
   *
   * Nothing here runs cargo or Maven; the graph is read from the manifests, so it opens on a project
   * that has never been built.
   */
  import { onMount, tick } from 'svelte';
  import {
    ArrowLeft, ArrowLeftRight, ArrowRight, BookOpen, Braces, Crosshair, Download, FileText,
    FlaskConical, Network, Sheet, TriangleAlert,
  } from 'lucide-svelte';
  import ZoomControls, { clampZoom } from '$lib/components/shared/ui/ZoomControls.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { moduleGraph, moduleWord, type GraphEdge, type ModuleGraph } from '$lib/ipc/bennu/deps';
  import { isKey } from '$lib/utils/keybindings';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { layoutGraph, neighbourhood } from './module-graph-layout';
  import GraphCanvas from './module-graph/GraphCanvas.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { writeFile } from '$lib/ipc/bennu';
  import {
    EXPORT_EXT, exportFilename, exportGraph, type ExportFormat,
  } from './module-graph/graph-export';
  import GraphLegend from './module-graph/GraphLegend.svelte';
  import GraphList, { type GraphSort } from './module-graph/GraphList.svelte';
  import GraphDetails from './module-graph/GraphDetails.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const root = $derived(projectStore.project?.root ?? '');

  let graph = $state<ModuleGraph | null>(null);
  let loading = $state(true);
  let failed = $state(false);

  let query = $state('');
  let selected = $state<number | null>(null);

  /**
   * **Solo**: draw only the selected module's world, and nothing else.
   *
   * A filter and not a dimming, which is the whole point — in a workspace of sixty crates, the answer
   * to "show me everything about this one" has to make the other fifty *stop taking up room*, so the
   * columns are recomputed from what is left. Dimming leaves the picture exactly as unreadable as it
   * was, with most of it greyed.
   *
   * Solo follows the selection: picking another module re-solos around it, which is how you walk a
   * dependency chain one crate at a time without leaving the mode.
   */
  let solo = $state(false);
  /** Which half of the world solo keeps. Both directions answer different questions, and one of them
   *  is usually the one you want: what am I built on, or what do I break. */
  let soloScope = $state<'both' | 'deps' | 'users'>('both');
  /** Whether edges that cannot order a build are drawn — Cargo's `dev`, Maven's `test`. */
  let showSoft = $state(true);
  let zoom = $state(1);
  let legendOpen = $state(legendWanted);
  let canvas = $state<{ reveal: (index: number) => void; fitZoom: () => number } | null>(null);

  const ZOOM_MIN = 0.4;
  const ZOOM_MAX = 2;
  const ZOOM_STEP = 0.2;

  onMount(() => {
    if (!root) {
      loading = false;
      return;
    }
    void moduleGraph(root)
      .then((g) => { graph = g; })
      .catch(() => { failed = true; })
      .finally(() => { loading = false; });
  });

  /** `crate` / `module`, so a Rust workspace is never told about its modules. */
  const word = $derived(moduleWord(graph?.ecosystem ?? ''));
  const words = $derived(moduleWord(graph?.ecosystem ?? '', true));
  /** What the ecosystem calls the edges that do not order a build. */
  const softWord = $derived(graph?.ecosystem === 'cargo' ? 'dev' : 'test');

  const edges = $derived<GraphEdge[]>(
    (graph?.edges ?? []).filter((e) => showSoft || e.structural),
  );

  /** The selection's transitive neighbourhood, both directions. */
  const around = $derived(
    graph && selected !== null ? neighbourhood(graph, selected, edges) : null,
  );

  const relatedSet = $derived(
    around ? new Set<number>([...around.dependencies, ...around.dependents]) : new Set<number>(),
  );

  /** The nodes solo keeps, or `null` for the whole graph. */
  const onlySet = $derived.by(() => {
    if (!solo || selected === null || !around) return null;
    const keep = new Set<number>([selected]);
    if (soloScope !== 'users') for (const i of around.dependencies) keep.add(i);
    if (soloScope !== 'deps') for (const i of around.dependents) keep.add(i);
    return keep;
  });

  const layout = $derived(
    graph
      ? layoutGraph(graph, edges, { only: onlySet })
      : { nodes: [], edges: [], columns: [], width: 0, height: 0 },
  );

  const needle = $derived(query.trim().toLowerCase());

  const matchSet = $derived.by(() => {
    if (!needle || !graph) return new Set<number>();
    const out = new Set<number>();
    graph.nodes.forEach((n, i) => {
      if (`${n.id} ${n.name} ${n.kind}`.toLowerCase().includes(needle)) out.add(i);
    });
    return out;
  });

  /**
   * What stays at full strength while a search is running.
   *
   * Dimming is the *search's* mechanism and only the search's. It used to double as a focus mode, and
   * having two ways to say "this part matters" — one that greys and one that filters — meant they
   * could be on at once and multiply into a picture where nothing was legible. Solo filters; a search
   * dims. Both can be on: then the search dims inside the soloed world, which is what you would want.
   */
  const highlight = $derived(needle ? matchSet : new Set<number>());
  const dimOthers = $derived(highlight.size > 0);

  // ── The list ───────────────────────────────────────────────────────────────────
  // Ordered here rather than in `GraphList` because the sort keys are facts about the graph, which
  // this component owns; the list owns the box you type in and the rows you walk.
  let sort = $state<GraphSort>('impact');

  /** Row order. Every tie breaks on the name, so the list never reshuffles between renders. */
  const rows = $derived.by(() => {
    if (!graph) return [];
    const all = graph.nodes.map((node, index) => ({ index, node }));
    const shown = needle ? all.filter((r) => matchSet.has(r.index)) : all;
    type Row = { index: number; node: (typeof all)[number]['node'] };
    const byName = (a: Row, b: Row) =>
      (a.node.name || a.node.id).localeCompare(b.node.name || b.node.id);
    return [...shown].sort((a, b) => {
      switch (sort) {
        case 'impact':
          return b.node.impact - a.node.impact || byName(a, b);
        case 'external':
          return b.node.external - a.node.external || byName(a, b);
        // Deepest first, matching the drawing's leftmost-is-most-dependent order.
        case 'layer':
          return b.node.layer - a.node.layer || byName(a, b);
        default:
          return byName(a, b);
      }
    });
  });

  const selectedNode = $derived(
    graph && selected !== null ? graph.nodes[selected] ?? null : null,
  );

  /** Resolve a list of node indices into rows the detail panel can render. */
  function resolve(indices: number[]) {
    if (!graph) return [];
    return indices.map((index) => ({ index, node: graph!.nodes[index] })).filter((r) => r.node);
  }

  /**
   * Select, then scroll to it.
   *
   * The `tick` is load-bearing in solo mode: choosing another module rebuilds the whole layout around
   * it, and revealing before that has happened scrolls to where the node *used to* be.
   */
  async function pick(index: number) {
    selected = index;
    await tick();
    canvas?.reveal(index);
  }

  /** Open the module's manifest and close — the graph answered, the file is where you act. */
  async function openManifest(index: number) {
    const node = graph?.nodes[index];
    if (!node?.manifest) return;
    onClose();
    await projectStore.openFile(node.manifest).catch(() => {});
  }

  /** Show the flat dependency list instead. Toggling would *close* an already-open panel, which is
   *  the opposite of what the label promises. */
  function openDependencies() {
    onClose();
    if (bennuUiStore.leftPanel !== 'dependencies') bennuUiStore.toggleLeft('dependencies');
  }

  /**
   * Alt+S toggles solo — the one control here worth a key.
   *
   * `isKey` rather than a comparison against `e.key`: macOS composes Option+S into `ß`, which is the
   * trap that silently unbound Bennu's whole Alt family until it was fixed. Alt-modified so it cannot
   * fire while the search box has the caret, which it does the moment this window opens.
   */
  function onWindowKey(e: KeyboardEvent) {
    if (!e.altKey || e.ctrlKey || e.metaKey || !isKey(e, 's')) return;
    if (selected === null) return;
    e.preventDefault();
    solo = !solo;
  }

  // ── Export ─────────────────────────────────────────────────────────────────────
  /** Which format the save picker is open for, or null. */
  let savingAs = $state<ExportFormat | null>(null);

  /** What the export describes: the filters, so the file can say what it is a picture of. */
  function exportScope() {
    return {
      project: projectStore.project?.name ?? '',
      only: onlySet,
      includesSoft: showSoft,
      soloScope,
      soloOf: selectedNode?.id,
    };
  }

  /** Render the drawn graph. Never throws — the formats are pure string building. */
  function rendered(format: ExportFormat): string {
    return graph ? exportGraph(format, graph, edges, exportScope()) : '';
  }

  /**
   * Copy it.
   *
   * The primary destination on purpose: the stated use for this is pasting the structure into a chat
   * with a model, and a file on disk is a detour on the way there. Clipboard access can be refused
   * (focus, permissions), so the failure says so instead of looking like nothing happened.
   */
  function copyAs(format: ExportFormat) {
    const text = rendered(format);
    if (!text) return;
    void navigator.clipboard
      ?.writeText(text)
      .then(() => toastStore.show(`Graph copied as ${EXPORT_EXT[format].toUpperCase()}`, 'success'))
      .catch(() => toastStore.show('Could not reach the clipboard', 'warning'));
  }

  /** Write it where the picker says. Arbor's own picker, never a native dialog. */
  async function saveTo(path: string) {
    const format = savingAs;
    savingAs = null;
    if (!format) return;
    const root = projectStore.project?.root ?? '';
    try {
      await writeFile(root, path, rendered(format));
      toastStore.show(`Graph written to ${path}`, 'success');
    } catch (e) {
      toastStore.show(`Couldn't write the export: ${e}`, 'error');
    }
  }

  /**
   * The export menu.
   *
   * Copy first and Markdown first inside it, because that is the path the feature exists for. The
   * parenthetical on each row says who the format is *for* — three file extensions with no hint of
   * which one to hand a model is a menu you have to think about every time.
   */
  const exportItems = $derived<DropdownItem[]>([
    { kind: 'separator' as const, label: 'Copy' },
    { kind: 'item' as const, id: 'copy-md', label: 'Markdown — for an AI', icon: FileText,
      onclick: () => copyAs('markdown') },
    { kind: 'item' as const, id: 'copy-json', label: 'JSON — every field', icon: Braces,
      onclick: () => copyAs('json') },
    { kind: 'item' as const, id: 'copy-csv', label: 'CSV — one row per edge', icon: Sheet,
      onclick: () => copyAs('csv') },
    { kind: 'separator' as const, label: 'Save to a file' },
    { kind: 'item' as const, id: 'save-md', label: 'Markdown…', icon: FileText,
      onclick: () => (savingAs = 'markdown') },
    { kind: 'item' as const, id: 'save-json', label: 'JSON…', icon: Braces,
      onclick: () => (savingAs = 'json') },
    { kind: 'item' as const, id: 'save-csv', label: 'CSV…', icon: Sheet,
      onclick: () => (savingAs = 'csv') },
  ]);

  /** Show or hide the legend, and remember the answer for the next time the window opens. */
  function toggleLegend() {
    legendOpen = !legendOpen;
    legendWanted = legendOpen;
  }

  /** Zoom so the whole drawing fits. The canvas measures; this owns the number. */
  function fit() {
    const next = canvas?.fitZoom();
    if (next) zoom = next;
  }

  /** Jump to the first cycle — the reason the warning in the header is clickable. */
  function goToCycle() {
    const first = graph?.cycles[0]?.[0];
    if (first === undefined) return;
    pick(first);
  }

  const cycleCount = $derived(graph?.cycles.length ?? 0);
  const inCycle = $derived(graph?.cycles.reduce((n, c) => n + c.length, 0) ?? 0);
</script>

<svelte:window onkeydown={onWindowKey} />

<Modal
  {onClose}
  width="min(1320px, 96vw)"
  height="min(840px, 92vh)"
  padBody={false}
  ariaLabel="Module graph"
>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Network size={14} />
      <span class="modal-title">{graph?.ecosystem === 'cargo' ? 'Crate graph' : 'Module graph'}</span>
      {#if graph}
        <!-- What is on screen, not what exists: with solo on or the dev edges hidden, quoting the
             project's totals would describe a picture the reader is not looking at. -->
        <span class="mg-counts">
          {#if onlySet}{onlySet.size} of {graph.nodes.length}{:else}{graph.nodes.length}{/if}
          {words} · {layout.edges.length} edges · {layout.columns.length} layers
        </span>
        {#if cycleCount}
          <!-- Not a badge: it is the one thing in here that means something is broken, and pressing
               it goes to the ring. -->
          <button class="mg-cycles" type="button" onclick={goToCycle}
            use:tooltip={'Go to the first cycle'}>
            <TriangleAlert size={11} />
            {cycleCount === 1 ? '1 cycle' : `${cycleCount} cycles`} · {inCycle} {words}
          </button>
        {/if}
        {#if graph.truncated}
          <span class="mg-trunc" use:tooltip={'Only the first 400 modules are drawn'}>truncated</span>
        {/if}
      {/if}
      <!-- `actions` puts this immediately left of the ✕ — the header's own slot for it, so the export
           sits with the window's chrome rather than among the view controls in the footer. It is the
           one thing here that produces something you take away.
           Exports **what is on screen**, filters included, and says so in the file's own header: an
           export that silently described the whole project while the window showed one crate's
           neighbourhood would mislead whoever, or whatever, reads it. -->
      {#snippet actions()}
        <Dropdown items={exportItems} position="fixed" direction="down" width="240px">
          {#snippet trigger({ toggle })}
            <button
              class="mg-export"
              type="button"
              aria-label="Export the graph"
              disabled={!graph?.nodes.length}
              use:tooltip={'Copy or save this graph'}
              onclick={toggle}
            >
              <Download size={13} />
            </button>
          {/snippet}
        </Dropdown>
      {/snippet}
    </ModalHeader>
  {/snippet}

  {#if loading}
    <div class="mg-mid"><Spinner size={16} /><span>Reading the manifests…</span></div>
  {:else if failed}
    <EmptyState message="The backend could not read this project's manifests." />
  {:else if !graph || !graph.nodes.length}
    <EmptyState
      message={root
        ? 'This project declares no modules to graph.'
        : 'Open a project to see its module graph.'}
    />
  {:else}
    <div class="mg">
      <aside class="mg-side">
        <GraphList
          {rows}
          {selected}
          bind:query
          bind:sort
          {words}
          onPick={pick}
          onOpen={(i) => void openManifest(i)}
        />

        {#if selectedNode && selected !== null && around}
          <GraphDetails
            node={selectedNode}
            ecosystem={graph.ecosystem}
            dependencies={resolve(around.directDependencies)}
            dependents={resolve(around.directDependents)}
            onPick={pick}
            onOpenManifest={() => void openManifest(selected!)}
          />
        {/if}
      </aside>

      <GraphCanvas
        bind:this={canvas}
        {layout}
        {selected}
        related={relatedSet}
        {highlight}
        {dimOthers}
        {zoom}
        onSelect={(i) => (selected = i)}
        onOpen={(i) => void openManifest(i)}
        onZoom={(by) => (zoom = clampZoom(zoom, by, { min: ZOOM_MIN, max: ZOOM_MAX }))}
      />

      {#if legendOpen}
        <GraphLegend ecosystem={graph.ecosystem} onClose={toggleLegend} />
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    <ModalFooter>
      <!-- One convention inline and the rest behind a button. The arrow's direction stays on screen
           because reading it backwards inverts every conclusion the window supports; the line styles
           need a drawn sample to be decodable at all, which is what the legend panel is for. -->
      <button
        class="mg-toggle"
        class:on={legendOpen}
        type="button"
        aria-pressed={legendOpen}
        use:tooltip={'What the lines and colours mean'}
        onclick={toggleLegend}
      >
        <BookOpen size={12} />legend
      </button>
      <span class="mg-key">A → B: A depends on B</span>
      <span class="mg-spacer"></span>
      {#if graph?.nodes.length}
        <button
          class="mg-toggle"
          class:on={showSoft}
          type="button"
          aria-pressed={showSoft}
          use:tooltip={`Show ${softWord} dependencies`}
          onclick={() => (showSoft = !showSoft)}
        >
          <FlaskConical size={12} />{softWord}
        </button>
        <!-- Solo. Disabled with nothing selected, because "everything about *which* crate" has no
             answer then — and the tooltip says so rather than leaving a dead button. -->
        <button
          class="mg-toggle"
          class:on={solo}
          type="button"
          aria-pressed={solo}
          disabled={selected === null}
          use:tooltip={selected === null
            ? 'Pick a ' + word + ' to isolate it'
            : `Show only ${selectedNode?.name ?? 'this'} and its world (Alt+S)`}
          onclick={() => (solo = !solo)}
        >
          <Crosshair size={12} />solo
        </button>
        {#if solo}
          <!-- Which half of the world. Only while solo is on: three buttons for a mode that is off is
               three controls that do nothing. -->
          <span class="mg-seg">
            <button
              class:on={soloScope === 'both'}
              type="button"
              use:tooltip={'Everything connected to it'}
              onclick={() => (soloScope = 'both')}
            ><ArrowLeftRight size={11} /></button>
            <button
              class:on={soloScope === 'deps'}
              type="button"
              use:tooltip={'What it is built on'}
              onclick={() => (soloScope = 'deps')}
            ><ArrowRight size={11} /></button>
            <button
              class:on={soloScope === 'users'}
              type="button"
              use:tooltip={'What it would break'}
              onclick={() => (soloScope = 'users')}
            ><ArrowLeft size={11} /></button>
          </span>
        {/if}
        <ZoomControls
          value={zoom}
          min={ZOOM_MIN}
          max={ZOOM_MAX}
          step={ZOOM_STEP}
          onFit={fit}
          fitLabel="Fit the whole graph"
          ariaLabel="Zoom the module graph"
          onChange={(next) => (zoom = next)}
        />
      {/if}
      <!-- The other window about dependencies — the flat, per-module list with versions and their
           origins. Closes this one on the way: leaving a panel open behind a modal is not an action a
           reader can see happen. -->
      <!-- No Close button down here: the header's ✕ is the way out, and a second one at the other end
           of the window is a second thing to find. Esc closes it too, like every other modal. -->
      <Button variant="ghost" onclick={openDependencies}>Dependencies…</Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if savingAs}
  <!-- Arbor's own picker in `save` mode — never the native dialog, and never `<input type="file">`. -->
  <FileExplorerModal
    mode="save"
    title={`Save the graph as ${EXPORT_EXT[savingAs].toUpperCase()}`}
    extensions={[EXPORT_EXT[savingAs]]}
    initialPath={projectStore.project?.root ?? ''}
    initialFilename={exportFilename(savingAs, projectStore.project?.name ?? '', !!onlySet)}
    onConfirm={(path: string) => void saveTo(path)}
    onCancel={() => (savingAs = null)}
    onClose={() => (savingAs = null)}
  />
{/if}

<style>
  .mg-counts {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .mg-cycles {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 1px 6px; border: 1px solid var(--error); border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--error) 14%, transparent);
    color: var(--error); cursor: pointer;
    font-size: var(--font-size-3xs);
  }
  .mg-cycles:hover { background: color-mix(in srgb, var(--error) 24%, transparent); }
  .mg-trunc {
    padding: 0 5px; border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    color: var(--warning); font-size: var(--font-size-3xs); text-transform: uppercase;
  }

  /* Sized like the header's minimize / close buttons so the chrome stays balanced, and quiet at rest
     so it does not compete with the ✕ next to it. */
  .mg-export {
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; padding: 0;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .mg-export:hover:not(:disabled) { background: rgb(255 255 255 / 8%); color: var(--text-primary); }
  .mg-export:disabled { opacity: 0.35; cursor: default; }

  .mg-mid {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; color: var(--text-disabled); font-size: var(--font-size-xs);
  }

  /* `relative` for the legend card, which floats over the drawing's bottom-right corner. */
  .mg { position: relative; display: flex; height: 100%; min-height: 0; }
  .mg-side {
    width: 288px; flex-shrink: 0;
    display: flex; flex-direction: column; min-height: 0;
    border-right: 1px solid var(--border-subtle);
  }
  /* ── Footer ── */
  .mg-key { font-size: var(--font-size-3xs); color: var(--text-disabled); }
  .mg-spacer { flex: 1; }

  .mg-toggle {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 7px; border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    background: none; color: var(--text-muted); cursor: pointer;
    font-size: var(--font-size-3xs); font-family: var(--font-code);
  }
  .mg-toggle:hover:not(:disabled) { color: var(--text-primary); background: var(--bg-hover); }
  .mg-toggle.on { color: var(--accent); border-color: var(--accent); }
  .mg-toggle:disabled { opacity: 0.4; cursor: default; }

  /* A segmented control, not three toggles: the three are exclusive, and buttons that look
     independent while behaving exclusively is the shape people click twice. */
  .mg-seg {
    display: inline-flex; align-items: center;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .mg-seg button {
    display: inline-flex; align-items: center; justify-content: center;
    height: 20px; padding: 0 6px;
    background: none; border: none; color: var(--text-muted); cursor: pointer;
  }
  .mg-seg button:hover { background: var(--bg-hover); color: var(--text-primary); }
  .mg-seg button.on { background: color-mix(in srgb, var(--accent) 22%, transparent); color: var(--accent); }

</style>
