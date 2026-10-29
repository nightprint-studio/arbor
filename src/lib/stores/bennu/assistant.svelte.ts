/**
 * The assistant panel's state — conversations, the choices around them, and the run in progress.
 *
 * The conversations live in `bennu-be` (a file each); this store holds what the screen shows of them
 * and drives a run: it sends, listens to the events the run streams back, and relays the user's
 * answers to the questions the run is waiting on. The choices that shape an answer (model, window,
 * effort, mode, reach) are the `assistant` section of Bennu's `config.toml` — read when the window
 * loads, written back when one changes — never `localStorage`.
 *
 * The assistant message that is being written is replaced wholesale by each `message` event rather
 * than patched: the backend builds it in the panel's own shape, so there is no second reducer here to
 * disagree with the one that saves it.
 */

import { totalTokens } from '$lib/components/bennu/assistant/assistant-format';
import type {
  AccessAnswer, AccessScope, AssistantEffort, AssistantMessage, AssistantMode, AssistantModel, Conversation,
  EditProposal, MessageBlock, ProposalStatus,
} from '$lib/components/bennu/assistant/assistant-types';
import {
  cancelAssistantRun, decideProposal, deleteConversation, listConversations, loadConversation, onAssistantEvent,
  compactConversation, getInstructions, getSpend, listModels, renameConversation, replyToAssistant, sendAssistantMessage, type AssistantEvent, type RequestPreview, type SpendMeter, type SpendStatus,
} from '$lib/ipc/bennu/assistant';
import { bennuConfigStore } from './config.svelte';
import type { BennuAssistantLimits } from '$lib/ipc/bennu/config';
import { periodStarts } from '$lib/components/bennu/assistant/assistant-periods';
import type { Activity } from '$lib/components/bennu/assistant/assistant-activity';
import { projectStore } from './project.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

/** What the window gives the store from the editor: the text selected in it. */
export interface AssistantEditorApi {
  selectedText: () => string;
}

/** The context sizes the picker offers; a model's own maximum decides which are available. */
const CONTEXT_SIZES = [50_000, 100_000, 200_000, 500_000, 1_000_000];

/** Stands in for the chosen model until the list has loaded, so nothing reads a model that is not there. */
function placeholderModel(id: string): AssistantModel {
  return { id, name: id, tagline: '', contextMax: 1_000_000, efforts: [], inputPrice: 0, outputPrice: 0 };
}

function newConversationId(): string {
  return `c-${Date.now().toString(16)}`;
}

function emptyConversation(model: string, chat: boolean): Conversation {
  return {
    id: newConversationId(), title: 'New conversation', updatedAt: Date.now(), model, messages: [], cost: 0, chat,
    // Every request carries the instructions and the tool definitions, so even an empty conversation
    // is not at zero.
    context: { system: 6_200, project: 0, conversation: 0 },
  };
}

