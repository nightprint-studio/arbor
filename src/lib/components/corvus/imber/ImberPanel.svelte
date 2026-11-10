<script lang="ts">
  /**
   * Imber's panel in Corvus: the same assistant as Bennu's, pointed at the repository of the active tab —
   * and drawn with the same shared widgets (`shared/internal/imber`), so the two panels look and behave alike.
   *
   * Two kinds of conversation. **Operate** has the git tools: commit, push, fetch, pull, branches, merge,
   * merge requests, and the history questions (log, blame, show). **Chat** has none: it only talks.
   * Two modes for Operate: **Manual** shows every write as a proposal with the exact command and waits;
   * **Auto** runs them, except the destructive ones, which always ask.
   *
   * Keyboard-first: the composer takes `Ctrl+Enter` to send; while an action waits, `Ctrl+Enter`
   * approves and `Esc` declines from anywhere in the panel; `Esc` while a run goes stops it.
   */
  import {
    Bot, CalendarClock, ChevronDown, GitBranch, GitCommitHorizontal, GitMerge, GitPullRequest, History, ShieldCheck,
    SquarePen, Undo2, Zap, MessageSquare, Lightbulb, Users,
  } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import ImberActivityLine from '$lib/components/shared/internal/imber/ImberActivityLine.svelte';
  import ImberChatPanel from '$lib/components/shared/internal/imber/ImberChatPanel.svelte';
  import ImberComposerFrame from '$lib/components/shared/internal/imber/ImberComposerFrame.svelte';
  import ImberConversationBar from '$lib/components/shared/internal/imber/ImberConversationBar.svelte';
  import ImberHome from '$lib/components/shared/internal/imber/ImberHome.svelte';
  import type { HomeStarter } from '$lib/components/shared/internal/imber/imber-types';
  import { storeAssistantKey } from '$lib/ipc/corvus/imber';
  import { imberStore } from '$lib/stores/corvus/imber.svelte';
  import { corvusImberSchedules } from '$lib/stores/corvus/imber-schedules.svelte';
  import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { ImberKind, ImberMode } from '$lib/types/corvus/imber';
  import ImberMessage from './ImberMessage.svelte';
  import { activityText } from './imber-messages';

  const store = imberStore;
  const tab = $derived(tabsStore.activeTab);
  const conv = $derived(store.current);
  const kind = $derived<ImberKind>(conv?.kind ?? 'operate');
  const messages = $derived(store.messages);
  const pending = $derived(store.pending);
  const running = $derived(store.running);

  let composer = $state<HTMLTextAreaElement | undefined>(undefined);
  let root = $state<HTMLElement | undefined>(undefined);
  let keyDraft = $state('');
  let savingKey = $state(false);

  const KINDS = [
    { value: 'operate', label: 'Operate', icon: Bot },
    { value: 'chat', label: 'Chat', icon: MessageSquare },
  ];

  // What Imber can begin with — a whole request each; they are sent as they are.
  const STARTERS: Record<ImberKind, HomeStarter[]> = {
    operate: [
      { icon: GitCommitHorizontal, title: 'Commit what is staged', hint: 'With a message it writes', text: 'Write a commit message for what is staged and commit it' },
      { icon: History, title: 'Summarise this branch', hint: 'What it adds over the default', text: 'Summarise what this branch adds over the default branch' },
      { icon: GitPullRequest, title: 'Push and open an MR', hint: 'Push, then the merge request', text: 'Push this branch and open a merge request' },
      { icon: Users, title: 'Who changed what', hint: 'The last month of history', text: 'Who changed the most here in the last month?' },
    ],
    chat: [
      { icon: Undo2, title: 'Undo a commit', hint: 'Keeping the changes', text: 'How do I undo my last commit without losing the changes?' },
      { icon: GitMerge, title: 'Merge or rebase?', hint: 'The difference, with an example', text: 'What is the difference between merge and rebase?' },
      { icon: Lightbulb, title: 'Explain a command', hint: 'In plain words', text: 'Explain this git command in plain words: ' },
      { icon: GitBranch, title: 'Plan a workflow', hint: 'Branches for a team', text: 'Suggest a branching workflow for a small team.' },
    ],
  };

  // An approval waiting is what the user must look at: take the keys, so Ctrl+Enter / Esc reach it.
  $effect(() => { if (pending) root?.focus(); });

  // Focus the composer when the panel opens on a repository.
  $effect(() => { if (tab && store.info?.hasKey) composer?.focus(); });

  async function send(text = store.draft) {
    const body = text.trim();
    if (!body || running || pending) return;
    store.draft = '';
    await store.send(body);
  }

  function begin(s: HomeStarter) {
    if (kind === 'chat' && s.text.endsWith(': ')) { store.draft = s.text; composer?.focus(); return; }
    void send(s.text);
  }

  function onKeydown(e: KeyboardEvent) {
    if (pending) {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void store.answer(pending.promptId, 'approve'); }
      else if (e.key === 'Escape') { e.preventDefault(); void store.answer(pending.promptId, 'reject'); }
      return;
    }
    if (e.key === 'Escape' && running) { e.preventDefault(); void store.cancel(); }
  }

  async function saveKey() {
    if (!keyDraft.trim() || savingKey) return;
    savingKey = true;
    try {
      await storeAssistantKey(keyDraft.trim());
      keyDraft = '';
      await store.loadState();
    } catch (err) {
      uiStore.showToast(`Could not store the key: ${err}`, 'error');
    } finally {
      savingKey = false;
    }
  }

  const ready = $derived(!!tab && !(store.info && !store.info.hasKey));
  const status = $derived(activityText({ running: !!conv?.running, retry: conv?.retry ?? null, waiting: !!pending, tool: conv?.status ?? '' }));

  const MODE_ICON = { manual: ShieldCheck, auto: Zap } as const;
  const MODES: { id: ImberMode; label: string; description: string }[] = [
    { id: 'manual', label: 'Manual', description: 'Every change is shown as a command you approve' },
    { id: 'auto', label: 'Auto', description: 'Changes run at once; force pushes and deletions still ask' },
  ];
  const modeItems = $derived<DropdownItem[]>(MODES.map((m) => ({
    kind: 'item' as const, id: m.id, label: m.label, subtitle: m.description, icon: MODE_ICON[m.id],
    active: store.mode === m.id, onclick: () => void store.setMode(m.id),
  })));
  const ModeIcon = $derived(MODE_ICON[store.mode]);
  const currentMode = $derived(MODES.find((m) => m.id === store.mode) ?? MODES[0]);
