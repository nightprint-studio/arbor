<script lang="ts">
  /**
   * The call stack of the suspended thread — the leftmost column of the console while the
   * program is stopped.
   *
   * Clicking a frame **selects** it, and selecting is what opens it: the source, the variables
   * beside it and the watches all follow the selected frame, from one place (see
   * `BennuWindow`'s navigation effect). This list does not navigate — otherwise landing on a
   * frame would be done twice, once by the click and once by the stop that caused it.
   *
   * Library frames are drawn muted, and — with the ⊟ toggle on — **runs of them collapse into
   * one row**. A stop inside Spring is forty frames of framework around three of yours, and
   * scrolling past `ReflectiveMethodInvocation` twelve times to find your own is the whole
   * reason IntelliJ grew this. Expanding a run is one click, and the collapsed row says how
   * many it is holding, because "the frames between these two" is sometimes the answer.
   */
  import { ChevronRight, ListCollapse, PanelLeftClose } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuDebugStore } from '$lib/stores/bennu/debug.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { bennuDebugLayout } from './debug-layout.svelte';
  import type { StackFrameDto } from '$lib/types/bennu/debug';

  const frames = $derived(bennuDebugStore.frames);
  const detail = $derived(bennuDebugStore.stopped?.detail);
  const originLabel = $derived(
    detail?.origin === 'residual'
      ? 'Leftover stop: hit while another was open'
      : detail?.origin === 'step'
        ? 'A step finished'
        : detail?.origin === 'fresh'
          ? 'New breakpoint'
          : '',
  );
  const collapsing = $derived(bennuSettingsStore.collapseLibraryFrames);

  /** One rendered row: a frame, or a run of consecutive library frames folded into one. */
  type Row =
    | { kind: 'frame'; frame: StackFrameDto }
    | { kind: 'run'; id: number; frames: StackFrameDto[] };

  /** Runs the reader has opened, keyed by the index of their first frame. Reset on every stop —
   *  the stack is a different one, and so are the runs. */
  let opened = $state<Set<number>>(new Set());
  let lastStop: string | null = null;
  $effect(() => {
    const stamp = `${bennuDebugStore.sessionId}:${frames.length}:${frames[0]?.line ?? ''}`;
    if (stamp !== lastStop) {
      lastStop = stamp;
      opened = new Set();
    }
  });

  const rows = $derived.by<Row[]>(() => {
    if (!collapsing) return frames.map((frame) => ({ kind: 'frame', frame }));
    const out: Row[] = [];
    let run: StackFrameDto[] = [];
    const flush = () => {
      if (!run.length) return;
      // A single library frame is not worth a fold — the row that hides it is the same height.
      if (run.length === 1) out.push({ kind: 'frame', frame: run[0] });
      else out.push({ kind: 'run', id: run[0].index, frames: run });
      run = [];
    };
    for (const frame of frames) {
      if (frame.project) {
        flush();
        out.push({ kind: 'frame', frame });
      } else {
        run.push(frame);
      }
    }
    flush();
    return out;
  });

  function toggleRun(id: number) {
    const next = new Set(opened);
    if (!next.delete(id)) next.add(id);
    opened = next;
  }

  /** `com.acme.Order` → `Order`. The package is the same for most of a stack and buys nothing
   *  at this width; the full name is the row's tooltip. */
  function shortClass(fqcn: string): string {
    return fqcn.split('.').pop() ?? fqcn;
  }

  /**
   * A frame's two halves: what to put first, and what to put after it.
   *
   * A JVM frame splits into a method and its class. A **native** frame does not — it is one path,
   * `geode::mine::dig` — so the leaf is the part worth reading at this width and the rest is the
   * qualifier, exactly as a package is for Java. Splitting at the last `::` is safe *here* in a way it
   * is not in the backend: this is only deciding what to show first, and the whole string is in the
   * tooltip and in `lead + trail` either way.
   */
  function parts(frame: StackFrameDto): { lead: string; trail: string } {
    const line = frame.line ? `:${frame.line}` : '';
    if (!frame.name) {
      return { lead: frame.method, trail: `${shortClass(frame.class)}${line}` };
    }
    const at = frame.name.lastIndexOf('::');
    if (at < 0) return { lead: frame.name, trail: line };
    return { lead: frame.name.slice(at + 2), trail: `${frame.name.slice(0, at)}${line}` };
  }

  /** The whole truth, for the tooltip. */
  function fullName(frame: StackFrameDto): string {
    const line = frame.line ? `:${frame.line}` : '';
    if (frame.name) return `${frame.name}${line}`;
    return `${frame.class}.${frame.method}${line}`;
  }
