<script lang="ts">
  /**
   * BennuEditorSplit — the editor area: one editor, or two side by side with a divider you drag.
   *
   * Each pane is a whole `BennuEditor` with its own tab strip, and the project store says what is open
   * in which (`editor-panes.ts`). The second pane exists only while it has tabs: drag a tab onto the
   * zone that appears at the right edge of a lone editor, or onto the other pane's strip, to move it;
   * close its last tab, or run *Close split*, to get one editor back. A file is open in one pane at a
   * time, so moving a tab moves the file.
   *
   * The divider is a separator in the IntelliJ sense — drag it, or focus it and use the arrow keys;
   * double-click returns it to the middle. The ratio lives here, for the life of the window: it is
   * layout, not a setting.
   *
   * The two editors are handed back (`first`, `second`) so the window can talk to whichever has the
   * focus.
   */
  import { PanelRight } from 'lucide-svelte';
  import BennuEditor from './BennuEditor.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { tabDragStore } from '$lib/stores/tab-drag.svelte';
  import type { GenerateMode } from './bennu-intentions';

  let {
    first = $bindable(),
    second = $bindable(),
    onGenerate,
    onOverride,
  }: {
    // The editors' own imperative APIs, which the window types for itself.
    first?: any;
    second?: any;
    onGenerate?: (mode: GenerateMode) => void;
    onOverride?: () => void;
  } = $props();

  const split = $derived(projectStore.isSplit);

  const MIN = 0.15;
  const STEP = 0.02;
  let ratio = $state(0.5);
  let host = $state<HTMLElement | null>(null);
  let dragging = $state(false);

  const clamp = (r: number) => Math.min(1 - MIN, Math.max(MIN, r));

  function startDrag(e: MouseEvent) {
    if (e.button !== 0 || !host) return;
    e.preventDefault();
    dragging = true;
    const rect = host.getBoundingClientRect();
    const move = (ev: MouseEvent) => { ratio = clamp((ev.clientX - rect.left) / rect.width); };
    const up = () => {
      dragging = false;
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  }

  function onDividerKey(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') { e.preventDefault(); ratio = clamp(ratio - STEP); }
    else if (e.key === 'ArrowRight') { e.preventDefault(); ratio = clamp(ratio + STEP); }
    else if (e.key === 'Home') { e.preventDefault(); ratio = MIN; }
    else if (e.key === 'End') { e.preventDefault(); ratio = 1 - MIN; }
  }
</script>

<div class="split" class:dragging bind:this={host}>
  <div class="pane" style:flex={split ? `0 0 ${ratio * 100}%` : '1 1 auto'}>
    <BennuEditor pane={0} bind:this={first} {onGenerate} {onOverride} />
  </div>

  {#if split}
    <div
      class="divider"
      class:active={dragging}
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize the editor panes"
      aria-valuenow={Math.round(ratio * 100)}
      aria-valuemin={Math.round(MIN * 100)}
      aria-valuemax={Math.round((1 - MIN) * 100)}
      tabindex="0"
      onmousedown={startDrag}
      onkeydown={onDividerKey}
      ondblclick={() => (ratio = 0.5)}
    ></div>
    <div class="pane" style:flex="1 1 0">
      <BennuEditor pane={1} bind:this={second} {onGenerate} {onOverride} />
    </div>
  {:else if tabDragStore.active}
    <!-- Only while a tab is being carried: dropping it here splits the editor and puts it on the right. -->
    <div class="zone" class:hot={tabDragStore.hot === 'new'} data-tab-drop="new">
      <PanelRight size={18} />
      <span>Split right</span>
    </div>
  {/if}
</div>

<style>
  .split { display: flex; flex: 1; min-width: 0; min-height: 0; position: relative; }
  .split.dragging { cursor: col-resize; user-select: none; }
  .pane { display: flex; min-width: 0; min-height: 0; }

  /* A real gutter between the panes — the app's own background showing through, which is how the
     layout draws every separation — with a handle you can see and a grab area wider than it looks. */
  .divider {
    flex: 0 0 5px;
    cursor: col-resize;
    position: relative;
    outline: none;
    background: var(--bg-elevated);
  }
  /* Wider than the gutter, both ways: the pointer does not have to be exact to catch it. The panes'
     own edges are overlapped, which is why it sits above them. */
  .divider::after {
    content: ''; position: absolute; top: 0; bottom: 0; left: -6px; right: -6px; z-index: 5;
  }
  /* The handle: a short pill in the middle of the gutter, brightening into the accent on hover, focus
     and drag, with the whole gutter tinting along with it. */
  .divider::before {
    content: ''; position: absolute; top: 50%; left: 50%; width: 3px; height: 44px;
    transform: translate(-50%, -50%);
    border-radius: 2px;
    background: var(--border);
    transition: background var(--transition-fast), height var(--transition-fast);
  }
  .divider:hover, .divider:focus-visible, .divider.active {
    background: color-mix(in srgb, var(--accent) 14%, var(--bg-elevated));
  }
  .divider:hover::before, .divider:focus-visible::before, .divider.active::before {
    height: 72px; background: var(--accent);
  }

  /* Over the lone editor, below its tab strip, at the right edge. */
  .zone {
    position: absolute; top: 33px; right: 0; bottom: 0; width: 38%; z-index: 40;
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px;
    color: var(--text-muted);
    background: color-mix(in srgb, var(--bg-elevated) 80%, transparent);
    border: 2px dashed var(--border);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .zone.hot {
    color: var(--accent);
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
</style>