function createAssistantStore() {
  let conversations = $state<Conversation[]>([]);
  let activeId = $state<string | null>(null);
  /** Conversations whose messages have been fetched (the list only carries headers). */
  let loaded = $state<Set<string>>(new Set());

  // The models come from the backend: a price list the user can edit, so a price change is not a release.
  let models = $state<AssistantModel[]>([]);
  let modelsFile = $state('');
  let modelsError = $state<string | null>(null);
  let modelId = $state('claude-opus-5-5');
  let maxContext = $state(200_000);
  let mode = $state<AssistantMode>('manual');
  // What it may reach. The default is the project it is working in — wide enough to be useful, narrow
  // enough that a wrong turn stays in one place. Wider is a choice, and deleting is never a setting:
  // it always asks.
  let scope = $state<AccessScope>('project');
  let effort = $state<AssistantEffort>('high');
  let allowEdits = $state(true);
  let allowTemp = $state(true);
  let extraFolders = $state<string[]>([]);
  // Spending limits in euros (0 = off) and what has been spent against them.
  // The message being written, kept here so the home screen can start one for the user to finish.
  let draft = $state('');
  let composerFocus = $state(0);
  let chatOnly = $state(false);
  let useInstructions = $state(true);
  let planEnabled = $state(true);
  let maxProjects = $state(6);
  let familiesOff = $state<string[]>([]);
  let gitEnabled = $state(false);
  let hideAuthors = $state(true);
  let instructions = $state<{ name: string; bytes: number; truncated: boolean }[]>([]);
  let limits = $state<BennuAssistantLimits>({ session_eur: 5, day_eur: 10, week_eur: 0, month_eur: 50, eur_per_usd: 0.92, warn_percent: 80 });
  let spend = $state<SpendStatus | null>(null);

  /** The proposal open in the review window, if any. */
  let review = $state<{ conversationId: string; proposalId: string } | null>(null);
  // Dry run: sending shows the request instead of making it. The preview can also be opened at any
  // time, to see what the next message would carry.
  let dryRun = $state(false);
  let previewOpen = $state(false);
  let preview = $state<RequestPreview | null>(null);
  let previewError = $state<string | null>(null);
  let previewLoading = $state(false);

  let compactOpen = $state(false);
  let compacting = $state(false);

  // What the next message will carry besides its text: the text selected in the editor, as last looked at.
  let selection = $state('');

  let activity = $state<Activity | null>(null);
  let runStartedAt = $state<number | null>(null);
  let runId = $state<string | null>(null);
  let runConversationId = $state<string | null>(null);
  let editorApi: AssistantEditorApi | null = null;
  /** Opens one of the editor's windows by name, for the assistant's `open_ui`. `false` for a name it does not know. */
  let uiOpener: ((target: string, query?: string) => boolean) | null = null;
  let started = false;

  const model = $derived<AssistantModel>(models.find((m) => m.id === modelId) ?? models[0] ?? placeholderModel(modelId));
  const active = $derived(conversations.find((c) => c.id === activeId) ?? null);
  const used = $derived(active ? totalTokens(active.context) : 0);
  const running = $derived(runId !== null);
  const reviewing = $derived.by<EditProposal | null>(() => {
    if (!review) return null;
    const conv = conversations.find((c) => c.id === review!.conversationId);
    for (const m of conv?.messages ?? []) {
      for (const b of m.blocks) if (b.type === 'proposal' && b.proposal.id === review.proposalId) return b.proposal;
    }
    return null;
  });
  /** The window closest to its limit, when it is past the warning mark — what the composer flags. */
  const pressing = $derived.by<(SpendMeter & { percent: number }) | null>(() => {
    const hot = (spend?.meters ?? []).filter((m) => m.level !== 'ok' && m.limitEur);
    if (!hot.length) return null;
    const m = hot.reduce((a, b) => (b.spentEur / (b.limitEur ?? 1) > a.spentEur / (a.limitEur ?? 1) ? b : a));
    return { ...m, percent: Math.round((m.spentEur / (m.limitEur ?? 1)) * 100) };
  });
  const sizes = $derived(CONTEXT_SIZES.filter((s) => s <= model.contextMax));
  const effectiveEffort = $derived<AssistantEffort | null>(
    model.efforts.length ? (model.efforts.includes(effort) ? effort : model.efforts.at(-1) ?? null) : null,
  );

  // ── Persistence of the choices ──────────────────────────────────────────────────────────────

  function adoptConfig() {
    const c = bennuConfigStore.cfg?.assistant;
    if (!c) return;
    modelId = models.some((m) => m.id === c.model) ? c.model : modelId;
    effort = (['low', 'medium', 'high', 'xhigh', 'max'] as const).find((e) => e === c.effort) ?? effort;
    mode = c.mode === 'auto' ? 'auto' : 'manual';
    scope = (['file', 'project', 'workspace', 'disk'] as const).find((s) => s === c.scope) ?? 'project';
    maxContext = Math.min(c.max_context, model.contextMax);
    allowEdits = c.allow_edits;
    allowTemp = c.allow_temp;
    extraFolders = [...c.extra_folders];
    limits = { ...limits, ...c.limits };
    chatOnly = c.chat_only ?? false;
    useInstructions = c.use_instructions ?? true;
    gitEnabled = c.git_enabled ?? false;
    planEnabled = c.plan_enabled ?? true;
    maxProjects = c.max_projects ?? 6;
    familiesOff = c.tool_families_off ?? [];
    hideAuthors = c.hide_authors ?? true;
  }

  function persist() {
    void bennuConfigStore.patch({
      assistant: {
        model: modelId, effort, mode, scope, max_context: maxContext,
        allow_edits: allowEdits, allow_temp: allowTemp, extra_folders: extraFolders, limits,
        chat_only: chatOnly, use_instructions: useInstructions, git_enabled: gitEnabled, hide_authors: hideAuthors, plan_enabled: planEnabled, max_projects: maxProjects, tool_families_off: familiesOff,
      },
    });
  }

  // ── Conversations ───────────────────────────────────────────────────────────────────────────

  function update(id: string, change: (c: Conversation) => Conversation) {
    conversations = conversations.map((c) => (c.id === id ? change(c) : c));
  }

  /** Replace the message with this id, or append it — the run sends the whole message each time. */
  function putMessage(conversationId: string, message: AssistantMessage) {
    update(conversationId, (c) => {
      const at = c.messages.findIndex((m) => m.id === message.id);
      const messages = at >= 0 ? c.messages.map((m, i) => (i === at ? message : m)) : [...c.messages, message];
      return { ...c, messages };
    });
  }

  function onEvent(e: AssistantEvent) {
    if (e.kind === 'open_ui') {
      if (!uiOpener?.(e.target, e.query)) toastStore.show(`The assistant tried to open "${e.target}", which this version cannot show.`, 'warning');
    } else if (e.kind === 'status') {
      if (runId === e.run_id) activity = { phase: e.phase, tool: e.tool ?? undefined, iteration: e.iteration };
    } else if (e.kind === 'message') {
      putMessage(e.conversation_id, e.message as AssistantMessage);
    } else if (e.kind === 'done') {
      update(e.conversation_id, (c) => ({
        ...c, cost: e.cost, context: e.context, title: e.title || c.title, updatedAt: e.updated_at,
      }));
      spend = e.spend ?? spend;
      if (runId === e.run_id) {
        activity = null;
        runStartedAt = null;
        runId = null;
        runConversationId = null;
      }
      if (e.error) toastStore.show(e.error, 'error');
      if (e.save_error) toastStore.show(`The conversation could not be saved: ${e.save_error}`, 'warning');
    }
  }

  async function fetchConversation(id: string) {
    if (loaded.has(id)) return;
    try {
      const full = await loadConversation(id);
      update(id, () => full);
      loaded = new Set(loaded).add(id);
    } catch {
      // A conversation that exists only here (not yet sent) has nothing to fetch.
    }
  }

  /** The prompt id carried by the block with this id, searched across the open conversation. */
  function promptOf(conversationId: string, match: (b: MessageBlock) => boolean): string | null {
    const conv = conversations.find((c) => c.id === conversationId);
    for (const m of conv?.messages ?? []) {
      for (const b of m.blocks) {
        if (match(b) && 'promptId' in b && b.promptId) return b.promptId;
      }
    }
    return null;
  }

  async function reply(promptId: string | null, answer: unknown) {
    if (!promptId) return;
    const ok = await replyToAssistant(promptId, answer).catch(() => false);
    if (!ok) toastStore.show('That question is no longer waiting for an answer.', 'warning');
  }

  function context(): { root: string; file: string | null; selection: string | null } | null {
    const root = projectStore.project?.root;
    if (!root) return null;
    const selection = editorApi?.selectedText().trim() || null;
    return { root, file: projectStore.activeFilePath ?? null, selection };
  }

  return {
    get conversations() { return conversations; },
    /** The conversations of the mode the panel is in: agent conversations and chats are kept apart. */
    get visible() { return conversations.filter((c) => !!c.chat === chatOnly); },
    get activeId() { return activeId; },
    get active() { return active; },
    get model() { return model; },
    get models() { return models; },
    get modelsFile() { return modelsFile; },
    get modelsError() { return modelsError; },

    /** Read the price list again — after the user has edited the file. */
    async reloadModels() {
      try {
        const list = await listModels();
        models = list.models;
        modelsFile = list.file;
        modelsError = list.error;
      } catch {
        // No backend yet: the placeholder stands in.
      }
    },
    get maxContext() { return maxContext; },
    get contextSizes() { return CONTEXT_SIZES; },
    get availableSizes() { return sizes; },
    get mode() { return mode; },
    get scope() { return scope; },
    /** The effort asked for, or `null` while the model in use has no such setting. */
    get effort(): AssistantEffort | null { return effectiveEffort; },
    get effortLevels(): AssistantEffort[] { return model.efforts; },
    get allowEdits() { return allowEdits; },
    get allowTemp() { return allowTemp; },
    get extraFolders() { return extraFolders; },
    /** The lines of editor text that will travel with the next message (0 when none is selected). */
    get selectionLines(): number { return selection ? selection.split(/\r?\n/).length : 0; },
    /** Look at the editor's selection again — it is not reactive, so the composer asks when it is focused. */
    refreshSelection() { selection = editorApi?.selectedText().trim() ?? ''; },
    get draft() { return draft; },
    set draft(v: string) { draft = v; },
    /** Ask the composer to take the focus (a starter that has put the start of a sentence in it). */
    get composerFocus() { return composerFocus; },
    focusComposer() { composerFocus += 1; },
    get chatOnly() { return chatOnly; },
    /** Switch between agent conversations and chats. The open conversation belongs to one of them, so
     *  changing mode goes back to the home screen of the other. */
    setChatOnly(on: boolean) {
      if (chatOnly === on) return;
      chatOnly = on;
      persist();
      if (active && !!active.chat !== on) activeId = null;
    },
    get familiesOff() { return familiesOff; },
    familyOn(id: string) { return !familiesOff.includes(id); },
    setFamily(id: string, on: boolean) {
      familiesOff = on ? familiesOff.filter((f) => f !== id) : [...familiesOff.filter((f) => f !== id), id];
      persist();
    },
    get maxProjects() { return maxProjects; },
    setMaxProjects(n: number) { maxProjects = Math.min(20, Math.max(1, Math.round(n))); persist(); },
    get planEnabled() { return planEnabled; },
    setPlanEnabled(on: boolean) { planEnabled = on; persist(); },
    get gitEnabled() { return gitEnabled; },
    setGitEnabled(on: boolean) { gitEnabled = on; persist(); },
    get hideAuthors() { return hideAuthors; },
    setHideAuthors(on: boolean) { hideAuthors = on; persist(); },
    get useInstructions() { return useInstructions; },
    setUseInstructions(on: boolean) { useInstructions = on; persist(); void this.refreshInstructions(); },
    /** The instruction files (CLAUDE.md, AGENTS.md…) that would be read for the open project. */
    get instructions() { return useInstructions ? instructions : []; },
    async refreshInstructions() {
      const root = projectStore.project?.root;
      if (!root || !useInstructions) { instructions = []; return; }
      try { instructions = await getInstructions(root); } catch { instructions = []; }
    },
    get limits() { return limits; },
    get spend() { return spend; },
    get pressing() { return pressing; },
    /** Dollars → euros at the configured rate (1 when the rate is unusable, as the backend does). */
    eur(usd: number): number { return usd * (limits.eur_per_usd > 0 ? limits.eur_per_usd : 1); },
    setLimits(patch: Partial<BennuAssistantLimits>) {
      limits = { ...limits, ...patch };
      persist();
      void this.refreshSpend();
    },
    async refreshSpend() {
      try {
        spend = await getSpend(periodStarts(), activeId);
      } catch {
        // No backend yet: the meters stay empty.
      }
    },
    get running() { return running; },
    /** What the run in progress is doing right now. */
    get activity() { return activity; },
    get runStartedAt() { return runStartedAt; },
    get used() { return used; },
    get dryRun() { return dryRun; },
    get previewOpen() { return previewOpen; },
    get preview() { return preview; },
    get previewError() { return previewError; },
    get previewLoading() { return previewLoading; },
    get review() { return review; },
    get reviewing() { return reviewing; },

    bindEditor(api: AssistantEditorApi) { editorApi = api; },
    bindUiOpener(open: (target: string, query?: string) => boolean) { uiOpener = open; },

    setEffort(next: AssistantEffort) { effort = next; persist(); },
    setScope(next: AccessScope) { scope = next; persist(); },
    setAllowEdits(on: boolean) { allowEdits = on; persist(); },
    setAllowTemp(on: boolean) { allowTemp = on; persist(); },
    setMode(next: AssistantMode) { mode = next; persist(); },
    setDryRun(on: boolean) { dryRun = on; },
    addFolder(path: string) {
      const clean = path.trim();
      if (clean && !extraFolders.includes(clean)) { extraFolders = [...extraFolders, clean]; persist(); }
    },
    removeFolder(path: string) { extraFolders = extraFolders.filter((f) => f !== path); persist(); },

    /** Changing model keeps the window if the new model can hold it, and shrinks it to the model's own
     *  largest size when it cannot. */
    setModel(id: string) {
      const next = models.find((m) => m.id === id);
      if (!next) return;
      modelId = id;
      if (maxContext > next.contextMax) {
        maxContext = CONTEXT_SIZES.filter((s) => s <= next.contextMax).at(-1) ?? next.contextMax;
      }
      persist();
    },

    setMaxContext(tokens: number) {
      if (tokens <= model.contextMax) { maxContext = tokens; persist(); }
    },

    openReview(conversationId: string, proposalId: string) { review = { conversationId, proposalId }; },
    closeReview() { review = null; },
    closePreview() { previewOpen = false; },

    /** Read the choices, list the conversations, and start listening to runs. Once. */
    async start() {
      if (started) return;
      started = true;
      await bennuConfigStore.load();
      await this.reloadModels();
    adoptConfig();
      await onAssistantEvent(onEvent);
      try {
        const list = await listConversations();
        // The panel opens on its home screen; the conversations are one click away, there and in the list.
        conversations = list.map((s) => ({ ...s, messages: [] }));
      } catch {
        // No backend yet (or no conversations): the panel opens empty.
      }
      await this.refreshSpend();
      void this.refreshInstructions();
    },

    async select(id: string) {
      const target = conversations.find((c) => c.id === id);
      if (!target) return;
      // Opening a chat from anywhere puts the panel in chat mode, and the other way round.
      if (!!target.chat !== chatOnly) { chatOnly = !!target.chat; persist(); }
      activeId = id;
      await fetchConversation(id);
      void this.refreshSpend();
    },

    /** A conversation to ask something in: the open one when it is still empty, a new one otherwise — so a
     *  one-off question from the editor never lands in a conversation the user was in the middle of. */
    startFresh() {
      // What the editor asks needs the project, which a chat does not have.
      if (chatOnly) this.setChatOnly(false);
      if (active && active.messages.length === 0) return;
      this.newChat();
    },

    /** Back to the home screen of the current mode. */
    goHome() { activeId = null; },

    newChat() {
      if (active && active.messages.length === 0 && !!active.chat === chatOnly) return;
      const c = emptyConversation(modelId, chatOnly);
      conversations = [c, ...conversations];
      loaded = new Set(loaded).add(c.id);
      activeId = c.id;
    },

    async rename(id: string, title: string) {
      const clean = title.trim();
      if (!clean) return;
      update(id, (c) => ({ ...c, title: clean }));
      await renameConversation(id, clean).catch(() => {});
    },

    async remove(id: string) {
      conversations = conversations.filter((c) => c.id !== id);
      if (activeId === id) {
        // Back to the home screen: which conversation to open next is not for the panel to guess.
        activeId = null;
      }
      await deleteConversation(id).catch(() => {});
    },

    /**
     * Send `text` in the open conversation — or, with the dry run on, build the request and show it.
     * Returns whether the draft was consumed (a dry run keeps it, so it can be sent for real next).
     */
    async send(text: string): Promise<boolean> {
      const ctx = context();
      if (!ctx) {
        toastStore.show('Open a project first — the assistant works inside one.', 'warning');
        return false;
      }
      if (running) return false;
      if (!active) this.newChat();
      const conv = conversations.find((c) => c.id === activeId)!;

      const params = {
        conversation_id: conv.id,
        text,
        root: ctx.root,
        file: ctx.file,
        selection: ctx.selection,
        model: modelId,
        effort: effectiveEffort,
        auto: mode === 'auto',
        scope,
        allow_edits: allowEdits,
        allow_temp: allowTemp,
        extra_folders: extraFolders,
        workspace_roots: projectStore.workspaceRoots,
        max_projects: maxProjects,
        max_context: maxContext,
        periods: periodStarts(),
        chat: chatOnly,
        instructions: useInstructions,
        git: gitEnabled,
        hide_authors: hideAuthors,
        plan: planEnabled,
        families_off: familiesOff,
        dry_run: dryRun,
      };

      if (dryRun) {
        await this.showPreview(text);
        return false;
      }

      // The user's message appears at once; the run's reply streams in behind it.
      const now = Date.now();
      putMessage(conv.id, { id: `u${now}`, role: 'user', at: now, blocks: [{ type: 'text', text }] });
      try {
        const result = await sendAssistantMessage(params);
        if ('run_id' in result) {
          runId = result.run_id;
          runStartedAt = Date.now();
          activity = { phase: 'thinking', iteration: 1 };
          runConversationId = result.conversation_id;
        }
        return true;
      } catch (e) {
        toastStore.show(String(e), 'error');
        return true;
      }
    },

    /** Build, without sending, the request the next message would make. */
    async showPreview(text: string) {
      const ctx = context();
      previewOpen = true;
      previewError = null;
      preview = null;
      if (!ctx) { previewError = 'Open a project first.'; return; }
      previewLoading = true;
      try {
        const result = await sendAssistantMessage({
          conversation_id: active?.id ?? null,
          text: text.trim() || '(the next message)',
          root: ctx.root, file: ctx.file, selection: ctx.selection,
          model: modelId, effort: effectiveEffort, auto: mode === 'auto', scope,
          allow_edits: allowEdits, allow_temp: allowTemp, extra_folders: extraFolders,
          workspace_roots: projectStore.workspaceRoots,
          max_projects: maxProjects,
          max_context: maxContext, periods: periodStarts(), chat: chatOnly, instructions: useInstructions, git: gitEnabled, hide_authors: hideAuthors, plan: planEnabled, families_off: familiesOff, dry_run: true,
        });
        if ('preview' in result) preview = result.preview;
      } catch (e) {
        previewError = String(e);
      } finally {
        previewLoading = false;
      }
    },

    get compactOpen() { return compactOpen; },
    get compacting() { return compacting; },
    openCompact() { compactOpen = true; },
    closeCompact() { compactOpen = false; },

    /** Clear old tool output and fold the early part into a note; `instructions` says what it must keep. */
    async compact(instructions = '') {
      if (!activeId || compacting || running) return;
      const id = activeId;
      compacting = true;
      try {
        const r = await compactConversation({
          id, instructions: instructions.trim() || undefined, model: modelId,
        });
        update(id, (c) => ({ ...c, context: r.context, cost: r.cost, messages: r.messages }));
        compactOpen = false;
        toastStore.show(r.freed > 0 ? `Freed about ${r.freed.toLocaleString()} tokens.` : 'Nothing earlier to fold yet.', 'info');
      } catch (e) {
        toastStore.show(String(e), 'error');
      } finally {
        compacting = false;
      }
    },

    async cancel() {
      if (runId) await cancelAssistantRun(runId).catch(() => {});
    },

    /** Apply or reject an edit proposal. */
    async resolveProposal(conversationId: string, proposalId: string, status: Exclude<ProposalStatus, 'pending'>) {
      try {
        await decideProposal(conversationId, proposalId, status === 'applied');
      } catch (e) {
        toastStore.show(String(e), 'error');
        return;
      }
      update(conversationId, (c) => ({
        ...c,
        messages: c.messages.map((m) => ({
          ...m,
          blocks: m.blocks.map((b) => (b.type === 'proposal' && b.proposal.id === proposalId ? { ...b, proposal: { ...b.proposal, status } } : b)),
        })),
      }));
    },

    /** Submit the answers to a set of questions the assistant asked. */
    async answerQuestions(conversationId: string, setId: string, answers: Record<string, string[]>) {
      await reply(promptOf(conversationId, (b) => b.type === 'questions' && b.set.id === setId), { answers });
    },

    /** Answer a request to reach outside the allowed scope. */
    async answerAccess(conversationId: string, requestId: string, answer: AccessAnswer) {
      await reply(promptOf(conversationId, (b) => b.type === 'access' && b.request.id === requestId), { answer });
    },

    /** Answer a yes/no the assistant is waiting on. */
    async answerConfirm(conversationId: string, confirmId: string, answer: 'yes' | 'no') {
      await reply(promptOf(conversationId, (b) => b.type === 'confirm' && b.confirm.id === confirmId), { answer });
    },

    /** Which conversation the run in progress belongs to. */
    get runConversationId() { return runConversationId; },
  };
}

export const bennuAssistantStore = createAssistantStore();
