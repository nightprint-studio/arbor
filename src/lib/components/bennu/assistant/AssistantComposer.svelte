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
    ArrowUp, Eye, FlaskConical, ChevronDown, FileText, TextCursorInput, ShieldCheck, Zap, Lock, HardDrive,
    Gauge, MessageSquare, SlidersHorizontal, Ruler, BookText, ListTodo, WandSparkles, GitBranch, Copy, Ban, Bot, Hammer, Scale, Network, Paperclip, Infinity as InfinityIcon,
  } from 'lucide-svelte';
  import { decisionInfo, effectiveDecisionLevel } from './assistant-decision';
  import { subagentsLabel } from './assistant-agents';
  import ImberComposerFrame from '$lib/components/shared/internal/imber/ImberComposerFrame.svelte';
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
  import AssistantAttachments from './AssistantAttachments.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { assistantAttachmentsStore as files, MAX_ATTACHMENTS } from '$lib/stores/bennu/assistant-attachments.svelte';
  import { pastedPictures, droppedFiles, dragHasFiles } from './assistant-attach-paste';
  import { createPasteHandlers } from './assistant-composer-paste';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import AssistantNote from './AssistantNote.svelte';
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
    // Backspace in an empty field takes back the last file, the way it takes back the last character.
    if (e.key === 'Backspace' && files.count > 0 && store.draft === '' && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      files.removeLast();
      return;
    }
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
            // On or off said three ways — the word on the right, the tinted icon, the tick — because a run with no step
            // limit is the one setting here that changes how long it goes on, and a tick alone was easy to miss.
            kind: 'item', id: 'non-stop', label: 'Non-stop', icon: InfinityIcon, active: store.chatNonStop,
            iconColor: store.chatNonStop ? 'var(--warning)' : undefined,
            meta: store.chatNonStop ? 'On' : 'Off', metaColor: store.chatNonStop ? 'var(--warning)' : undefined,
            subtitle: store.chatNonStop ? 'No step limit in this conversation — the spending limits still stop it' : `This conversation stops after ${store.maxSteps} steps`,
            onclick: () => store.setChatNonStop(!store.chatNonStop),
          },
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
      : `${effortLabel || 'no effort'} · ${formatTokens(store.maxContext)} · ${scopeLabel}${store.chatNonStop ? ' · non-stop' : ''}`,
  );

  const canSend = $derived(store.draft.trim().length > 0 || files.count > 0);

  // Attaching: the explorer in its picker mode (never the system dialog), opened by the clip or the palette.
  let pickerOpen = $state(false);
  let seenPickerRequest = files.pickerRequest;
  $effect(() => {
    const request = files.pickerRequest;
    if (request === seenPickerRequest) return;
    seenPickerRequest = request;
    pickerOpen = true;
  });

  function attachPicked(paths: string[]) {
    pickerOpen = false;
    const left = files.addPaths(paths);
    if (left > 0) toastStore.show(`At most ${MAX_ATTACHMENTS} files go with one message — ${left} left out.`, 'warning');
    textEl?.focus();
  }

  /** Pictures on the clipboard become attachments; returns whether the paste was one (so it is not also text). */
  function attachPasted(e: ClipboardEvent): boolean {
    const found = pastedPictures(e);
    if (found.length === 0) return false;
    e.preventDefault();
    void Promise.all(found).then((pictures) => {
      for (const p of pictures) {
        if (!files.addPasted(p.name, p.base64, p.size, p.preview)) {
          toastStore.show(`At most ${MAX_ATTACHMENTS} files go with one message.`, 'warning');
          break;
        }
      }
    }).catch(() => toastStore.show('Could not read the pasted picture.', 'warning'));
    return true;
  }

  // Files dropped on the composer. The system drag-drop handler is off app-wide, so the DOM events arrive; a
  // dropped file has no path, so it is read whole and travels as bytes like a pasted picture.
  let dropping = $state(false);

  function onDragOver(e: DragEvent) {
    if (!dragHasFiles(e)) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    dropping = true;
  }

  function onDrop(e: DragEvent) {
    dropping = false;
    if (!dragHasFiles(e)) return;
    e.preventDefault();
    const { read, tooLarge } = droppedFiles(e);
    if (tooLarge.length > 0) toastStore.show(`Too large to attach: ${tooLarge.join(', ')}.`, 'warning');
    void Promise.all(read).then((dropped) => {
      let left = 0;
      for (const f of dropped) {
        if (!files.addData(f.name, f.base64, f.size, f.kind, f.preview)) left += 1;
      }
      if (left > 0) toastStore.show(`At most ${MAX_ATTACHMENTS} files go with one message — ${left} left out.`, 'warning');
      textEl?.focus();
    }).catch(() => toastStore.show('Could not read the dropped file.', 'warning'));
  }

  // Dictated text lands where the caret is (or replaces the selection), then the caret moves past it.
  function insertDictation(text: string) {
    const start = textEl?.selectionStart ?? store.draft.length;
    const end = textEl?.selectionEnd ?? store.draft.length;
    const { value, caret } = insertAtCaret(store.draft, start, end, text);
    store.draft = value;
    queueMicrotask(() => { textEl?.focus(); textEl?.setSelectionRange(caret, caret); });
  }

  async function send() {
    if (!canSend) return;
    // While it works, the text goes to the run as a note it reads after the current step. Files wait for the
    // next ordinary message: a note is words only.
    if (store.running) {
      const text = store.draft;
      if (!text.trim()) return;
      if (files.count > 0) toastStore.show('Files go with your next message, once Imber has finished.', 'info');
      if (await store.sendNote(text)) {
        if (store.draft === text) store.draft = '';
      } else {
        toastStore.show('Imber has just finished: send it again.', 'info');
      }
      return;
    }
    // A dry run shows the request that would go and keeps the draft, so it can be sent for real next.
    const text = store.draft;
    const consumed = await store.send(text, { attachments: files.snapshot() });
    if (consumed && store.draft === text) store.draft = '';
    if (consumed) files.clear();
  }