</script>

{#snippet frameRow(frame: StackFrameDto, inRun: boolean)}
  <!-- Hoisted out of the button: `{@const}` is only legal as an immediate child of a block. -->
  {@const p = parts(frame)}
  <button
    class="df-frame"
    class:selected={frame.index === bennuDebugStore.selectedFrame}
    class:library={!frame.project}
    class:nested={inRun}
    type="button"
    use:tooltip={fullName(frame)}
    onclick={() => bennuDebugStore.selectFrame(frame.index)}
  >
    <span class="df-method">{p.lead}</span>
    <span class="df-class">{p.trail}</span>
  </button>
{/snippet}

<div class="df">
  <div class="df-title">
    Frames
    {#if bennuDebugStore.stopped?.thread_name}
      <span class="df-thread" use:tooltip={'The suspended thread'}>{bennuDebugStore.stopped.thread_name}</span>
    {/if}
    <!-- An explicit spacer rather than `margin-left: auto` on the first button: the thread
         badge is conditional, and two competing autos would split the gap instead of pushing
         the toggles flush right. -->
    <span class="df-gap"></span>
    <button
      class="df-toggle"
      class:on={collapsing}
      type="button"
      use:tooltip={collapsing ? 'Show every frame' : 'Fold runs of library frames'}
      aria-label="Fold runs of library frames"
      aria-pressed={collapsing}
      onclick={() => void bennuSettingsStore.setCollapseLibraryFrames(!collapsing)}
    >
      <ListCollapse size={12} />
    </button>
    <button
      class="df-toggle"
      type="button"
      use:tooltip={'Collapse this column'}
      aria-label="Collapse the frames column"
      onclick={() => bennuDebugLayout.toggleFrames()}
    >
      <PanelLeftClose size={12} />
    </button>
  </div>
  {#if detail && (originLabel || detail.condition || detail.returned || detail.also_stopped.length)}
    <!-- Why this stop and not another: what makes it new or leftover, the condition that let it
         through with what it read, what the last method returned, and the threads that wait. -->
    <div class="df-why">
      {#if originLabel}
        <span class="df-why-row" class:residual={detail.origin === 'residual'}>{originLabel}{detail.suspends === 'thread' ? ' · this thread only' : ''}</span>
      {/if}
      {#if detail.condition}
        <span class="df-why-row" use:tooltip={`Condition: ${detail.condition}`}>
          when <code>{detail.condition}</code>{detail.condition_values ? ' → ' : ''}<code>{detail.condition_values}</code>
        </span>
      {/if}
      {#if detail.returned}
        <span class="df-why-row">
          {#if detail.returned.method}
            ↩ <code>{detail.returned.method.split('.').slice(-2).join('.')}</code> → <code>{detail.returned.value}</code>
          {:else}
            ↩ {detail.returned.value}
          {/if}
        </span>
      {/if}
      {#each detail.also_stopped as other (other.thread)}
        <button
          class="df-other"
          type="button"
          use:tooltip={`Switch to this stopped thread (${other.reason})`}
          onclick={() => void bennuDebugStore.selectThread(other.thread)}
        >
          <span class="df-other-name">{other.thread_name}</span>
          <span class="df-other-at">{other.location}</span>
        </button>
      {/each}
    </div>
  {/if}
  <div class="df-scroll">
    {#each rows as row (row.kind === 'run' ? `run-${row.id}` : `f-${row.frame.index}`)}
      {#if row.kind === 'frame'}
        {@render frameRow(row.frame, false)}
      {:else}
        <button
          class="df-run"
          class:open={opened.has(row.id)}
          type="button"
          onclick={() => toggleRun(row.id)}
        >
          <span class="df-chev" class:open={opened.has(row.id)}><ChevronRight size={11} /></span>
          <span class="df-run-label">{row.frames.length} library frames</span>
          <span class="df-run-hint">{shortClass(row.frames[0].class)} …</span>
        </button>
        {#if opened.has(row.id)}
          {#each row.frames as frame (frame.index)}
            {@render frameRow(frame, true)}
          {/each}
        {/if}
      {/if}
    {/each}
  </div>
</div>

<style>
  /* The width belongs to the enclosing ResizablePanel; this only has to fill it. */
  .df {
    display: flex; flex-direction: column; height: 100%; min-height: 0; min-width: 0;
  }
  .df-title {
    display: flex; align-items: center; gap: 4px;
    padding: 3px 10px; flex-shrink: 0;
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
  }
  .df-gap { flex: 1 1 auto; min-width: 4px; }
  .df-thread {
    min-width: 0;
    text-transform: none; letter-spacing: 0;
    font-family: var(--font-code); font-size: 10px;
    padding: 0 5px; border-radius: var(--radius-sm);
    background: var(--bg-elevated);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .df-toggle {
    flex: 0 0 auto;
    display: inline-flex; align-items: center; justify-content: center;
    width: 18px; height: 16px; padding: 0;
    border: 0; border-radius: var(--radius-sm); background: none;
    color: var(--text-disabled); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .df-toggle:hover { background: var(--bg-hover); color: var(--text-primary); }
  .df-toggle.on { color: var(--accent); }

  .df-why {
    display: flex; flex-direction: column; gap: 2px; flex-shrink: 0;
    padding: 4px 10px; font-size: 11px; color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
  }
  .df-why-row { overflow-wrap: anywhere; }
  .df-why-row.residual { color: var(--warning); }
  .df-why code { font-family: var(--font-code); font-size: 10.5px; color: var(--text-primary); }
  .df-other {
    display: flex; align-items: baseline; gap: 6px; min-width: 0;
    padding: 1px 4px; border: 0; border-radius: var(--radius-sm); background: none;
    color: var(--text-secondary); font-size: 11px; text-align: left; cursor: pointer;
  }
  .df-other:hover { background: var(--bg-hover); color: var(--text-primary); }
  .df-other-name { font-family: var(--font-code); }
  .df-other-at { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .df-scroll { flex: 1; overflow: auto; min-height: 0; padding: 2px 0; }

  /* A folded run. Deliberately quieter than a frame: it is a thing you skip past, and it
     becomes interesting only when you go looking for what is inside it. */
  .df-run {
    display: flex; align-items: baseline; gap: 5px;
    width: 100%; padding: 2px 10px;
    border: 0; background: none; text-align: left; cursor: pointer;
    font-size: 11px; line-height: 1.6; color: var(--text-muted);
    white-space: nowrap; overflow: hidden;
  }
  .df-run:hover { background: var(--bg-hover); }
  .df-chev {
    display: inline-flex; align-self: center; flex: 0 0 auto;
    transition: transform var(--transition-fast);
  }
  .df-chev.open { transform: rotate(90deg); }
  .df-run-label { font-style: italic; }
  .df-run-hint {
    font-family: var(--font-code); font-size: 10.5px; color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis;
  }
  .df-frame.nested { padding-left: 22px; }

  .df-frame {
    display: flex; align-items: baseline; gap: 6px;
    width: 100%; padding: 2px 10px;
    border: 0; background: none; text-align: left; cursor: pointer;
    font-family: var(--font-code); font-size: 11.5px; line-height: 1.6;
    white-space: nowrap; overflow: hidden;
  }
  .df-frame:hover { background: var(--bg-hover); }
  .df-frame.selected { background: var(--bg-selected); }
  .df-method { color: var(--text-primary); }
  .df-class { color: var(--text-muted); font-size: 10.5px; }
  /* Muted, because a stop inside a framework is forty of these around three of yours. */
  .df-frame.library .df-method { color: var(--text-muted); }
  .df-frame.library .df-class { opacity: 0.7; }
</style>
