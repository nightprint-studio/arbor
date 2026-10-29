<script lang="ts">
  /**
   * BennuAssistantPanel — the assistant, docked on the right.
   *
   * A switch in the header chooses between two kinds of conversation, kept apart: **Agent**, which works in
   * the project through Bennu's tools, and **Chat**, which has no tools and no access to files. Each has its
   * own conversations and its own home screen.
   *
   * Top to bottom: the conversation (a switcher, saved), how full the window is, the thread — or the home
   * screen when there is no conversation open — and the composer with the choices that shape the next answer.
   */
  import { onMount, tick } from 'svelte';
  import { Sparkles, SquarePen, Trash2, Ellipsis, KeyRound, Eye, FlaskConical, Bot, MessageSquare } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import AssistantHeader from './AssistantHeader.svelte';
  import AssistantContextBar from './AssistantContextBar.svelte';
  import AssistantMessage from './AssistantMessage.svelte';
  import AssistantComposer from './AssistantComposer.svelte';
  import AssistantReviewModal from './AssistantReviewModal.svelte';
  import AssistantRequestPreview from './AssistantRequestPreview.svelte';
  import AssistantCompactModal from './AssistantCompactModal.svelte';
  import AssistantActivity from './AssistantActivity.svelte';
  import AssistantHome from './AssistantHome.svelte';
  import './assistant-motion.css';

  const store = bennuAssistantStore;
  onMount(() => { void store.start(); });
  let thread = $state<HTMLElement | null>(null);

  // The thread follows its end: a new message, or another conversation, lands at the bottom.
  $effect(() => {
    void store.activeId;
    void store.active?.messages.length;
    void tick().then(() => thread?.scrollTo({ top: thread.scrollHeight }));
  });

  const menu = $derived<DropdownItem[]>([
    {
      // The key is set once, in Settings, and lives in the credential vault — the panel only points there.
      kind: 'item', id: 'settings', label: 'Assistant settings…', subtitle: 'API key, defaults and what it can reach',
      icon: KeyRound, onclick: () => bennuUiStore.openSettings('assistant'),
    },
    {
      kind: 'item', id: 'preview', label: 'Show what would be sent…', subtitle: 'The request for the next message, nothing sent',
      icon: Eye, onclick: () => void store.showPreview(''),
    },
    {
      kind: 'item', id: 'dry', label: 'Dry run', subtitle: 'Send shows the request instead of making it',
      icon: FlaskConical, active: store.dryRun, onclick: () => store.setDryRun(!store.dryRun),
    },
    { kind: 'separator' },
    {
      kind: 'item', id: 'delete', label: 'Delete conversation', icon: Trash2, danger: true,
      disabled: !store.active,
      onclick: () => store.active && store.remove(store.active.id),
    },
  ]);

  const MODES = [
    { value: 'agent', label: 'Agent', icon: Bot },
    { value: 'chat', label: 'Chat', icon: MessageSquare },
  ];
</script>

<PanelShell title="Assistant" scrollable={false}>
  {#snippet icon()}<Sparkles size={13} />{/snippet}

  {#snippet actions()}
    <RadioGroup
      value={store.chatOnly ? 'chat' : 'agent'}
      options={MODES}
      appearance="segment"
      size="sm"
      nowrap
      onchange={(v) => store.setChatOnly(v === 'chat')}
    />
    <IconButton tooltip="New conversation" size={24} onclick={() => store.newChat()}><SquarePen size={14} /></IconButton>
    <Dropdown items={menu} position="fixed" direction="down" width="270px">
      {#snippet trigger({ open, toggle })}
        <IconButton tooltip="More" size={24} active={open} ariaHasPopup ariaExpanded={open} onclick={toggle}>
          <Ellipsis size={14} />
        </IconButton>
      {/snippet}
    </Dropdown>
  {/snippet}

  {#snippet toolbar()}
    <div class="top">
      <AssistantHeader />
    </div>
    <AssistantContextBar />
    <!-- A thin sweep under the header for as long as a run is open. -->
    {#if store.running}<div class="asst-sweep" aria-hidden="true"></div>{/if}
  {/snippet}

  <div class="thread" class:home={!(store.active && store.active.messages.length > 0)} bind:this={thread}>
    {#if store.active && store.active.messages.length > 0}
      {#each store.active.messages as m (m.id)}
        <AssistantMessage message={m} conversationId={store.active.id} mode={store.mode} last={m.id === store.active.messages.at(-1)?.id} />
      {/each}
      <AssistantActivity />
    {:else}
      <AssistantHome />
    {/if}
  </div>

  {#snippet footer()}
    <AssistantComposer />
  {/snippet}
</PanelShell>

<AssistantReviewModal />
<AssistantRequestPreview />
<AssistantCompactModal />

<style>
  .top { padding: 8px 12px 4px; }

  .thread {
    flex: 1; min-height: 0;
    overflow-y: auto;
    display: flex; flex-direction: column; gap: 18px;
    padding: 14px 12px 18px;
    scroll-behavior: smooth;
  }

  /* The home screen takes the whole panel, and paints its own padding. */
  .thread.home { padding: 0; gap: 0; overflow: hidden; }
</style>
