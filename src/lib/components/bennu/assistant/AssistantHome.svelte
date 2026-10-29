<script lang="ts">
  /**
   * The assistant's home: where the panel opens, and where it goes when there is no conversation.
   *
   * It is the one screen that is allowed to be pretty, so it is: a slow aurora behind an emblem with a ripple
   * ringing it, the line for the mode you are in, the file and instructions it would work with, a few things
   * to begin with as cards, and your recent conversations. Everything on it does something — a card sends
   * its request, a recent row opens that conversation — and nothing on it moves faster than a breath.
   *
   * Agent and Chat have different homes because they are different things: one works inside the project with
   * the project's tools, the other only talks and so offers things that need no project.
   */
  import {
    Sparkles, MessageSquare, Wand2, Bug, BookOpen, GitCompareArrows, Lightbulb, Languages, Regex, ListChecks, BookText, FileText,
  } from 'lucide-svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import ImberOrb from '$lib/components/shared/internal/ImberOrb.svelte';
  import ImberAurora from '$lib/components/shared/internal/ImberAurora.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { formatEur, relativeTime } from './assistant-format';

  const store = bennuAssistantStore;

  interface Starter { icon: typeof Wand2; title: string; hint: string; text: string }

  // What an agent can begin with: things about the project it is in, which it reads for itself.
  const AGENT: Starter[] = [
    { icon: Wand2, title: 'Explain this file', hint: 'What it does and how it fits', text: 'Explain what this file does and how it fits into the project.' },
    { icon: BookOpen, title: 'Explain the project', hint: 'Structure, modules, where to start', text: 'Give me a tour of this project: its structure, the main modules and where to start reading. Open the panels that help.' },
    { icon: ListChecks, title: 'Show me a panel', hint: 'Dependencies, problems, tests…', text: 'Open the dependencies panel for this project and tell me what stands out.' },
    { icon: Bug, title: 'Find why it fails', hint: 'Start from the problems list', text: 'Look at the project\'s current problems and tell me what is wrong and how to fix it.' },
    { icon: BookOpen, title: 'Document it', hint: 'Javadoc for the public methods', text: 'Write documentation comments for the public methods of this file that have none.' },
    { icon: GitCompareArrows, title: 'Review my changes', hint: 'What could go wrong in the diff', text: 'Review the uncommitted changes of this project and point out what could go wrong.' },
  ];

  // What a chat can begin with: nothing that needs the project.
  const CHAT: Starter[] = [
    { icon: Lightbulb, title: 'Explain a concept', hint: 'In plain words, with an example', text: 'Explain this to me in plain words, with a short example: ' },
    { icon: Regex, title: 'Write a regex', hint: 'And say how it works', text: 'Write a regular expression that matches: ' },
    { icon: Languages, title: 'Rewrite a text', hint: 'Clearer, shorter, or translated', text: 'Rewrite this text so it is clearer and shorter: ' },
    { icon: ListChecks, title: 'Think it through', hint: 'Options and their trade-offs', text: 'Help me think through the options and trade-offs for: ' },
  ];

  const chat = $derived(store.chatOnly);
  const starters = $derived(chat ? CHAT : AGENT);
  const projectName = $derived(projectStore.project?.name ?? null);
  const fileName = $derived(projectStore.activeFilePath?.split(/[\\/]/).pop() ?? null);
  const recent = $derived(store.visible.filter((c) => c.updatedAt).slice(0, 4));
  const month = $derived(store.spend?.meters.find((m) => m.period === 'month'));

  // An agent starter is a whole request and is sent. A chat starter is the start of a sentence: it goes into
  // the composer for the user to finish.
  function begin(s: Starter) {
    if (!chat) { void store.send(s.text); return; }
    store.draft = s.text;
    store.focusComposer();
  }
</script>

<section class="home" class:chat aria-label="Imber home">
  <ImberAurora hue="var(--hue)" />

  <div class="hero">
    <div class="emblem">
      <ImberOrb size={76} hue="var(--hue)">
        {#if chat}<MessageSquare size={22} />{:else}<Sparkles size={22} />{/if}
      </ImberOrb>
    </div>
    <h2>{chat ? 'What’s on your mind?' : 'What are we working on?'}</h2>
    <p class="lead">
      {#if chat}
        Just a conversation with Imber. It cannot read, search or change your project — it only sees what you type. It can explain how Bennu works and open any panel for you.
      {:else}
        Ask Imber about the project or describe a change. It can explain the project, open any panel for you, and works through Bennu’s own tools, never a shell.
      {/if}
    </p>

    {#if !chat && (projectName || fileName || store.instructions.length)}
      <div class="ctx">
        {#if projectName}<Badge variant="chip" size="sm" label={projectName} />{/if}
        {#if fileName}<Badge variant="chip" size="sm" label={fileName}>{#snippet icon()}<FileText size={10} />{/snippet}</Badge>{/if}
        {#each store.instructions as file (file.name)}
          <Badge variant="chip" size="sm" label={file.name}>{#snippet icon()}<BookText size={10} />{/snippet}</Badge>
        {/each}
      </div>
    {/if}
  </div>

  <div class="grid">
    {#each starters as s, i (s.title)}
      <button type="button" class="starter" style="--i: {i}" onclick={() => begin(s)}>
        <Card hoverable padding="sm">
          <span class="s-ico"><s.icon size={15} /></span>
          <span class="s-title">{s.title}</span>
          <span class="s-hint">{s.hint}</span>
        </Card>
      </button>
    {/each}
  </div>

  {#if recent.length}
    <div class="recent">
      <h4>Recent</h4>
      {#each recent as c (c.id)}
        <button type="button" class="row" onclick={() => store.select(c.id)}>
          <span class="r-title">{c.title}</span>
          <span class="r-meta">{relativeTime(c.updatedAt, Date.now())} · {formatEur(store.eur(c.cost))}</span>
        </button>
      {/each}
    </div>
  {/if}

  {#if month && month.spentEur > 0}
    <p class="foot">{formatEur(month.spentEur)} this month{month.limitEur ? ` of ${formatEur(month.limitEur)}` : ''}</p>
  {/if}
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
