<script lang="ts">
  /**
   * Imber's panel in Corvus: the same assistant as Bennu's, pointed at the repository of the active tab.
   *
   * Two kinds of conversation. **Operate** has the git tools: commit, push, fetch, pull, branches, merge,
   * merge requests, and the history questions (log, blame, show). **Chat** has none: it only talks.
   * Two modes for Operate: **Manual** shows every write as a proposal with the exact command and waits;
   * **Auto** runs them, except the destructive ones, which always ask.
   *
   * Keyboard-first: the composer takes `Ctrl+Enter` to send; while an action waits, `Ctrl+Enter`
   * approves and `Esc` declines from anywhere in the panel; `Esc` while a run goes stops it.
   */
  import { tick } from 'svelte';
  import { CalendarClock, MessageSquarePlus, Send, Sparkles, Square, Undo2 } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { storeAssistantKey } from '$lib/ipc/corvus/imber';
  import { imberStore } from '$lib/stores/corvus/imber.svelte';
  import { imberSchedulesStore } from '$lib/stores/corvus/imber-schedules.svelte';
  import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { ImberKind } from '$lib/types/corvus/imber';
  import ImberMessage from './ImberMessage.svelte';

  const store = imberStore;
  const tab = $derived(tabsStore.activeTab);
  const conv = $derived(store.current);
  const kind = $derived<ImberKind>(conv?.kind ?? 'operate');
  const messages = $derived(store.messages);
  const pending = $derived(store.pending);
  const running = $derived(store.running);

  let draft = $state('');
  let composer = $state<HTMLTextAreaElement | undefined>(undefined);
  let thread = $state<HTMLElement | undefined>(undefined);
  let root = $state<HTMLElement | undefined>(undefined);
  let keyDraft = $state('');
  let savingKey = $state(false);

  const KINDS = [{ id: 'operate', label: 'Operate' }, { id: 'chat', label: 'Chat' }];

  const SUGGESTIONS: Record<ImberKind, string[]> = {
    operate: [
      'Write a commit message for what is staged and commit it',
      'Summarise what this branch adds over the default branch',
      'Push this branch and open a merge request',
      'Who changed the most here in the last month?',
    ],
    chat: [
      'How do I undo my last commit without losing the changes?',
      'What is the difference between merge and rebase?',
    ],
  };

  // Follow the thread while it grows, but only if the reader is already at the bottom.
  $effect(() => {
    void messages;
    const el = thread;
    if (!el) return;
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
    if (nearBottom) tick().then(() => { el.scrollTop = el.scrollHeight; });
  });

  // An approval waiting is what the user must look at: take the keys, so Ctrl+Enter / Esc reach it.
  $effect(() => { if (pending) root?.focus(); });

  // Focus the composer when the panel opens on a repository.
  $effect(() => { if (tab && store.info?.hasKey) composer?.focus(); });

  async function send(text = draft) {
    const body = text.trim();
    if (!body || running || pending) return;
    draft = '';
    await store.send(body);
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

  const status = $derived.by(() => {
    if (!conv?.running) return '';
    if (conv.retry) return `Retrying (${conv.retry.attempt}/${conv.retry.max}) in ${conv.retry.waitSecs}s…`;
    if (pending) return 'Waiting for you';
    return conv.status && conv.status !== 'thinking' ? `Running ${conv.status.replace(/^git_/, '').replace(/_/g, ' ')}…` : 'Thinking…';
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="imber" bind:this={root} tabindex="-1" role="region" aria-label="Imber" onkeydown={onKeydown}>
  <PanelShell title="Imber">
    {#snippet icon()}<Sparkles size={13} />{/snippet}
    {#snippet actions()}
      <IconButton size={22} tooltip="Undo Imber's last action" disabled={!store.canUndo || running} onclick={() => void store.undo()}>
        <Undo2 size={13} />
      </IconButton>
      <IconButton size={22} tooltip="Schedules" onclick={() => imberSchedulesStore.show()}>
        <CalendarClock size={13} />
      </IconButton>
      <IconButton size={22} tooltip="New conversation" disabled={!conv || messages.length === 0} onclick={() => void store.newConversation()}>
        <MessageSquarePlus size={13} />
      </IconButton>
    {/snippet}
    {#snippet toolbar()}
      <div class="bar">
        <Tabs items={KINDS} value={kind} size="sm" variant="segmented" onSelect={(id) => store.setKind(id as ImberKind)} ariaLabel="Conversation kind" />
        {#if kind === 'operate'}
          <Toggle
            checked={store.mode === 'auto'}
            size="sm"
            label={store.mode === 'auto' ? 'Auto' : 'Manual'}
            ariaLabel="Auto mode: run writes without asking (destructive actions still ask)"
            onchange={(on) => void store.setMode(on ? 'auto' : 'manual')}
          />
        {/if}
      </div>
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
      <div class="thread" bind:this={thread}>
        {#if messages.length === 0}
          <div class="intro">
            <EmptyState
              message={kind === 'operate' ? `Ask Imber to work on ${tab.name}` : 'Ask anything about git'}
              description={kind === 'operate'
                ? (store.mode === 'manual' ? 'Manual: every change is shown as a command you approve.' : 'Auto: changes run at once; force pushes and deletions still ask.')
                : 'A chat cannot see or change the repository.'}
            />
            <div class="ideas">
              {#each SUGGESTIONS[kind] as idea}
                <Button variant="outline" size="sm" block onclick={() => void send(idea)}>{idea}</Button>
              {/each}
            </div>
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
        {/if}
      </div>

      <div class="composer">
        {#if status}
          <div class="status"><Spinner size="xs" label={status} delay={0} /></div>
        {/if}
        <Textarea
          bind:value={draft}
          bind:element={composer}
          framed
          minRows={2}
          placeholder={kind === 'operate' ? 'Ask Imber to commit, push, branch, merge, or explain the history…' : 'Ask a question about git…'}
          disabled={running || !!pending}
          ariaLabel="Message to Imber"
          onsubmit={() => void send()}
        />
        <div class="actions">
          {#if conv && conv.costUsd > 0}<Badge variant="chip" size="sm" label={`≈ $${conv.costUsd.toFixed(3)}`} tooltip="What this conversation has cost so far, at the model's list price" />{/if}
          <span class="spacer"></span>
          {#if running}
            <Button variant="outline" size="sm" tooltip="Stop (Esc)" onclick={() => void store.cancel()}><Square size={11} /> Stop</Button>
          {:else}
            <Button variant="primary" size="sm" disabled={!draft.trim()} tooltip="Send (Ctrl+Enter)" onclick={() => void send()}><Send size={11} /> Send</Button>
          {/if}
        </div>
      </div>
    {/if}
  </PanelShell>
</div>

<style>
  .imber { height: 100%; min-height: 0; outline: none; display: flex; flex-direction: column; }
  .bar { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 4px 8px; }
  .thread { flex: 1; min-height: 0; overflow-y: auto; padding: 10px; display: flex; flex-direction: column; gap: 14px; }
  .intro { display: flex; flex-direction: column; gap: 10px; }
  .ideas { display: flex; flex-direction: column; gap: 6px; }
  .setup { display: flex; flex-direction: column; gap: 8px; padding: 10px; }
  .composer { padding: 8px; display: flex; flex-direction: column; gap: 6px; border-top: 1px solid var(--border-subtle); }
  .status { font-size: var(--font-size-xs); color: var(--text-muted); }
  .actions { display: flex; align-items: center; gap: 6px; }
  .spacer { flex: 1; }
</style>