</script>

{#if store.pendingNotes.length}
  <div class="pending-notes">
    {#each store.pendingNotes as n (n.id)}<AssistantNote text={n.text} pending />{/each}
  </div>
{/if}

<ImberComposerFrame
  bind:value={store.draft}
  bind:element={textEl}
  placeholder={store.chatOnly ? 'Ask anything…' : 'Ask about this project, or tell it what to change…'}
  running={store.running}
  sendWhileRunning
  {canSend}
  sendTooltip={store.dryRun ? 'Preview the request — dry run is on' : 'Send'}
  {dropping}
  onSend={send}
  onStop={() => store.cancel()}
  onfocusin={() => store.refreshSelection()}
  ondragover={onDragOver}
  ondragleave={() => (dropping = false)}
  ondrop={onDrop}
  onfieldkeydown={(e) => { paste.onkeydown(e); onFieldKeydown(e); }}
  onfieldpaste={(e) => { if (!attachPasted(e)) paste.onpaste(e); }}
>
  {#snippet chips()}
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
  {/snippet}

  {#snippet attachments()}
    {#if files.count > 0}
      <AssistantAttachments items={files.items} onRemove={(i) => files.remove(i)} />
    {/if}
  {/snippet}

  {#snippet beforeField()}
    <AssistantComposerBlocks bind:draft={store.draft} />
  {/snippet}

  {#snippet popup()}
    {#if slashOpen}
      <SuggestionList items={slashItems} active={slashActive} id="asst-slash" ariaLabel="Skills" onpick={pickSlash} />
    {/if}
  {/snippet}

  {#snippet controls()}
    <Dropdown items={modelItems} position="fixed" direction="up" width="300px">
      {#snippet trigger({ open, toggle })}
        <Button variant="ghost" size="sm" ariaExpanded={open} ariaHaspopup="menu" tooltip="Model" onclick={toggle}>
          <span class="lbl"><i class="model-dot"></i>{store.model.name}<ChevronDown size={12} /></span>
        </Button>
      {/snippet}
    </Dropdown>

    <Dropdown items={settingsItems} position="fixed" direction="up" width="270px">
      {#snippet trigger({ open, toggle })}
        <IconButton tooltip={`Settings — ${settingsTip}`} size={24} active={open} ariaHasPopup ariaExpanded={open} onclick={toggle}>
          <span class="settings-ico">
            <SlidersHorizontal size={14} />
            <!-- Seen without opening the menu: this conversation runs with no step limit. -->
            {#if store.chatNonStop && !store.chatOnly}<span class="nonstop-mark"><InfinityIcon size={9} strokeWidth={2.6} /></span>{/if}
          </span>
        </IconButton>
      {/snippet}
    </Dropdown>

    <IconButton
      tooltip={files.full ? `At most ${MAX_ATTACHMENTS} files` : 'Attach files — pictures or text (or paste a picture)'}
      size={24} disabled={files.full} onclick={() => (pickerOpen = true)}
    >
      <Paperclip size={14} />
    </IconButton>

    <VoiceMicButton onText={insertDictation} onSetup={() => bennuUiStore.openSettings('voice')} />
  {/snippet}

  {#snippet trailing()}
    {#if !store.chatOnly}
      <Dropdown items={modeItems} position="fixed" direction="up" width="340px">
        {#snippet trigger({ open, toggle })}
          <!-- Only the icon takes the warning colour: the word stays readable, the risk still shows. -->
          <Button
            variant="ghost" size="sm" ariaExpanded={open} ariaHaspopup="menu"
            tooltip={{ content: `Mode — ${current.label}`, description: current.description }} onclick={toggle}
          >
            <span class="lbl"><span class="mode-ico" class:warn={current.tone === 'warning'}><ModeIcon size={13} /></span>{current.label}<ChevronDown size={12} /></span>
          </Button>
        {/snippet}
      </Dropdown>
    {/if}
  {/snippet}

  {#snippet sendIcon()}
    {#if store.dryRun}<Eye size={15} />{:else}<ArrowUp size={15} />{/if}
  {/snippet}

  {#snippet hint()}
    {#if store.dryRun}<FlaskConical size={10} />Dry run — send shows the request instead of making it.
    {:else if store.chatOnly}<MessageSquare size={10} />It only talks: it cannot read or change anything.
    {:else if store.mode === 'autonomous'}<Bot size={10} />Edits are applied; files its own build reports an error in are opened for you.
    {:else if store.mode === 'accept_edits'}<Zap size={10} />Edits are applied as they are made, inside what you allowed.
    {:else}<ShieldCheck size={10} />You approve every edit before it is written.{/if}
  {/snippet}

  {#snippet figures()}
    {#if store.pressing}
      <span class="limit" class:reached={store.pressing.level === 'reached'} title={store.spend?.warning ?? ''}>
        {store.pressing.level === 'reached' ? 'Limit reached' : `${store.pressing.percent}% of ${store.pressing.period} limit`}
      </span>
    {/if}
    {#if store.active}<span class="cost" class:live={store.running} title={store.running ? 'What this conversation has cost — updated with each request while the run goes' : 'What this conversation has cost'}>{formatEur(store.eur(store.active.cost))}</span>{/if}
  {/snippet}
</ImberComposerFrame>

{#if pickerOpen}
  <FileExplorerModal
    mode="file"
    multiple
    title="Attach to Imber"
    initialPath={projectStore.project?.root}
    onConfirm={(path) => attachPicked([path])}
    onConfirmMulti={attachPicked}
    onCancel={() => (pickerOpen = false)}
  />
{/if}

<style>
  .pending-notes { display: flex; flex-direction: column; gap: 6px; margin-bottom: 6px; }
  /* An icon and its text sit apart; the buttons they are in do not space their own children. */
  .lbl { display: inline-flex; align-items: center; gap: 6px; }
  .model-dot {
    width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
    background: linear-gradient(140deg, var(--accent), var(--info));
  }
  .limit { margin-left: auto; flex-shrink: 0; font-size: var(--font-size-2xs); color: var(--warning); }
  .limit.reached { color: var(--danger); font-weight: 600; }
  .limit + .cost { margin-left: 8px; }
  .mode-ico { display: inline-flex; }
  .settings-ico { position: relative; display: inline-flex; }
  .nonstop-mark {
    position: absolute; right: -6px; bottom: -5px; display: inline-flex; padding: 0 1px;
    border-radius: var(--radius-sm); background: var(--bg-elevated); color: var(--warning);
  }
  .mode-ico.warn { color: var(--warning); }
  .cost.live { color: var(--accent); }
  .cost { margin-left: auto; flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
</style>
