<script lang="ts">
  /**
   * Where you write — and where you decide how the assistant works.
   *
   * One frame holds the next message: what it will be given (the file, the selection, the instruction
   * files, as chips), the text, and one quiet line of controls. Only two choices are always in view,
   * because they are the two made most often: **which model**, and whether edits go in **on their own**
   * (the Auto switch — absent in a chat, which has no edits). Everything else that shapes an answer —
   * agent or chat only, how hard it thinks, how large a window, how far it may reach and what it may do —
   * is behind one settings menu, each in its own submenu with the current value on the row, so the line
   * stays short and nothing is more than two clicks away.
   */
  import {
    ArrowUp, Square, Eye, FlaskConical, ChevronDown, FileText, TextCursorInput, ShieldCheck, Zap, Lock, HardDrive,
    Gauge, MessageSquare, SlidersHorizontal, Ruler, BookText, ListTodo,
  } from 'lucide-svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { formatEur, formatPrice, formatTokens } from './assistant-format';
  import type { AccessScope, AssistantEffort } from './assistant-types';

  const store = bennuAssistantStore;
  // What the message will carry besides its text: the file the user is in, and the lines they selected.
  const fileName = $derived(projectStore.activeFilePath?.split(/[\\/]/).pop() ?? null);
  const selected = $derived(store.selectionLines);
  let textEl = $state<HTMLTextAreaElement | undefined>();

  // The home screen can leave the start of a sentence here and ask for the focus.
  $effect(() => {
    if (store.composerFocus === 0) return;
    textEl?.focus();
    textEl?.setSelectionRange(textEl.value.length, textEl.value.length);
  });

  // The instruction files in play follow the open project.
  $effect(() => {
    void projectStore.project?.root;
    void store.refreshInstructions();
  });

  const modelItems = $derived<DropdownItem[]>(
    store.models.map((m) => ({
      kind: 'item',
      id: m.id,
      label: m.name,
      subtitle: `${m.tagline} · ${formatTokens(m.contextMax)} window`,
      meta: formatPrice(m.inputPrice, m.outputPrice),
      active: m.id === store.model.id,
      onclick: () => store.setModel(m.id),
    })),
  );

  // What the assistant may reach, narrowest first. The wider ones say what they cost: it is a choice
  // you make with the consequence in front of you, not a setting you find later.
  const SCOPES: { id: AccessScope; label: string; short: string; why: string }[] = [
    { id: 'file', label: 'This file', short: 'File', why: 'Only the file you are in' },
    { id: 'project', label: 'This project', short: 'Project', why: 'Every file and Bennu tool of the project' },
    { id: 'workspace', label: 'All open projects', short: 'Workspace', why: 'Across the projects in this workspace' },
    { id: 'disk', label: 'Anywhere on disk', short: 'Disk', why: 'No prompts — everything but secrets and deletions' },
  ];
  const scopeLabel = $derived(SCOPES.find((s) => s.id === store.scope)?.short ?? '');

  const EFFORTS: { id: AssistantEffort; label: string; why: string }[] = [
    { id: 'low', label: 'Low', why: 'Quick answers, few tools — for simple questions' },
    { id: 'medium', label: 'Medium', why: 'Balanced; most everyday edits' },
    { id: 'high', label: 'High', why: 'Thorough; the default' },
    { id: 'xhigh', label: 'Extra high', why: 'For hard, multi-file work' },
    { id: 'max', label: 'Max', why: 'Longest thinking, highest cost' },
  ];
  const effortLabel = $derived(EFFORTS.find((e) => e.id === store.effort)?.label ?? '');

  // A size the model cannot hold is listed but cannot be picked, so the limit is visible instead of
  // making a size silently vanish when the model changes.
  const sizeItems = $derived<DropdownItem[]>(
    store.contextSizes.map((s) => ({
      kind: 'item' as const, id: String(s), label: formatTokens(s),
      disabled: s > store.model.contextMax, active: s === store.maxContext, onclick: () => store.setMaxContext(s),
    })),
  );

  const settingsItems = $derived<DropdownItem[]>([
    {
      kind: 'submenu', id: 'effort', label: `Effort · ${effortLabel || 'not available'}`, icon: Gauge, disabled: store.effortLevels.length === 0,
      items: EFFORTS.filter((e) => store.effortLevels.includes(e.id)).map((e) => ({
        kind: 'item' as const, id: e.id, label: e.label, subtitle: e.why, active: store.effort === e.id, onclick: () => store.setEffort(e.id),
      })),
    },
    {
      kind: 'item', id: 'plan', label: 'Step plan', icon: ListTodo, active: store.planEnabled,
      subtitle: store.planEnabled ? 'Shows its steps and progress on multi-step work' : 'Off — no plan, a little fewer tokens',
      onclick: () => store.setPlanEnabled(!store.planEnabled),
    },
    { kind: 'submenu', id: 'window', label: `Context window · ${formatTokens(store.maxContext)}`, icon: Ruler, items: sizeItems },
    ...(store.chatOnly
      ? []
      : ([
          { kind: 'separator' },
          {
            kind: 'submenu', id: 'reach', label: `Reach · ${scopeLabel}`, icon: ShieldCheck,
            items: SCOPES.map((s) => ({
              kind: 'item' as const, id: s.id, label: s.label, subtitle: s.why,
              icon: s.id === 'disk' ? HardDrive : undefined, danger: s.id === 'disk',
              active: store.scope === s.id, onclick: () => store.setScope(s.id),
            })),
          },
          {
            kind: 'submenu', id: 'may', label: 'Permissions', icon: Lock,
            items: [
              { kind: 'item', id: 'edit', label: 'Edit files', subtitle: 'Through Bennu’s tools, never a shell', active: store.allowEdits, onclick: () => store.setAllowEdits(!store.allowEdits) },
              { kind: 'item', id: 'temp', label: 'Use the Temp folders', subtitle: 'Scratch files outside the project', active: store.allowTemp, onclick: () => store.setAllowTemp(!store.allowTemp) },
              { kind: 'item', id: 'delete', label: 'Delete — always asks', subtitle: 'Not a setting: every deletion needs a yes', icon: Lock, disabled: true, onclick: () => {} },
            ],
          },
        ] as DropdownItem[])),
  ]);

  const settingsTip = $derived(
    store.chatOnly
      ? `Chat only · ${effortLabel || 'no effort'} · ${formatTokens(store.maxContext)}`
      : `${effortLabel || 'no effort'} · ${formatTokens(store.maxContext)} · ${scopeLabel}`,
  );

  const canSend = $derived(store.draft.trim().length > 0);

  async function send() {
    if (!canSend || store.running) return;
    // A dry run shows the request that would go and keeps the draft, so it can be sent for real next.
    const text = store.draft;
    const consumed = await store.send(text);
    if (consumed && store.draft === text) store.draft = '';
  }
