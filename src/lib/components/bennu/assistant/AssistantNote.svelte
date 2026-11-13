<script lang="ts">
  /**
   * A message the user wrote while Imber was working. In the thread it stands where the run read it — between
   * the steps, not as a turn of its own; above the composer, while it still waits for the step in progress to end.
   */
  import { MessageSquareReply, Clock } from 'lucide-svelte';

  let { text, pending = false }: { text: string; pending?: boolean } = $props();
</script>

<div class="note" class:pending>
  <div class="head">
    {#if pending}<Clock size={11} /> Waiting — read after the current step{:else}<MessageSquareReply size={11} /> You, while it worked{/if}
  </div>
  <p>{text}</p>
</div>

<style>
  .note {
    display: flex; flex-direction: column; gap: 3px;
    padding: 7px 10px; border-radius: var(--radius-md);
    border-left: 2px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 7%, var(--bg-elevated));
  }
  .note.pending { border-left-color: var(--warning); background: color-mix(in srgb, var(--warning) 7%, var(--bg-elevated)); }
  .head { display: flex; align-items: center; gap: 5px; font-size: var(--font-size-xs); color: var(--text-muted); }
  p { margin: 0; font-size: var(--font-size-sm); line-height: 1.5; color: var(--text-primary); white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
