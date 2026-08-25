<script lang="ts">
  /**
   * Tyto footer — IntelliJ-style status strip on `bg-elevated`. Shows the active
   * target + recording state on the left, and the output folder + capture count
   * on the right.
   */
  import { Circle, Video, Camera, Images, FolderOpen } from 'lucide-svelte';
  import { recorderStore, formatDuration } from '$lib/stores/tyto/recorder.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  const captureCount = $derived(recorderStore.captures.length);
  const frames = $derived(recorderStore.mode === 'record' && recorderStore.recordOutput === 'frames');
  const readyLabel = $derived(
    recorderStore.mode !== 'record' ? 'Ready to capture' : frames ? 'Ready to record frames' : 'Ready to record',
  );
</script>

<footer class="tyto-footer">
  <div class="left">
    {#if recorderStore.recording}
      <span class="rec-chip">
        <Circle size={9} fill="currentColor" />
        REC · {formatDuration(recorderStore.elapsedMs)}
      </span>
    {:else}
      {#if frames}<Images size={12} />
      {:else if recorderStore.mode === 'record'}<Video size={12} />
      {:else}<Camera size={12} />{/if}
      <span class="muted">{readyLabel}</span>
    {/if}
    <span class="sep">·</span>
    <span class="target">{recorderStore.currentTargetLabel}</span>
  </div>

  <div class="right">
    <span class="out" use:tooltip={recorderStore.outputDir}>
      <FolderOpen size={12} />
      {recorderStore.outputDir}
    </span>
    <span class="sep">·</span>
    <span class="muted">{captureCount} {captureCount === 1 ? 'capture' : 'captures'}</span>
  </div>
</footer>

<style>
  .tyto-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    height: 26px;
    flex-shrink: 0;
    padding: 0 12px;
    background: var(--bg-elevated);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    user-select: none;
  }
  .left, .right { display: flex; align-items: center; gap: 7px; min-width: 0; }
  .muted { color: var(--text-muted); }
  .sep { color: var(--border); }
  .target { color: var(--text-primary); font-weight: 500; }
  .out {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 340px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .rec-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--error);
    font-weight: 650;
    letter-spacing: 0.4px;
    font-variant-numeric: tabular-nums;
  }
  .rec-chip :global(svg) { animation: rec-pulse 1.3s ease-in-out infinite; }
  @keyframes rec-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.3; } }
</style>
