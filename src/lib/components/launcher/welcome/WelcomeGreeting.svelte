<script lang="ts">
  /**
   * The greeting, typed by the hour, and under it a prompt that waits for you: one caret that sits on whichever line is
   * being written, a phrase typed and wiped in turn. When the hour crosses into another part of the day, the greeting
   * is typed again. With animations off both lines are simply there.
   */
  import { untrack } from 'svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { greetingFor } from './welcome-logic';
  import { write, wipe, type Typist } from './typewriter';

  let { phrases }: { phrases: string[] } = $props();

  let greeting = $state('');
  let line = $state('');
  let caretOn = $state<'greeting' | 'line'>('greeting');

  let epoch = 0;
  const snapshot = () => phrases;

  async function run() {
    const mine = ++epoch;
    const animated = animStore.enabled;
    const typist = (set: (t: string) => void): Typist => ({
      set,
      sleep: (ms) => (animated ? new Promise((r) => setTimeout(r, ms)) : Promise.resolve()),
      cancelled: () => mine !== epoch,
    });
    const text = greetingFor(new Date().getHours());
    caretOn = 'greeting'; line = '';
    if (!animated) { greeting = text; line = snapshot()[0] ?? ''; caretOn = 'line'; return; }
    greeting = '';
    if (!(await write(typist((t) => (greeting = t)), text))) return;
    await typist(() => {}).sleep(380);
    caretOn = 'line';
    for (let k = 0; mine === epoch; k++) {
      const list = snapshot(), phrase = list[k % list.length];
      if (!(await write(typist((t) => (line = t)), phrase, 34))) return;
      await typist(() => {}).sleep(2400);
      if (!(await wipe(typist((t) => (line = t)), phrase))) return;
      await typist(() => {}).sleep(260);
    }
  }

  $effect(() => {
    // the run reads `phrases` and the animations switch; it must not restart every time a recent is added
    untrack(() => void run());
    let shown = greetingFor(new Date().getHours());
    const tick = setInterval(() => {
      const now = greetingFor(new Date().getHours());
      if (now !== shown) { shown = now; void run(); }
    }, 30_000);
    return () => { epoch++; clearInterval(tick); };
  });
</script>

<div class="greet">
  <h1 aria-live="polite">{greeting}{#if caretOn === 'greeting'}<span class="caret bar"></span>{/if}</h1>
  <p class="prompt"><span class="chev" aria-hidden="true">›</span><span>{line}</span>{#if caretOn === 'line'}<span class="caret block"></span>{/if}</p>
</div>

<style>
  .greet { margin-bottom: 22px; }
  h1 {
    margin: 0;
    min-height: 1.25em;
    font-size: 34px;
    line-height: 1.25;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: var(--text-primary);
  }
  .prompt {
    display: flex;
    align-items: baseline;
    gap: 9px;
    margin: 6px 0 0;
    min-height: 1.6em;
    font: 15px/1.6 var(--font-code);
    color: var(--text-secondary);
  }
  .chev { color: var(--accent); }
  .caret { display: inline-block; background: var(--accent); border-radius: 1px; animation: blink 1.05s steps(1) infinite; }
  .caret.bar { width: 3px; height: 0.95em; margin-left: 2px; vertical-align: -0.1em; }
  .caret.block { width: 0.55em; height: 1.05em; margin-left: 2px; vertical-align: -0.17em; }
  @keyframes blink { 50% { opacity: 0; } }
  @media (max-width: 640px) { h1 { font-size: 26px; } }
</style>