</script>

<div class="composer">
  <div class="frame" onfocusin={() => store.refreshSelection()}>
    <div class="chips">
      {#if fileName && !store.chatOnly}
        <Badge variant="chip" size="sm" label={fileName}>
          {#snippet icon()}<FileText size={10} />{/snippet}
        </Badge>
      {/if}
      {#if selected > 0}
        <Badge variant="chip" size="sm" label={selected === 1 ? '1 line selected' : `${selected} lines selected`}>
          {#snippet icon()}<TextCursorInput size={10} />{/snippet}
        </Badge>
      {/if}
      {#each store.instructions as file (file.name)}
        <Badge variant="chip" size="sm" label={file.name}>
          {#snippet icon()}<BookText size={10} />{/snippet}
        </Badge>
      {/each}
    </div>

    <div class="field">
      <Textarea
        bind:value={store.draft}
        bind:element={textEl}
        placeholder={store.chatOnly ? 'Ask anything…' : 'Ask about this project, or tell it what to change…'}
        minRows={2}
        maxRows={9}
        onsubmit={send}
      />
    </div>

    <div class="controls">
      <Dropdown items={modelItems} position="fixed" direction="up" width="300px">
        {#snippet trigger({ open, toggle })}
          <Button variant="ghost" size="xs" ariaExpanded={open} ariaHaspopup="menu" tooltip="Model" onclick={toggle}>
            <span class="lbl"><i class="model-dot"></i>{store.model.name}<ChevronDown size={11} /></span>
          </Button>
        {/snippet}
      </Dropdown>

      <Dropdown items={settingsItems} position="fixed" direction="up" width="270px">
        {#snippet trigger({ open, toggle })}
          <IconButton tooltip={`Settings — ${settingsTip}`} size={24} active={open} ariaHasPopup ariaExpanded={open} onclick={toggle}>
            <SlidersHorizontal size={14} />
          </IconButton>
        {/snippet}
      </Dropdown>

      <span class="grow"></span>

      {#if !store.chatOnly}
        <span class="auto" title={store.mode === 'auto' ? 'Edits are applied as they are made' : 'You approve each edit before it is written'}>
          <Toggle size="sm" label="Auto" checked={store.mode === 'auto'} onchange={(v) => store.setMode(v ? 'auto' : 'manual')} ariaLabel="Apply edits automatically" />
        </span>
      {/if}

      {#if store.running}
        <span class="asst-ring"><IconButton tooltip="Stop" variant="accent" size={28} onclick={() => store.cancel()}><Square size={12} /></IconButton></span>
      {:else}
        <IconButton
          tooltip={store.dryRun ? 'Preview the request — dry run is on' : 'Send'}
          shortcut="Ctrl+Enter" variant="accent" size={28} disabled={!canSend} onclick={send}
        >
          {#if store.dryRun}<Eye size={15} />{:else}<ArrowUp size={15} />{/if}
        </IconButton>
      {/if}
    </div>
  </div>

  <div class="foot">
    <span class="hint">
      {#if store.dryRun}<FlaskConical size={10} />Dry run — send shows the request instead of making it.
      {:else if store.chatOnly}<MessageSquare size={10} />It only talks: it cannot read or change anything.
      {:else if store.mode === 'auto'}<Zap size={10} />Edits are applied as they are made.
      {:else}<ShieldCheck size={10} />You approve every edit before it is written.{/if}
    </span>
    {#if store.pressing}
      <span class="limit" class:reached={store.pressing.level === 'reached'} title={store.spend?.warning ?? ''}>
        {store.pressing.level === 'reached' ? 'Limit reached' : `${store.pressing.percent}% of ${store.pressing.period} limit`}
      </span>
    {/if}
    {#if store.active}<span class="cost" title="What this conversation has cost">{formatEur(store.eur(store.active.cost))}</span>{/if}
  </div>
</div>

<style>
  .composer { display: flex; flex-direction: column; gap: 6px; padding: 8px 10px 8px; }

  .frame {
    display: flex; flex-direction: column; gap: 8px;
    padding: 9px 10px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-elevated);
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .frame:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-subtle);
  }

  .chips { display: flex; align-items: center; flex-wrap: wrap; gap: 5px; }
  .chips:empty { display: none; }
  .field { min-width: 0; }

  .controls { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .grow { flex: 1; min-width: 4px; }
  /* An icon and its text sit apart; the buttons they are in do not space their own children. */
  .lbl { display: inline-flex; align-items: center; gap: 6px; }
  .model-dot {
    width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
    background: linear-gradient(140deg, var(--accent), var(--info));
  }
  .auto { display: inline-flex; align-items: center; margin-right: 4px; }

  .foot { display: flex; align-items: center; gap: 8px; padding: 0 4px; min-height: 16px; }
  .hint {
    display: inline-flex; align-items: center; gap: 6px; min-width: 0;
    font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .limit { margin-left: auto; flex-shrink: 0; font-size: var(--font-size-2xs); color: var(--warning); }
  .limit.reached { color: var(--danger); font-weight: 600; }
  .limit + .cost { margin-left: 8px; }
  .cost { margin-left: auto; flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
</style>
