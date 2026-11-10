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
  import { onDestroy, onMount } from 'svelte';
  import { BookUser, SquarePen, Trash2, Ellipsis, KeyRound, Eye, FlaskConical, Bot, MessageSquare, WandSparkles, Maximize2, Minimize2, CalendarClock } from 'lucide-svelte';
  import { bennuImberSchedules } from '$lib/stores/bennu/imber-schedules.svelte';
  import { assistantLayoutStore } from '$lib/stores/bennu/assistant-layout.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import ImberChatPanel from '$lib/components/shared/internal/imber/ImberChatPanel.svelte';
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

  const store = bennuAssistantStore;
  onMount(() => { void store.start(); });
  let root = $state<HTMLElement | undefined>(undefined);

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


  const menu = $derived<DropdownItem[]>([
    {
      // The key is set once, in Settings, and lives in the credential vault — the panel only points there.
      kind: 'item', id: 'settings', label: 'Imber settings…', subtitle: 'API key, defaults and what it can reach',
      icon: KeyRound, onclick: () => bennuUiStore.openSettings('assistant'),
    },
    {
      kind: 'item', id: 'context', label: 'General context…', subtitle: store.generalContext.trim() ? 'Standing notes sent with every conversation' : 'Tell Imber who you are and how you work',
      icon: BookUser, onclick: () => store.openContext(),
    },
    {
      kind: 'item', id: 'skills', label: 'Skills…', subtitle: `${store.skillCounts.on} of ${store.skillCounts.total} enabled`,
      icon: WandSparkles, onclick: () => store.openSkills(),
    },
    {
      kind: 'item', id: 'schedules', label: 'Schedules…', subtitle: 'Prompts Imber runs on a timetable',
      icon: CalendarClock, onclick: () => bennuImberSchedules.show(),
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
<ImberChatPanel
  running={store.running}
  home={!(store.active && store.active.messages.length > 0)}
  wide={layout.expanded}
  followKey={`${store.active?.messages.length ?? 0}:${store.active?.messages.at(-1)?.blocks.length ?? 0}`}
  resetKey={store.activeId ?? ''}
  bind:root
  onheaderdblclick={() => layout.toggle()}
>
  {#snippet headerActions()}
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

  {#snippet bars()}
    <div class="top">
      <AssistantHeader />
    </div>
    <AssistantContextBar />
    <AssistantWorktreeBanner />
    <AssistantAgents />
  {/snippet}

  {#snippet homeContent()}<AssistantHome />{/snippet}

  {#if store.active}
    {#each store.active.messages as m (m.id)}
      <AssistantMessage message={m} conversationId={store.active.id} mode={store.mode} last={m.id === store.active.messages.at(-1)?.id} />
    {/each}
  {/if}
  <AssistantActivity />
  <AssistantStopCard />

  {#snippet dock()}<AssistantComposer />{/snippet}
</ImberChatPanel>

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
</style>
