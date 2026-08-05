<script lang="ts">
  /**
   * The debugger's transport controls — resume, the three steps, detach.
   *
   * They live where the run status used to be, on the left of the Run console's status row,
   * and only while a session exists. That is the point of there being one panel: a debug
   * launch is a run, and the controls for it belong on the run you are looking at rather than
   * in a second window you have to go and find.
   */
  import {
    Play, StepForward, CornerDownRight, CornerUpLeft, Unplug, CircleSlash, CircleDot,
  } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuDebugStore } from '$lib/stores/bennu/debug.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

  const paused = $derived(bennuDebugStore.paused);
  const muted = $derived(bennuDebugStore.muted);
</script>

<div class="dc" role="toolbar" tabindex="-1" aria-label="Debugger">
  <button
    class="dc-btn"
    type="button"
    use:tooltip={{ content: 'Resume the program', shortcut: 'F9' }}
    aria-label="Resume the program"
    disabled={!paused}
    onclick={() => void bennuDebugStore.resume()}
  >
    <Play size={13} />
  </button>
  <button
    class="dc-btn"
    type="button"
    use:tooltip={{ content: 'Step over', shortcut: 'F8' }}
    aria-label="Step over"
    disabled={!paused}
    onclick={() => void bennuDebugStore.step('over')}
  >
    <StepForward size={13} />
  </button>
  <button
    class="dc-btn"
    type="button"
    use:tooltip={{ content: 'Step into', shortcut: 'F7' }}
    aria-label="Step into"
    disabled={!paused}
    onclick={() => void bennuDebugStore.step('into')}
  >
    <CornerDownRight size={13} />
  </button>
  <button
    class="dc-btn"
    type="button"
    use:tooltip={{ content: 'Step out', shortcut: 'Shift+F8' }}
    aria-label="Step out"
    disabled={!paused}
    onclick={() => void bennuDebugStore.step('out')}
  >
    <CornerUpLeft size={13} />
  </button>
  <span class="dc-sep"></span>
  <!-- Mute, not delete. Reaching the end of a run without losing the twelve breakpoints you
       will want back in a minute — the VM's requests go, the breakpoints stay. -->
  <button
    class="dc-btn"
    class:on={muted}
    type="button"
    use:tooltip={muted ? 'Breakpoints are muted — click to arm them' : 'Mute breakpoints'}
    aria-label="Mute breakpoints"
    aria-pressed={muted}
    onclick={() => void bennuDebugStore.toggleMute()}
  >
    <CircleSlash size={13} />
  </button>
  <button
    class="dc-btn"
    type="button"
    use:tooltip={{ content: 'Breakpoints…', shortcut: 'Ctrl+Shift+F8' }}
    aria-label="Breakpoints"
    onclick={() => bennuUiStore.openBreakpoints()}
  >
    <CircleDot size={13} />
  </button>
  <span class="dc-sep"></span>
  <!-- Detach, not stop. The program keeps running: a server you attached to in order to look
       at one request should not die because you finished looking. Stopping it is the ⏹ above. -->
  <button
    class="dc-btn"
    type="button"
    use:tooltip={'Detach — the program keeps running'}
    aria-label="Detach from the program"
    onclick={() => void bennuDebugStore.detachSession()}
  >
    <Unplug size={13} />
  </button>
</div>

<style>
  .dc { display: flex; align-items: center; gap: 1px; }
  .dc-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 20px; padding: 0;
    border: 0; border-radius: var(--radius-sm); background: none;
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .dc-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .dc-btn:disabled { color: var(--text-disabled); cursor: default; }
  /* Muted reads as a WARNING, not as an accent: it is a state in which the debugger is
     deliberately ignoring you, and forgetting it is on is the whole failure mode. */
  .dc-btn.on { color: var(--warning); }
  .dc-sep {
    width: 1px; height: 13px; margin: 0 4px;
    background: var(--border-subtle);
  }
</style>
