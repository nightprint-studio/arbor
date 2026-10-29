<script lang="ts">
  /**
   * Where you write — and where you decide how the assistant works.
   *
   * One frame holds the next message: what it will be given (the file, the selection, the instruction
   * files, as chips), the text, and one quiet line of controls. Only two choices are always in view,
   * because they are the two made most often: **which model**, and **how much it decides for itself**
   * (the mode menu — Manual, Accept edits, Autonomous; absent in a chat, which has no edits). Everything else that shapes an answer —
   * agent or chat only, how hard it thinks, how large a window, how far it may reach and what it may do —
   * is behind one settings menu, each in its own submenu with the current value on the row, so the line
   * stays short and nothing is more than two clicks away.
   */
  import {
    ArrowUp, Square, Eye, FlaskConical, ChevronDown, FileText, TextCursorInput, ShieldCheck, Zap, Lock, HardDrive,
    Gauge, MessageSquare, SlidersHorizontal, Ruler, BookText, ListTodo, WandSparkles, GitBranch, Copy, Ban, Bot, Hammer, Scale, Network,
  } from 'lucide-svelte';
  import { decisionInfo, effectiveDecisionLevel } from './assistant-decision';
  import { subagentsLabel } from './assistant-agents';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import SuggestionList from '$lib/components/shared/ui/SuggestionList.svelte';
  import VoiceMicButton from '$lib/components/shared/internal/VoiceMicButton.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { insertAtCaret } from '$lib/utils/insert-at-caret';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuWorktreeStore } from '$lib/stores/bennu/assistant-worktree.svelte';
  import { formatEur, formatPrice, formatTokens } from './assistant-format';
  import { applySlash, matchSkills, slashQuery } from './assistant-skills';
  import AssistantComposerBlocks from './AssistantComposerBlocks.svelte';
  import { createPasteHandlers } from './assistant-composer-paste';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { MODES, modeInfo } from './assistant-mode';
  import type { AccessScope, AssistantEffort } from './assistant-types';

  const store = bennuAssistantStore;
  const wt = bennuWorktreeStore;
  // What the message will carry besides its text: the file the user is in, and the lines they selected.
  const fileName = $derived(projectStore.activeFilePath?.split(/[\\/]/).pop() ?? null);
  const selected = $derived(store.selectionLines);
  let textEl = $state<HTMLTextAreaElement | undefined>();
  // Code pasted becomes a fenced block with its language; Ctrl+Shift+V pastes the text as it is.
  const paste = createPasteHandlers({
    enabled: () => store.wrapPastedCode,
    activePath: () => projectStore.activeFilePath ?? null,
    notify: (m) => toastStore.show(m, 'warning'),
  });

  // The home screen can leave the start of a sentence here and ask for the focus.
  $effect(() => {
    if (store.composerFocus === 0) return;
    textEl?.focus();
    textEl?.setSelectionRange(textEl.value.length, textEl.value.length);
  });

  // The instruction files and the skills in play follow the open project.
  $effect(() => {
    void projectStore.project?.root;
    void store.refreshInstructions();
    void store.refreshSkills();
    // Which isolated copies this project can have, for the menu below.
    const root = projectStore.project?.root;
    if (root) void wt.refreshOptions(root, store.gitEnabled);
  });

  // `/` at the start of the message offers the enabled skills; a chat has no tools, so no skills.
  let slashActive = $state(0);
  let slashDismissed = $state(false);
  const slash = $derived(store.chatOnly ? null : slashQuery(store.draft));
  const slashItems = $derived(
    slash ? matchSkills(store.enabledSkills, slash.query).map((s) => ({ id: s.id, label: `/${s.name}`, description: s.description, meta: s.scope === 'project' ? 'Project' : 'User' })) : [],
  );
  const slashOpen = $derived(!slashDismissed && slashItems.length > 0);
  // A new word or a new match list starts again from the first row, and un-dismisses.
  $effect(() => {
    void slash?.query;
    slashActive = 0;
    slashDismissed = false;
  });

  function pickSlash(item: { id: string }) {
    const skill = store.enabledSkills.find((s) => s.id === item.id);
    if (!skill) return;
    store.draft = applySlash(store.draft, skill.name);
    textEl?.focus();
  }

  // Arrows walk the list, Enter or Tab picks, Esc closes it — all while the text field keeps the focus.
  function onFieldKeydown(e: KeyboardEvent) {
    if (!slashOpen || e.ctrlKey || e.metaKey || e.altKey) return;
    const n = slashItems.length;
    if (e.key === 'ArrowDown') { e.preventDefault(); slashActive = (slashActive + 1) % n; }
    else if (e.key === 'ArrowUp') { e.preventDefault(); slashActive = (slashActive - 1 + n) % n; }
    else if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); pickSlash(slashItems[slashActive]); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); slashDismissed = true; }
  }

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

  // One icon per mode; the dangerous-looking colour is reserved for the one that opens files on its own.
  const MODE_ICON = { manual: ShieldCheck, accept_edits: Zap, autonomous: Bot } as const;
  const modeItems = $derived<DropdownItem[]>(
    MODES.map((m) => ({
      kind: 'item' as const, id: m.id, label: m.label, subtitle: m.description, icon: MODE_ICON[m.id],
      danger: m.tone === 'warning', active: store.mode === m.id, onclick: () => store.setMode(m.id),
    })),
  );
  const current = $derived(modeInfo(store.mode));
  const ModeIcon = $derived(MODE_ICON[store.mode]);

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

  // `.by` so the three items declared below it are read when the menu is built, not while this is declared.
  const settingsItems = $derived.by<DropdownItem[]>(() => [
    {
      kind: 'submenu', id: 'effort', label: `Effort · ${effortLabel || 'not available'}`, icon: Gauge, disabled: store.effortLevels.length === 0,
      items: EFFORTS.filter((e) => store.effortLevels.includes(e.id)).map((e) => ({
        kind: 'item' as const, id: e.id, label: e.label, subtitle: e.why, active: store.effort === e.id, onclick: () => store.setEffort(e.id),
      })),
    },
    {
      kind: 'item', id: 'skills', label: 'Skills…', icon: WandSparkles,
      subtitle: `${store.skillCounts.on} of ${store.skillCounts.total} enabled · type / to use one`,
      onclick: () => store.openSkills(),
    },
    {
      kind: 'item', id: 'plan', label: 'Step plan', icon: ListTodo, active: store.planEnabled,
      subtitle: store.planEnabled ? 'Shows its steps and progress on multi-step work' : 'Off — no plan, a little fewer tokens',
      onclick: () => store.setPlanEnabled(!store.planEnabled),
    },
    {
      kind: 'item', id: 'decisions', label: `Decisions · ${decisionInfo(effectiveDecisionLevel(store.chatDecisionLevel, store.decisionLevel)).label}`, icon: Scale,
      subtitle: store.chatDecisionLevel || store.chatDecisionNotes ? 'This conversation has its own settings' : 'Follows the settings',
      onclick: () => store.openDecision(),
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
          copyItem,
          buildItem,
          subagentsItem,
          {
            kind: 'submenu', id: 'may', label: 'Permissions', icon: Lock,
            items: [
              { kind: 'item', id: 'edit', label: 'Edit files', subtitle: 'Through Bennu’s tools, never a shell', active: store.allowEdits, onclick: () => store.setAllowEdits(!store.allowEdits) },
              { kind: 'item', id: 'temp', label: 'Use the Temp folders', subtitle: 'Scratch files outside the project', active: store.allowTemp, onclick: () => store.setAllowTemp(!store.allowTemp) },
              { kind: 'item', id: 'delete', label: 'Delete — by mode', subtitle: 'Free in Temp; in scope free unless Manual; outside it always asks', icon: Lock, disabled: true, onclick: () => {} },
            ],
          },
        ] as DropdownItem[])),
  ]);

  // The isolated copy: a choice for a new conversation, fixed once it has started (like chat only).
  const copyLocked = $derived(!!store.active && (store.active.messages.length > 0 || !!store.active.worktree));
  const copyLabel = $derived(wt.kind === 'git' ? 'Git worktree' : wt.kind === 'copy' ? 'Copy' : 'Off');
  const copyItem = $derived<DropdownItem>({
    kind: 'submenu', id: 'copy', label: `Isolated copy · ${copyLocked ? (store.active?.worktree ? 'in use' : 'off') : copyLabel}`, icon: GitBranch,
    items: copyLocked
      ? [{ kind: 'item', id: 'locked', label: 'Fixed when the conversation starts', subtitle: 'Start a new conversation to change it', icon: Lock, disabled: true, onclick: () => {} }]
      : [
          { kind: 'item', id: 'off', label: 'Off', subtitle: 'Work on the project itself', icon: Ban, active: wt.kind === null, onclick: () => wt.setKind(null) },
          {
            kind: 'item', id: 'copy', label: 'Copy — text only', icon: Copy, active: wt.kind === 'copy',
            subtitle: wt.options?.copy.available === false ? wt.options.copy.reason : 'Files, search, build and run; no code intelligence. Cheap, not indexed',
            disabled: wt.options?.copy.available === false, onclick: () => wt.setKind('copy'),
          },
          {
            kind: 'item', id: 'git', label: 'Git worktree — indexed', icon: GitBranch, active: wt.kind === 'git',
            subtitle: wt.options?.git.available === false ? wt.options.git.reason : 'A real worktree, indexed: every tool works on it',
            disabled: wt.options?.git.available === false, onclick: () => wt.setKind('git'),
          },
        ],
  });

  // Build isolation: queue behind the others (default) or compile in a target of its own. Fixed at the start.
  const buildLocked = $derived(!!store.active && store.active.messages.length > 0);
  const buildSeparate = $derived(buildLocked ? store.active?.buildIsolation === 'separate' : store.buildIsolation === 'separate');
  const buildItem = $derived<DropdownItem>({
    kind: 'submenu', id: 'build', label: `Build isolation · ${buildSeparate ? 'separate target' : 'shared (queue)'}`, icon: Hammer,
    items: buildLocked
      ? [{ kind: 'item', id: 'locked', label: 'Fixed when the conversation starts', subtitle: 'Start a new conversation to change it', icon: Lock, disabled: true, onclick: () => {} }]
      : [
          { kind: 'item', id: 'shared', label: 'Shared (queue)', subtitle: 'Builds wait their turn behind the other conversations and the Build button', active: !buildSeparate, onclick: () => store.setBuildIsolation('shared') },
          { kind: 'item', id: 'separate', label: 'Separate target', subtitle: 'Cargo only: its own target folder — more disk and a cold first compile, no waiting', active: buildSeparate, onclick: () => store.setBuildIsolation('separate') },
        ],
  });

  // Sub-agents for this conversation: follow the setting, or switch them on or off here. Changeable at any time.
  const subagentsItem = $derived<DropdownItem>({
    kind: 'submenu', id: 'subagents', label: `Sub-agents · ${subagentsLabel(store.subagentsEnabled, store.chatSubagents)}`, icon: Network,
    items: [
      { kind: 'item', id: 'inherit', label: 'Follow the settings', subtitle: store.subagentsEnabled ? 'On in the settings' : 'Off in the settings', active: store.chatSubagents === '', onclick: () => store.setChatSubagents('') },
      { kind: 'item', id: 'on', label: 'On', subtitle: 'Imber may start helpers with a clean context', active: store.chatSubagents === 'on', onclick: () => store.setChatSubagents('on') },
      { kind: 'item', id: 'off', label: 'Off', subtitle: 'No sub-agents in this conversation', active: store.chatSubagents === 'off', onclick: () => store.setChatSubagents('off') },
    ],
  });

  const settingsTip = $derived(
    store.chatOnly
      ? `Chat only · ${effortLabel || 'no effort'} · ${formatTokens(store.maxContext)}`
      : `${effortLabel || 'no effort'} · ${formatTokens(store.maxContext)} · ${scopeLabel}`,
  );

  const canSend = $derived(store.draft.trim().length > 0);

  // Dictated text lands where the caret is (or replaces the selection), then the caret moves past it.
  function insertDictation(text: string) {
    const start = textEl?.selectionStart ?? store.draft.length;
    const end = textEl?.selectionEnd ?? store.draft.length;
    const { value, caret } = insertAtCaret(store.draft, start, end, text);
    store.draft = value;
    queueMicrotask(() => { textEl?.focus(); textEl?.setSelectionRange(caret, caret); });
  }

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

    <!-- Keys are handled on the way up from the textarea, which keeps the focus. -->
    <AssistantComposerBlocks bind:draft={store.draft} />

    <div class="field" role="presentation" onkeydown={(e) => { paste.onkeydown(e); onFieldKeydown(e); }} onpaste={paste.onpaste}>
      {#if slashOpen}
        <div class="slash">
          <SuggestionList items={slashItems} active={slashActive} id="asst-slash" ariaLabel="Skills" onpick={pickSlash} />
        </div>
      {/if}
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

      <VoiceMicButton onText={insertDictation} onSetup={() => bennuUiStore.openSettings('voice')} />

      <span class="grow"></span>

      {#if !store.chatOnly}
        <Dropdown items={modeItems} position="fixed" direction="up" width="340px">
          {#snippet trigger({ open, toggle })}
            <Button
              variant="ghost" size="xs" ariaExpanded={open} ariaHaspopup="menu"
              color={current.tone === 'warning' ? 'var(--warning)' : undefined}
              tooltip={{ content: `Mode — ${current.label}`, description: current.description }} onclick={toggle}
            >
              <span class="lbl"><ModeIcon size={12} />{current.label}<ChevronDown size={11} /></span>
            </Button>
          {/snippet}
        </Dropdown>
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
      {:else if store.mode === 'autonomous'}<Bot size={10} />Edits are applied; files its own build reports an error in are opened for you.
      {:else if store.mode === 'accept_edits'}<Zap size={10} />Edits are applied as they are made, inside what you allowed.
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
  .field { position: relative; min-width: 0; }
  /* Opens upward, over the thread: the composer sits at the bottom of the panel. */
  .slash { position: absolute; left: -4px; right: -4px; bottom: calc(100% + 8px); z-index: 20; }

  .controls { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .grow { flex: 1; min-width: 4px; }
  /* An icon and its text sit apart; the buttons they are in do not space their own children. */
  .lbl { display: inline-flex; align-items: center; gap: 6px; }
  .model-dot {
    width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
    background: linear-gradient(140deg, var(--accent), var(--info));
  }

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
