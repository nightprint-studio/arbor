<script lang="ts">
  /**
   * Imber's home: where the panel opens, and where it goes when there is no conversation.
   *
   * The one screen that is allowed to be pretty, so it is: a slow aurora behind an emblem with a ripple
   * ringing it, the line for the kind of conversation you are in, what it would work with (`context`), a few
   * things to begin with as cards, and your recent conversations. Everything on it does something — a card
   * starts its request, a recent row opens that conversation — and nothing on it moves faster than a breath.
   * The words and the cards are the product's; the look is shared.
   */
  import type { Snippet } from 'svelte';
  import { MessageSquare, Sparkles } from 'lucide-svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import ImberAurora from '../ImberAurora.svelte';
  import ImberOrb from '../ImberOrb.svelte';
  import './imber-motion.css';
  import type { HomeRecent, HomeStarter } from './imber-types';

  let { chat = false, title, lead, starters, recent = [], footLine = '', label = 'Imber home', context, onStart, onOpen }: {
    /** A conversation that only talks: the cooler hue and the speech-bubble emblem. */
    chat?: boolean;
    title: string;
    lead: string;
    starters: HomeStarter[];
    recent?: HomeRecent[];
    /** A quiet line at the bottom ("€1.20 this month of €10.00"). */
    footLine?: string;
    label?: string;
    /** What the conversation would work with, as chips under the lead. */
    context?: Snippet;
    onStart: (starter: HomeStarter) => void;
    onOpen?: (id: string) => void;
  } = $props();
</script>

<section class="home" class:chat aria-label={label}>
  <ImberAurora hue="var(--hue)" />

  <div class="hero">
    <div class="emblem">
      <ImberOrb size={76} hue="var(--hue)">
        {#if chat}<MessageSquare size={22} />{:else}<Sparkles size={22} />{/if}
      </ImberOrb>
    </div>
    <h2>{title}</h2>
    <p class="lead">{lead}</p>
    {#if context}<div class="ctx">{@render context()}</div>{/if}
  </div>

  <div class="grid">
    {#each starters as s, i (s.title)}
      <button type="button" class="starter" style="--i: {i}" onclick={() => onStart(s)}>
        <Card hoverable padding="sm">
          <span class="s-ico"><s.icon size={15} /></span>
          <span class="s-title">{s.title}</span>
          <span class="s-hint">{s.hint}</span>
        </Card>
      </button>
    {/each}
  </div>

  {#if recent.length && onOpen}
    <div class="recent">
      <h4>Recent</h4>
      {#each recent as c (c.id)}
        <button type="button" class="row" onclick={() => onOpen?.(c.id)}>
          <span class="r-title">{c.title}</span>
          <span class="r-meta">{c.meta}</span>
        </button>
      {/each}
    </div>
  {/if}

  {#if footLine}<p class="foot">{footLine}</p>{/if}
</section>

<style>
  .home {
    --hue: var(--accent);
    position: relative; isolation: isolate;
    flex: 1; min-height: 0; overflow-x: hidden; overflow-y: auto;
    display: flex; flex-direction: column; align-items: center; gap: 16px;
    padding: 26px 16px 18px;
  }
  .home.chat { --hue: var(--info); }

  .hero { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 8px; padding-top: 4px; }
  .emblem { margin-bottom: 6px; }

  h2 { margin: 0; font-size: var(--font-size-lg); font-weight: 650; color: var(--text-primary); letter-spacing: -0.01em; }
  .lead { margin: 0; max-width: 290px; font-size: var(--font-size-xs); line-height: 1.55; color: var(--text-secondary); }
  .ctx { display: flex; flex-wrap: wrap; justify-content: center; gap: 5px; margin-top: 4px; }
  .ctx:empty { display: none; }

  .grid { width: 100%; max-width: 340px; display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .starter {
    all: unset; display: block; cursor: pointer; border-radius: var(--radius-md);
    animation: asst-rise calc(var(--anim-dur-slow, 240ms) * 2.5) ease-out both;
    animation-delay: calc(var(--i) * var(--anim-dur-slow, 240ms) * 0.4);
  }
  .starter:focus-visible { outline: 2px solid var(--hue); outline-offset: 2px; }
  .starter :global(.card) { height: 100%; transition: transform var(--transition-fast), border-color var(--transition-fast); }
  .starter:hover :global(.card) { transform: translateY(-2px); border-color: color-mix(in srgb, var(--hue) 55%, var(--border)); }
  .s-ico { display: grid; place-items: center; width: 26px; height: 26px; margin-bottom: 7px; border-radius: 8px; color: var(--hue); background: color-mix(in srgb, var(--hue) 14%, transparent); }
  .s-title { display: block; font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .s-hint { display: block; margin-top: 2px; font-size: var(--font-size-2xs); line-height: 1.4; color: var(--text-muted); }

  .recent { width: 100%; max-width: 340px; display: flex; flex-direction: column; gap: 2px; }
  h4 { margin: 4px 0 2px; font-size: var(--font-size-2xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-muted); }
  .row {
    all: unset; display: flex; align-items: baseline; justify-content: space-between; gap: 10px;
    padding: 5px 8px; border-radius: var(--radius-sm); cursor: pointer; font-size: var(--font-size-xs);
  }
  .row:hover { background: var(--bg-hover); }
  .row:focus-visible { outline: 2px solid var(--hue); }
  .r-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-secondary); }
  .r-meta { flex-shrink: 0; font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }

  .foot { margin: auto 0 0; font-size: var(--font-size-2xs); color: var(--text-muted); }

  @media (prefers-reduced-motion: reduce) {
    .starter { animation: none; }
  }
</style>
