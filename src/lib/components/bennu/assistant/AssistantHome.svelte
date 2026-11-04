<script lang="ts">
  /**
   * Bennu's assistant home: the shared Imber home with Bennu's words and cards.
   *
   * Agent and Chat have different homes because they are different things: one works inside the project with
   * the project's tools, the other only talks and so offers things that need no project. An agent starter is
   * a whole request and is sent; a chat starter is the start of a sentence and goes into the composer for the
   * user to finish.
   */
  import {
    Wand2, Bug, BookOpen, GitCompareArrows, Lightbulb, Languages, Regex, ListChecks, BookText, FileText,
  } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import ImberHome from '$lib/components/shared/internal/imber/ImberHome.svelte';
  import type { HomeStarter } from '$lib/components/shared/internal/imber/imber-types';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { formatEur, relativeTime } from './assistant-format';

  const store = bennuAssistantStore;

  // What an agent can begin with: things about the project it is in, which it reads for itself.
  const AGENT: HomeStarter[] = [
    { icon: Wand2, title: 'Explain this file', hint: 'What it does and how it fits', text: 'Explain what this file does and how it fits into the project.' },
    { icon: BookOpen, title: 'Explain the project', hint: 'Structure, modules, where to start', text: 'Give me a tour of this project: its structure, the main modules and where to start reading. Open the panels that help.' },
    { icon: ListChecks, title: 'Show me a panel', hint: 'Dependencies, problems, tests…', text: 'Open the dependencies panel for this project and tell me what stands out.' },
    { icon: Bug, title: 'Find why it fails', hint: 'Start from the problems list', text: 'Look at the project\'s current problems and tell me what is wrong and how to fix it.' },
    { icon: BookOpen, title: 'Document it', hint: 'Javadoc for the public methods', text: 'Write documentation comments for the public methods of this file that have none.' },
    { icon: GitCompareArrows, title: 'Review my changes', hint: 'What could go wrong in the diff', text: 'Review the uncommitted changes of this project and point out what could go wrong.' },
  ];

  // What a chat can begin with: nothing that needs the project.
  const CHAT: HomeStarter[] = [
    { icon: Lightbulb, title: 'Explain a concept', hint: 'In plain words, with an example', text: 'Explain this to me in plain words, with a short example: ' },
    { icon: Regex, title: 'Write a regex', hint: 'And say how it works', text: 'Write a regular expression that matches: ' },
    { icon: Languages, title: 'Rewrite a text', hint: 'Clearer, shorter, or translated', text: 'Rewrite this text so it is clearer and shorter: ' },
    { icon: ListChecks, title: 'Think it through', hint: 'Options and their trade-offs', text: 'Help me think through the options and trade-offs for: ' },
  ];

  const chat = $derived(store.chatOnly);
  const projectName = $derived(projectStore.project?.name ?? null);
  const fileName = $derived(projectStore.activeFilePath?.split(/[\\/]/).pop() ?? null);
  const recent = $derived(
    store.visible.filter((c) => c.updatedAt).slice(0, 4)
      .map((c) => ({ id: c.id, title: c.title, meta: `${relativeTime(c.updatedAt, Date.now())} · ${formatEur(store.eur(c.cost))}` })),
  );
  const month = $derived(store.spend?.meters.find((m) => m.period === 'month'));
  const footLine = $derived(month && month.spentEur > 0 ? `${formatEur(month.spentEur)} this month${month.limitEur ? ` of ${formatEur(month.limitEur)}` : ''}` : '');

  function begin(s: HomeStarter) {
    if (!chat) { void store.send(s.text); return; }
    store.draft = s.text;
    store.focusComposer();
  }
</script>

<ImberHome
  {chat}
  title={chat ? 'What’s on your mind?' : 'What are we working on?'}
  lead={chat
    ? 'Just a conversation with Imber. It cannot read, search or change your project — it only sees what you type. It can explain how Bennu works and open any panel for you.'
    : 'Ask Imber about the project or describe a change. It can explain the project, open any panel for you, and works through Bennu’s own tools, never a shell.'}
  starters={chat ? CHAT : AGENT}
  {recent}
  {footLine}
  onStart={begin}
  onOpen={(id) => store.select(id)}
>
  {#snippet context()}
    {#if !chat}
      {#if projectName}<Badge variant="chip" size="sm" label={projectName} />{/if}
      {#if fileName}<Badge variant="chip" size="sm" label={fileName}>{#snippet icon()}<FileText size={10} />{/snippet}</Badge>{/if}
      {#each store.instructions as file (file.name)}
        <Badge variant="chip" size="sm" label={file.name}>{#snippet icon()}<BookText size={10} />{/snippet}</Badge>
      {/each}
    {/if}
  {/snippet}
</ImberHome>