</script>

<ImberChatPanel
  ariaLabel="Imber"
  {running}
  home={ready && messages.length === 0}
  followKey={messages}
  resetKey={tab?.id ?? ''}
  bind:root
  onkeydown={onKeydown}
>
  {#snippet headerActions()}
    <RadioGroup
      value={kind}
      options={KINDS}
      appearance="segment"
      size="sm"
      nowrap
      onchange={(v) => store.setKind(v as ImberKind)}
    />
    <IconButton size={24} tooltip="Undo Imber's last action" disabled={!store.canUndo || running} onclick={() => void store.undo()}>
      <Undo2 size={14} />
    </IconButton>
    <IconButton size={24} tooltip="Schedules" onclick={() => corvusImberSchedules.show()}>
      <CalendarClock size={14} />
    </IconButton>
    <IconButton size={24} tooltip="New conversation" disabled={!conv || messages.length === 0} onclick={() => void store.newConversation()}>
      <SquarePen size={14} />
    </IconButton>
  {/snippet}

  {#snippet bars()}
    {#if tab}
      <div class="top"><ImberConversationBar title={tab.name} /></div>
    {/if}
  {/snippet}

  {#snippet homeContent()}
    <ImberHome
      chat={kind === 'chat'}
      title={kind === 'operate' ? 'What are we working on?' : 'What’s on your mind?'}
      lead={kind === 'operate'
        ? `Ask Imber to work on ${tab?.name ?? 'the repository'}. ${store.mode === 'manual' ? 'Manual: every change is shown as a command you approve.' : 'Auto: changes run at once; force pushes and deletions still ask.'}`
        : 'A chat about git. It cannot see or change the repository — it only sees what you type.'}
      starters={STARTERS[kind]}
      onStart={begin}
    />
  {/snippet}

  {#if !tab}
    <EmptyState message="Open a repository" description="Imber works on the repository of the active tab." />
  {:else if store.info && !store.info.hasKey}
    <div class="setup">
      <Alert variant="info" title="Add your API key" text="Imber uses your own Anthropic key. It is the same one Bennu uses: store it once, here or in Bennu's settings, and both work. It goes to the system credential vault, never to a file." />
      <Input type="password" placeholder="sk-ant-…" bind:value={keyDraft} onkeydown={(e) => { if (e.key === 'Enter') void saveKey(); }} />
      <Button variant="primary" size="sm" loading={savingKey} disabled={!keyDraft.trim()} onclick={saveKey}>Store key</Button>
    </div>
  {:else}
    {#each messages as m, i (m.id)}
      <ImberMessage
        message={m}
        name="Imber"
        live={running && i === messages.length - 1 && m.role === 'assistant'}
        canRetry={!!conv?.canRetry && i === messages.length - 1}
        onDecide={(id, decision) => void store.answer(id, decision)}
        onRetry={() => void store.retry()}
      />
    {/each}
    {#if status}
      <ImberActivityLine label={status.label} retry={status.retry} detail={status.detail} onStop={() => void store.cancel()} />
    {/if}
  {/if}

  {#snippet dock()}
    {#if ready}
      <ImberComposerFrame
        bind:value={store.draft}
        bind:element={composer}
        placeholder={kind === 'operate' ? 'Ask Imber to commit, push, branch, merge, or explain the history…' : 'Ask a question about git…'}
        disabled={!!pending}
        {running}
        canSend={!!store.draft.trim() && !pending}
        onSend={() => void send()}
        onStop={() => void store.cancel()}
      >
        {#snippet controls()}
          {#if store.info?.modelName}
            <span class="model" title="The model Imber uses — set in Settings, shared with Bennu"><i class="model-dot"></i>{store.info.modelName}</span>
          {/if}
        {/snippet}

        {#snippet trailing()}
          {#if kind === 'operate'}
            <Dropdown items={modeItems} position="fixed" direction="up" width="340px">
              {#snippet trigger({ open, toggle })}
                <Button
                  variant="ghost" size="xs" ariaExpanded={open} ariaHaspopup="menu"
                  tooltip={{ content: `Mode — ${currentMode.label}`, description: currentMode.description }} onclick={toggle}
                >
                  <span class="lbl"><ModeIcon size={12} />{currentMode.label}<ChevronDown size={11} /></span>
                </Button>
              {/snippet}
            </Dropdown>
          {/if}
        {/snippet}

        {#snippet hint()}
          {#if kind === 'chat'}<MessageSquare size={10} />It only talks: it cannot see or change the repository.
          {:else if store.mode === 'auto'}<Zap size={10} />Changes run at once; force pushes and deletions still ask.
          {:else}<ShieldCheck size={10} />You approve every change before it runs.{/if}
        {/snippet}

        {#snippet figures()}
          {#if conv && conv.costUsd > 0}<span class="cost" title="What this conversation has cost so far, at the model's list price">≈ ${conv.costUsd.toFixed(3)}</span>{/if}
        {/snippet}
      </ImberComposerFrame>
    {/if}
  {/snippet}
</ImberChatPanel>

<style>
  .top { padding: 8px 12px 4px; }
  .setup { display: flex; flex-direction: column; gap: 8px; }
  .lbl { display: inline-flex; align-items: center; gap: 6px; }
  .model { display: inline-flex; align-items: center; gap: 6px; padding: 0 6px; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .model-dot {
    width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
    background: linear-gradient(140deg, var(--accent), var(--info));
  }
  .cost { margin-left: auto; flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
</style>
