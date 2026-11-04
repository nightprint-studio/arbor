<script lang="ts">
  /**
   * The Imber panel, as every product shows it: a header (title, actions), an optional toolbar, the thread —
   * or the home screen while there is nothing to read — and the composer docked underneath. A thin sweep
   * under the header runs for as long as a run is open.
   *
   * Props-driven and free of any product store: Bennu and Corvus fill the slots with their own content
   * (conversation switcher, mode toggle, messages, composer), so the two panels look and behave the same
   * and differ only in what they put in. The thread follows its end while the reader is at the bottom, and
   * always jumps to it when `resetKey` changes (another conversation was opened).
   */
  import { tick, type Snippet } from 'svelte';
  import { Sparkles } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import './imber-motion.css';
  import { isNearBottom } from './imber-thread';

  let {
    title = 'Imber',
    ariaLabel = 'Imber',
    running = false,
    home = false,
    wide = false,
    followKey = 0,
    resetKey = '',
    root = $bindable(),
    thread = $bindable(),
    headerActions,
    bars,
    homeContent,
    children,
    dock,
    onkeydown,
    onheaderdblclick,
  }: {
    title?: string;
    ariaLabel?: string;
    /** A run is open: shows the sweep under the header. */
    running?: boolean;
    /** Show `homeContent` instead of the thread. */
    home?: boolean;
    /** Expanded layout: the thread becomes a centred column with a readable line length. */
    wide?: boolean;
    /** Changes whenever the thread grew (a message, a streamed token); the thread follows it if the reader is at the bottom. */
    followKey?: unknown;
    /** Changes when another conversation is shown; the thread then always lands at the bottom. */
    resetKey?: string;
    /** The panel's root, for a product that moves the focus into it. */
    root?: HTMLElement;
    thread?: HTMLElement;
    headerActions?: Snippet;
    /** Rows under the header (conversation bar, context bar, banners). */
    bars?: Snippet;
    homeContent?: Snippet;
    children?: Snippet;
    dock?: Snippet;
    /** Keys while focus is inside the panel. */
    onkeydown?: (e: KeyboardEvent) => void;
    onheaderdblclick?: () => void;
  } = $props();

  let lastReset = '';
  $effect(() => {
    void followKey;
    const reset = resetKey;
    const el = thread;
    if (!el) return;
    const jump = reset !== lastReset;
    lastReset = reset;
    if (jump || isNearBottom(el.scrollHeight, el.scrollTop, el.clientHeight)) {
      void tick().then(() => el.scrollTo({ top: el.scrollHeight }));
    }
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="imber-root" bind:this={root} tabindex="-1" role="region" aria-label={ariaLabel} {onkeydown}>
  <PanelShell {title} scrollable={false} {onheaderdblclick}>
    {#snippet icon()}<Sparkles size={13} />{/snippet}
    {#snippet actions()}{@render headerActions?.()}{/snippet}

    {#snippet toolbar()}
      {@render bars?.()}
      {#if running}<div class="asst-sweep" aria-hidden="true"></div>{/if}
    {/snippet}

    <div class="thread" class:home class:wide bind:this={thread}>
      {#if home}{@render homeContent?.()}{/if}
      {@render children?.()}
    </div>

    {#snippet footer()}
      <div class="composer-col" class:wide>{@render dock?.()}</div>
    {/snippet}
  </PanelShell>
</div>

<style>
  .imber-root { height: 100%; min-height: 0; outline: none; display: flex; flex-direction: column; }

  .thread {
    flex: 1; min-height: 0;
    overflow-y: auto;
    display: flex; flex-direction: column; gap: 18px;
    padding: 14px 12px 18px;
    scroll-behavior: smooth;
  }

  /* Expanded: the thread is a centred column. Prose keeps a readable line length (`--asst-text-max`,
     read by the message), code, diagrams, diffs and the live cards use the column's full width. */
  .thread.wide:not(.home) {
    --asst-text-max: 78ch;
    padding-inline: max(12px, calc((100% - 1040px) / 2));
  }
  .composer-col.wide { max-width: 1040px; margin-inline: auto; width: 100%; }

  /* The home screen takes the whole panel, and paints its own padding. */
  .thread.home { padding: 0; gap: 0; overflow: hidden; }
</style>
