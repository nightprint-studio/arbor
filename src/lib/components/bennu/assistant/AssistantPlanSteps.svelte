<script lang="ts">
  /**
   * The steps of a plan, numbered: the one list both the plan card in the thread and the plan bar above it draw.
   * Each step carries its number — the same badge the questions use, so "2" reads the same in both — a tick once
   * done, and the one in progress is lifted out of the list with "Now": its badge breathes and its label shimmers,
   * so the eye finds it in a long plan. Steps are Markdown, so a flag or a path in backticks reads as code.
   *
   * With `center`, inside a box that scrolls (the tray's plan panel), the step in progress is brought to the middle
   * of the box when it opens and each time the work moves on — never by scrolling the thread around it.
   */
  import { Check } from 'lucide-svelte';
  import MarkdownView from '$lib/components/shared/ui/MarkdownView.svelte';
  import type { PlanStep } from './assistant-types';

  let { steps, center = false }: { steps: PlanStep[]; center?: boolean } = $props();

  let list = $state<HTMLOListElement | null>(null);
  const current = $derived(steps.findIndex((st) => st.status === 'in_progress'));

  /** The nearest ancestor that scrolls vertically, or null. */
  function scroller(from: HTMLElement): HTMLElement | null {
    for (let el = from.parentElement; el; el = el.parentElement) {
      const y = getComputedStyle(el).overflowY;
      if ((y === 'auto' || y === 'scroll') && el.scrollHeight > el.clientHeight) return el;
    }
    return null;
  }

  $effect(() => {
    if (!center || current < 0 || !list) return;
    const row = list.children[current] as HTMLElement | undefined;
    const box = row && scroller(row);
    if (!row || !box) return;
    const r = row.getBoundingClientRect();
    const b = box.getBoundingClientRect();
    box.scrollTo({ top: box.scrollTop + (r.top + r.height / 2) - (b.top + b.height / 2), behavior: 'smooth' });
  });
</script>

<ol bind:this={list}>
  {#each steps as step, i (i)}
    <li class="s-{step.status}" aria-current={step.status === 'in_progress' ? 'step' : undefined}>
      <span class="idx">{#if step.status === 'completed'}<Check size={10} strokeWidth={3} />{:else}{i + 1}{/if}</span>
      <span class="text"><MarkdownView text={step.text} size="sm" /></span>
      {#if step.status === 'in_progress'}<span class="now"><span class="asst-orb"></span><span class="asst-shimmer">Now</span></span>{/if}
    </li>
  {/each}
</ol>

<style>
  ol { list-style: none; margin: 2px 0 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  li {
    display: flex; align-items: flex-start; gap: 9px;
    padding: 4px 8px; border-radius: var(--radius-sm);
    border-left: 2px solid transparent;
    color: var(--text-secondary);
  }
  .text { flex: 1; min-width: 0; }

  .idx {
    display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; margin-top: 1px;
    width: 18px; height: 18px; border-radius: 50%;
    font-size: var(--font-size-2xs); font-weight: 700; font-variant-numeric: tabular-nums;
    background: var(--bg-overlay); color: var(--text-muted);
  }

  .s-completed { color: var(--text-muted); }
  .s-completed .idx { background: var(--success); color: var(--text-on-accent); }
  .s-completed .text { text-decoration: line-through; text-decoration-color: color-mix(in srgb, var(--text-muted) 50%, transparent); }

  .s-in_progress {
    color: var(--text-primary);
    border-left-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .s-in_progress .idx { background: var(--accent); color: var(--text-on-accent); animation: plan-now 2.4s ease-in-out infinite; }
  .s-in_progress {
    background-image: linear-gradient(90deg, color-mix(in srgb, var(--accent) 10%, transparent), transparent 70%);
    animation: plan-row 0.32s ease-out;
  }
  /* The badge breathes: a ring that swells and fades, so the step in progress is found at a glance. */
  @keyframes plan-now {
    0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 45%, transparent); }
    50% { box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 0%, transparent); }
  }
  /* A step that just became the current one slides in a touch, so the move is seen. */
  @keyframes plan-row { from { transform: translateX(-4px); opacity: 0.6; } to { transform: none; opacity: 1; } }
  @media (prefers-reduced-motion: reduce) {
    .s-in_progress, .s-in_progress .idx { animation: none; }
  }
  .now {
    display: inline-flex; align-items: center; gap: 6px; flex-shrink: 0; margin-top: 2px;
    font-size: var(--font-size-2xs); font-weight: 600; color: var(--accent);
  }
</style>
