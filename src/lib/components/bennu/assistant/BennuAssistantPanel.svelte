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
  import { onDestroy, onMount, tick } from 'svelte';
  import { Sparkles, SquarePen, Trash2, Ellipsis, KeyRound, Eye, FlaskConical, Bot, MessageSquare, WandSparkles, Maximize2, Minimize2 } from 'lucide-svelte';
  import { assistantLayoutStore } from '$lib/stores/bennu/assistant-layout.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuWorktreeStore } from '$lib/stores/bennu/assistant-worktree.svelte';
  import AssistantHeader from './AssistantHeader.svelte';
  import AssistantContextBar from './AssistantContextBar.svelte';
  import AssistantWorktreeBanner from './AssistantWorktreeBanner.svelte';
  import AssistantAgents from './AssistantAgents.svelte';
  import AssistantAgentModal from './AssistantAgentModal.svelte';
  import AssistantMessage from './AssistantMessage.svelte';
  import AssistantComposer from './AssistantComposer.svelte';
  import AssistantReviewModal from './AssistantReviewModal.svelte';
  import AssistantRequestPreview from './AssistantRequestPreview.svelte';
  import AssistantCompactModal from './AssistantCompactModal.svelte';
  import AssistantDecisionModal from './AssistantDecisionModal.svelte';
  import AssistantCostConfirm from './AssistantCostConfirm.svelte';
  import AssistantActivity from './AssistantActivity.svelte';
  import AssistantStopCard from './AssistantStopCard.svelte';
  import AssistantHome from './AssistantHome.svelte';
  import './assistant-motion.css';

  const store = bennuAssistantStore;
  onMount(() => { void store.start(); });
  let thread = $state<HTMLElement | null>(null);
  let root = $state<HTMLElement | null>(null);

  // The panel's two keyboard verbs, only while focus is inside it (Ctrl+Shift+M belongs to structural search).
  const layout = assistantLayoutStore;
  // Closing the panel (or switching to another right panel) ends the expansion: it reopens docked.
  onDestroy(() => layout.restore());

  /** Is anything above the panel — a modal, or an open menu — that should get the Escape first? */
  const somethingOpenOver = () =>
    [...document.querySelectorAll('[role="dialog"], [role="menu"], [role="listbox"]')].some((el) => el.getClientRects().length > 0);

  // Expanding is available wherever the panel is on screen (Ctrl+Shift+U); Escape gives the space back
  // only from inside the panel, and only when nothing the key belongs to is open: the slash menu and the
  // dropdowns stop the key themselves before it gets here.
  function onLayoutKey(e: KeyboardEvent) {
    if (e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey && e.key.toLowerCase() === 'u') {
      e.preventDefault();
      layout.toggle();
    } else if (e.key === 'Escape' && layout.expanded && !e.defaultPrevented && root?.contains(e.target as Node) && !somethingOpenOver()) {
      e.preventDefault();
      layout.restore();
    }
  }

  function onKeydown(e: KeyboardEvent) {
    onLayoutKey(e);
    if (!e.ctrlKey || !e.shiftKey || e.altKey || e.metaKey || !root?.contains(e.target as Node)) return;
    if (e.key === 'Enter' && bennuWorktreeStore.active && !store.running) {
      e.preventDefault();
      void bennuWorktreeStore.openDiff();
    } else if (e.key.toLowerCase() === 'q' && !store.chatOnly) {
      e.preventDefault();
      store.cycleMode();
    }
  }

  // The thread follows its end: a new message, or another conversation, lands at the bottom.
  $effect(() => {
    void store.activeId;
    void store.active?.messages.length;
    void tick().then(() => thread?.scrollTo({ top: thread.scrollHeight }));
  });

  const menu = $derived<DropdownItem[]>([
    {
      // The key is set once, in Settings, and lives in the credential vault — the panel only points there.
      kind: 'item', id: 'settings', label: 'Imber settings…', subtitle: 'API key, defaults and what it can reach',
      icon: KeyRound, onclick: () => bennuUiStore.openSettings('assistant'),
    },
    {
      kind: 'item', id: 'skills', label: 'Skills…', subtitle: `${store.skillCounts.on} of ${store.skillCounts.total} enabled`,
      icon: WandSparkles, onclick: () => store.openSkills(),
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
      onclick: () => store.active && store.askRemove(store.active.id),
    },
  ]);

  const MODES = [
    { value: 'agent', label: 'Agent', icon: Bot },
    { value: 'chat', label: 'Chat', icon: MessageSquare },
  ];
</script>

<svelte:window onkeydown={onKeydown} />
<div class="asst-root" bind:this={root}>
<PanelShell title="Imber" scrollable={false} onheaderdblclick={() => layout.toggle()}>
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
    <IconButton
      tooltip={layout.expanded ? 'Restore the panel' : 'Expand over the editor'}
      shortcut="Ctrl+Shift+U" size={24} active={layout.expanded} onclick={() => layout.toggle()}
    >
      {#if layout.expanded}<Minimize2 size={14} />{:else}<Maximize2 size={14} />{/if}
    </IconButton>
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
    <AssistantWorktreeBanner />
    <AssistantAgents />
    <!-- A thin sweep under the header for as long as a run is open. -->
    {#if store.running}<div class="asst-sweep" aria-hidden="true"></div>{/if}
  {/snippet}

  <div class="thread" class:home={!(store.active && store.active.messages.length > 0)} class:wide={layout.expanded} bind:this={thread}>
    {#if store.active && store.active.messages.length > 0}
      {#each store.active.messages as m (m.id)}
        <AssistantMessage message={m} conversationId={store.active.id} mode={store.mode} last={m.id === store.active.messages.at(-1)?.id} />
      {/each}
      <AssistantActivity />
    {:else}
      <AssistantHome />
    {/if}
    <AssistantStopCard />
  </div>

  {#snippet footer()}
    <div class="composer-col" class:wide={layout.expanded}><AssistantComposer /></div>
  {/snippet}
</PanelShell>
</div>

{#if store.deleteAsked}
  <ConfirmModal
    title="Delete conversation?"
    message={`“${store.deleteAsked.title}” will be deleted.`}
    detail="This cannot be undone."
    variant="danger"
    confirmLabel="Delete"
    onConfirm={() => void store.remove(store.deleteAsked!.id)}
    onCancel={() => store.cancelRemove()}
  />
{/if}
<AssistantReviewModal />
<AssistantRequestPreview />
<AssistantCompactModal />
<AssistantDecisionModal />
<AssistantAgentModal />
<AssistantCostConfirm />

<style>
  .top { padding: 8px 12px 4px; }

  /* Only there to scope the panel's keys; it adds no box of its own. */
  .asst-root { display: contents; }

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
