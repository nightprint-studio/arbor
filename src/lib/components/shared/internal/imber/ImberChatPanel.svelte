<script lang="ts">
  /**
   * The Imber panel, as every product shows it: a header (title, actions), an optional toolbar, the thread —
   * or the home screen while there is nothing to read — and the composer docked underneath. A thin sweep
   * under the header runs for as long as a run is open.
   *
   * Props-driven and free of any product store: Bennu and Corvus fill the slots with their own content
   * (conversation switcher, mode toggle, messages, composer), so the two panels look and behave the same
   * and differ only in what they put in. The thread follows its end until the reader scrolls up, and each
   * conversation (`resetKey`) reopens where it was left — or at its end (`imber-follow`).
   */
  import { onDestroy, type Snippet } from 'svelte';
  import { Sparkles } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import './imber-motion.css';
  import { ThreadFollower } from './imber-follow';

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
    /** Which conversation is shown; another one lands where it was left, or at its end. */
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

  let flow = $state<HTMLElement | undefined>(undefined);
  let follower: ThreadFollower | null = null;
  $effect(() => {
    if (!thread || !flow || follower) return;
    follower = new ThreadFollower(thread, flow);
  });
  $effect(() => { const key = resetKey; follower?.open(key); void thread; void flow; });
  $effect(() => { void followKey; follower?.grew(); });
  onDestroy(() => follower?.destroy());
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
      <div class="flow" bind:this={flow}>
        {#if home}{@render homeContent?.()}{/if}
        {@render children?.()}
      </div>
    </div>

    {#snippet footer()}
      <div class="composer-col" class:wide>{@render dock?.()}</div>
    {/snippet}
  </PanelShell>
</div>

<style>
  .imber-root { height: 100%; min-height: 0; outline: none; display: flex; flex-direction: column; }

  /* The scroller; `.flow` is what it scrolls, and what the follower watches grow. No smooth scrolling: following
     the end is a jump per change, and a scroll still animating reads as the reader having moved. */
  /* No scroll anchoring either: the browser moving the view to keep a block in place is what made following
     slip a little above the end; the follower keeps the end itself. */
  .thread { flex: 1; min-height: 0; overflow-y: auto; overflow-anchor: none; }
  .flow {
    display: flex; flex-direction: column; gap: 18px;
    min-height: 100%; box-sizing: border-box;
    padding: 14px 12px 18px;
  }

  /* Expanded: the thread is a wide centred column — wide enough to use a big screen, narrow enough that the eye
     does not travel across it. Prose keeps a readable line length (`--asst-text-max`, read by the message);
     code, diagrams, diffs and the live cards use the column's full width. */
  .thread.wide:not(.home) .flow {
    --asst-text-max: 110ch;
    padding-inline: max(16px, calc((100% - var(--imber-wide-max)) / 2));
  }
  .imber-root { --imber-wide-max: min(1480px, 94%); }
  .composer-col.wide { max-width: var(--imber-wide-max); margin-inline: auto; width: 100%; }

  /* The home screen takes the whole panel, and paints its own padding. */
  .thread.home { overflow: hidden; }
  .thread.home .flow { padding: 0; gap: 0; height: 100%; }
</style>
