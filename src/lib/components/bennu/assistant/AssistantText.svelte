<script lang="ts">
  /**
   * A passage of the assistant's answer, written a little at a time.
   *
   * What has been received is paced by `nextLength` so a burst reads as writing rather than as a jump.
   * A passage that is already complete when it is first shown (an old conversation, a switch back to
   * the thread) appears whole. With animations off the text is shown as it arrives.
   */
  import { onDestroy, untrack } from 'svelte';
  import MarkdownView from '$lib/components/shared/ui/MarkdownView.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { nextLength } from './assistant-typewriter';

  let { text, live }: {
    text: string;
    /** The run that writes it is still going. */
    live: boolean;
  } = $props();

  // Typed out only if it is written while we watch: a long passage already there is history.
  let animated = untrack(() => live && text.length < 200);
  let shown = $state(untrack(() => (animated ? 0 : text.length)));
  let raf = 0;
  let last = 0;

  function frame(now: number) {
    raf = 0;
    const dt = last ? now - last : 16;
    last = now;
    shown = nextLength(shown, text.length, dt);
    if (shown < text.length) raf = requestAnimationFrame(frame);
    else last = 0;
  }

  $effect(() => {
    const total = text.length;
    if (live && untrack(() => shown) === 0 && total > 0) animated = true;
    untrack(() => {
      if (!animated || !animStore.enabled) { shown = total; return; }
      if (shown > total) shown = total;
      if (shown < total && !raf) raf = requestAnimationFrame(frame);
    });
  });

  onDestroy(() => { if (raf) cancelAnimationFrame(raf); });

  /** Still being written: the caret and the half-open Markdown handling stay on until it has caught up. */
  export function writing() { return live || shown < text.length; }
</script>

<MarkdownView text={shown >= text.length ? text : text.slice(0, shown)} streaming={live || shown < text.length} />
