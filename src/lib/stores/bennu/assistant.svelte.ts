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

import { imberCreditsStore } from '$lib/stores/imber-credits.svelte';
import { formatTokens, totalTokens } from '$lib/components/bennu/assistant/assistant-format';
import type {
  AccessAnswer, AccessScope, AttachmentItem, AssistantEffort, AssistantMessage, AssistantMode, AssistantModel, Conversation,
  EditProposal, MessageBlock, ProposalStatus, ScriptAnswer, ScriptRequest,
} from '$lib/components/bennu/assistant/assistant-types';
import {
  cancelAssistantRun, retryAssistantRunNow, sendAssistantNote, revokeAssistantGrant, decideProposal, remakeProposal as remakeProposalIpc, undoProposal as undoProposalIpc, canUndoProposal, type DecideResult, type RemakeResult, type UndoResult, deleteConversation, listConversations, loadConversation, onAssistantEvent,
  compactConversation, getInstructions, getSpend, listModels, listSkills, type SkillListing, type SkillInfo, renameConversation, replyToAssistant, sendAssistantMessage, type AttachmentParam, type AssistantEvent, type SendParams, type RequestPreview, type LastRequest, type SpendMeter, type SpendStatus,
} from '$lib/ipc/bennu/assistant';
import { bennuConfigStore } from './config.svelte';
import type { BennuAssistantLimits, ExternalTool, OpenProjectsWord } from '$lib/ipc/bennu/config';
import { nextMode, parseMode } from '$lib/components/bennu/assistant/assistant-mode';
import { periodStarts } from '$lib/components/bennu/assistant/assistant-periods';
import { clampMaxSteps, DEFAULT_MAX_STEPS } from '$lib/components/bennu/assistant/assistant-steps';
import { pendingDiagrams, verdictAnswer } from '$lib/components/bennu/assistant/assistant-diagram';
import { validateMermaid } from '$lib/utils/mermaid';
import { redoRequest, withDecision, withRemade, withUndo } from '$lib/components/bennu/assistant/assistant-proposal';
import type { Activity } from '$lib/components/bennu/assistant/assistant-activity';
import type { RetryStatus } from '$lib/components/bennu/assistant/assistant-retry';
import { projectStore } from './project.svelte';
import { bennuWorktreeStore } from './assistant-worktree.svelte';
import { bennuImberSchedules } from './imber-schedules.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { onBackendProgress, type BackendProgress } from '$lib/ipc/progress';
import { contextChange, modelChange, type CostChange } from '$lib/components/bennu/assistant/assistant-cost-change';
import { DEFAULT_DECISION_LEVEL, MAX_DECISION_NOTES, MAX_GENERAL_CONTEXT, parseDecisionLevel, type DecisionLevel } from '$lib/components/bennu/assistant/assistant-decision';
import { nextSubagentsWord, parseSubagentsWord, subagentsEffective, type SubagentsWord } from '$lib/components/bennu/assistant/assistant-agents';

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

/** One conversation's run, while it goes. */
interface RunState {
  runId: string;
  startedAt: number;
  /** What it is doing right now. */
  activity: Activity | null;
  // What the tool being run says about itself (a test run's counts, the crate being compiled), and when the
  // current step began: a step of minutes with nothing to show reads as a hang.
  toolProgress: BackendProgress | null;
  stepStartedAt: number | null;
  /** Set while it waits to send a failed request again: the one status line, no toast per attempt. */
  retrying: RetryStatus | null;
  /** What the user wrote while it worked, not read yet. */
  notes: { id: string; text: string }[];
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
  /** Where the next conversation's builds compile; fixed once it starts. Not remembered across restarts. */
  let buildIsolation = $state<'shared' | 'separate'>('shared');
  let useInstructions = $state(true);
  let planEnabled = $state(true);
  let maxProjects = $state(6);
  let familiesOff = $state<string[]>([]);
  let bevyWrite = $state(false);
  let debugMutate = $state(false);
  let decisionLevel = $state<DecisionLevel>(DEFAULT_DECISION_LEVEL);
  let decisionNotes = $state('');
  let generalContext = $state('');
  let contextOpen = $state(false);
  let confirmCostChanges = $state(true);
  let subagentsEnabled = $state(false);
  let debugIdleMinutes = $state(30);
  let maxSteps = $state(DEFAULT_MAX_STEPS);
  let autoContinueOnStop = $state(false);
  let longCache = $state(true);
  let lastRequest = $state<LastRequest | null>(null);
  let sideModel = $state('');
  let wrapPastedCode = $state(true);
  // Skills: opt-in, by `<scope>/<name>` id — see the skills manager. The listing is what is on disk now.
  let skillsOn = $state<string[]>([]);
  let skillFolders = $state<string[]>([]);
  /** Programs Imber may run; written back whole with the rest of the section, or the next save would drop them. */
  let externalTools = $state<ExternalTool[]>([]);
  let openProjects = $state<OpenProjectsWord>('autonomous');
  let notifyQuestions = $state(true);
  let notifyDone = $state(false);
  let skillListing = $state<SkillListing | null>(null);
  let skillsLoading = $state(false);
  let skillsOpen = $state(false);
  let deleteAsked = $state<string | null>(null);
  let gitEnabled = $state(false);
  let hideAuthors = $state(true);
  let instructions = $state<{ name: string; bytes: number; truncated: boolean }[]>([]);
  let limits = $state<BennuAssistantLimits>({ session_eur: 5, day_eur: 10, week_eur: 0, month_eur: 50, eur_per_usd: 0.92, warn_percent: 80, schedule_run_eur: 1, schedule_runs_per_day: 24, credit_usd: 0, credit_set_at: 0 });
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

  let decisionOpen = $state(false);
  let costPending = $state<{ change: CostChange; to: string; apply: () => void } | null>(null);
  let compactOpen = $state(false);
  let compacting = $state(false);

  // What the next message will carry besides its text: the text selected in the editor, as last looked at.
  let selection = $state('');

  /**
   * The runs going on, by conversation: several conversations work at once, each with its own run, its own
   * status line and its own messages waiting to be read. What the panel shows is the open conversation's.
   */
  let runs = $state<Record<string, RunState>>({});
  let editorApi: AssistantEditorApi | null = null;
  /** Opens one of the editor's windows by name, for the assistant's `open_ui`. `false` for a name it does not know. */
  let uiOpener: ((target: string, query?: string) => boolean) | null = null;
  let started = false;

  const model = $derived<AssistantModel>(models.find((m) => m.id === modelId) ?? models[0] ?? placeholderModel(modelId));
  const active = $derived(conversations.find((c) => c.id === activeId) ?? null);
  const used = $derived(active ? totalTokens(active.context) : 0);
  /** The open conversation's run, when it has one going. */
  const current = $derived<RunState | null>(activeId ? runs[activeId] ?? null : null);
  const running = $derived(current !== null);

  /** Change the run of `conversationId`, if that run is `id` (an event of a run already over changes nothing). */
  function changeRun(conversationId: string, id: string, change: (r: RunState) => RunState) {
    const r = runs[conversationId];
    if (r && r.runId === id) runs = { ...runs, [conversationId]: change(r) };
  }
  /** The open conversation ended on a failed request that can be sent again as it stands. */
  const canRetry = $derived.by(() => {
    if (running || !active) return false;
    const last = active.messages.at(-1);
    return !!last && last.role === 'assistant' && last.blocks.some((b) => b.type === 'notice' && b.canRetry === true);
  });
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
    mode = parseMode(c.mode);
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
    bevyWrite = c.bevy_write_enabled ?? false;
    debugMutate = c.debug_mutate_enabled ?? false;
    skillsOn = c.skills_on ?? [];
    skillFolders = c.skill_folders ?? [];
    externalTools = c.external_tools ?? [];
    openProjects = c.open_projects ?? 'autonomous';
    notifyQuestions = c.notify_questions ?? true;
    notifyDone = c.notify_done ?? false;
    hideAuthors = c.hide_authors ?? true;
    decisionLevel = parseDecisionLevel(c.decision_level);
    decisionNotes = c.decision_notes ?? '';
    generalContext = c.general_context ?? '';
    confirmCostChanges = c.confirm_cost_changes ?? true;
    subagentsEnabled = c.subagents_enabled ?? false;
    debugIdleMinutes = c.debug_idle_minutes ?? 30;
    maxSteps = clampMaxSteps(c.max_steps ?? 0);
    autoContinueOnStop = c.auto_continue_on_stop ?? false;
    longCache = c.long_cache ?? true;
    sideModel = c.side_model ?? '';
    wrapPastedCode = c.wrap_pasted_code ?? true;
  }

  function persist() {
    void bennuConfigStore.patch({
      assistant: {
        model: modelId, effort, mode, scope, max_context: maxContext,
        allow_edits: allowEdits, allow_temp: allowTemp, extra_folders: extraFolders, limits,
        chat_only: chatOnly, use_instructions: useInstructions, git_enabled: gitEnabled, hide_authors: hideAuthors, plan_enabled: planEnabled, max_projects: maxProjects, tool_families_off: familiesOff, bevy_write_enabled: bevyWrite, debug_mutate_enabled: debugMutate, skills_on: skillsOn, skill_folders: skillFolders,
        decision_level: decisionLevel, decision_notes: decisionNotes, confirm_cost_changes: confirmCostChanges,
        subagents_enabled: subagentsEnabled, debug_idle_minutes: debugIdleMinutes, max_steps: maxSteps, auto_continue_on_stop: autoContinueOnStop, wrap_pasted_code: wrapPastedCode,
        long_cache: longCache, side_model: sideModel, general_context: generalContext, external_tools: externalTools, open_projects: openProjects, notify_questions: notifyQuestions, notify_done: notifyDone,
      },
    });
  }

  // ── Conversations ───────────────────────────────────────────────────────────────────────────

  /** Open an empty conversation of the current kind, unless the one open already is one. */
  function newChatNow() {
    if (active && active.messages.length === 0 && !!active.chat === chatOnly) return;
    const c = emptyConversation(modelId, chatOnly);
    conversations = [c, ...conversations];
    loaded = new Set(loaded).add(c.id);
    activeId = c.id;
  }

  /**
   * Change a choice that belongs to the open conversation (non-stop, sub-agents, decisions). On the screen of a new
   * conversation there is none yet, and changing nothing is what made such a switch look stuck on "off" — so the
   * conversation is made now, as the first message would make it, and the choice is kept with it.
   */
  function changeChat(change: (c: Conversation) => Conversation) {
    if (!activeId) newChatNow();
    if (activeId) update(activeId, change);
  }

  function update(id: string, change: (c: Conversation) => Conversation) {
    conversations = conversations.map((c) => (c.id === id ? change(c) : c));
  }

  function findProposal(conversationId: string, proposalId: string): EditProposal | null {
    const conv = conversations.find((c) => c.id === conversationId);
    for (const m of conv?.messages ?? []) {
      for (const b of m.blocks) if (b.type === 'proposal' && b.proposal.id === proposalId) return b.proposal;
    }
    return null;
  }

  function changeProposal(conversationId: string, proposalId: string, change: (p: EditProposal) => EditProposal) {
    update(conversationId, (c) => ({
      ...c,
      messages: c.messages.map((m) => ({
        ...m,
        blocks: m.blocks.map((b) => (b.type === 'proposal' && b.proposal.id === proposalId ? { ...b, proposal: change(b.proposal) } : b)),
      })),
    }));
  }

  /** Replace the message with this id, or append it — the run sends the whole message each time. */
  function putMessage(conversationId: string, message: AssistantMessage) {
    update(conversationId, (c) => {
      const at = c.messages.findIndex((m) => m.id === message.id);
      const messages = at >= 0 ? c.messages.map((m, i) => (i === at ? message : m)) : [...c.messages, message];
      return { ...c, messages };
    });
  }

  /** Prompts of diagrams already taken up, so a message pushed again does not parse its diagram twice. */
  const diagramsTaken = new Set<string>();

  /** Tell the run whether the diagrams it just drew can be drawn: only Mermaid can say. Asynchronous, so a
   *  first diagram (which loads the library) never holds the event stream. No toast on a refusal — the run
   *  hands the reason back to the model, which fixes the source. */
  function checkDiagrams(message: AssistantMessage) {
    for (const d of pendingDiagrams(message, diagramsTaken)) {
      diagramsTaken.add(d.promptId);
      void validateMermaid(d.source)
        .then((verdict) => replyToAssistant(d.promptId, verdictAnswer(verdict)))
        .catch(() => {})
        .finally(() => diagramsTaken.delete(d.promptId));
    }
  }

  /** The parameters of a send: the settings as they stand now. A retry sends the conversation again, so it never previews. */
  function buildParams(conv: Conversation, ctx: NonNullable<ReturnType<typeof context>>, text: string, retry: boolean): SendParams {
    return {
      conversation_id: conv.id,
      text,
      retry,
      root: ctx.root,
      file: ctx.file,
      selection: ctx.selection,
      model: modelId,
      effort: effectiveEffort,
      mode,
      scope,
      allow_edits: allowEdits,
      allow_temp: allowTemp,
      extra_folders: extraFolders,
      workspace_roots: projectStore.workspaceRoots,
      max_projects: maxProjects,
      max_context: maxContext,
      periods: periodStarts(),
      chat: chatOnly,
      build_isolation: buildIsolation,
      worktree: chatOnly ? null : bennuWorktreeStore.forSend(conv),
      instructions: useInstructions,
      git: gitEnabled,
      hide_authors: hideAuthors,
      plan: planEnabled,
      families_off: familiesOff,
      bevy_write: bevyWrite,
      debug_mutate: debugMutate,
      skills_on: skillsOn,
      skill_folders: skillFolders,
      decision_level: decisionLevel,
      decision_notes: decisionNotes,
      debug_idle_minutes: debugIdleMinutes,
      max_steps: maxSteps,
      chat_decision_level: conv.decisionLevel ?? '',
      chat_decision_notes: conv.decisionNotes ?? '',
      subagents_enabled: subagentsEnabled,
      chat_subagents: conv.subagents ?? '',
      chat_non_stop: !!conv.nonStop,
      dry_run: dryRun && !retry,
    };
  }

  function onEvent(e: AssistantEvent) {
    if (e.kind === 'open_ui') {
      if (!uiOpener?.(e.target, e.query)) toastStore.show(`Imber tried to open "${e.target}", which this version cannot show.`, 'warning');
    } else if (e.kind === 'status') {
      changeRun(e.conversation_id, e.run_id, (r) => {
        const next = { phase: e.phase, tool: e.tool ?? undefined, iteration: e.iteration };
        const a = r.activity;
        const newStep = !a || a.phase !== next.phase || a.tool !== next.tool || a.iteration !== next.iteration;
        return { ...r, activity: next, ...(newStep ? { stepStartedAt: Date.now(), toolProgress: null } : {}) };
      });
    } else if (e.kind === 'retry') {
      changeRun(e.conversation_id, e.run_id, (r) => ({
        ...r, retrying: { attempt: e.attempt, max: e.max, until: e.until, kind: e.error_kind, message: e.message, detail: e.detail },
      }));
    } else if (e.kind === 'retry_end') {
      changeRun(e.conversation_id, e.run_id, (r) => ({ ...r, retrying: null }));
    } else if (e.kind === 'title') {
      update(e.conversation_id, (c) => ({ ...c, title: e.title }));
    } else if (e.kind === 'live') {
      // The meter moves while the run goes; `done` sets the final figures.
      update(e.conversation_id, (c) => ({ ...c, cost: e.cost ?? c.cost, context: e.context ?? c.context }));
    } else if (e.kind === 'message') {
      putMessage(e.conversation_id, e.message as AssistantMessage);
      checkDiagrams(e.message as AssistantMessage);
      // A note shows in the thread once the run has read it: then it no longer waits.
      const r = runs[e.conversation_id];
      if (r?.notes.length) {
        const read = new Set((e.message as AssistantMessage).blocks.flatMap((b) => (b.type === 'note' ? [b.note.id] : [])));
        if (read.size) runs = { ...runs, [e.conversation_id]: { ...r, notes: r.notes.filter((n) => !read.has(n.id)) } };
      }
    } else if (e.kind === 'done') {
      // A scheduled run may belong to a conversation the panel never opened; the schedule list still wants to know it ended.
      bennuImberSchedules.runEnded();
      // What the providers have left moved: the title bar's meter reads again, in the background.
      void imberCreditsStore.refresh();
      update(e.conversation_id, (c) => ({
        ...c, cost: e.cost, context: e.context, title: e.title || c.title, updatedAt: e.updated_at,
      }));
      spend = e.spend ?? spend;
      if (e.request) lastRequest = e.request;
      if (runs[e.conversation_id]?.runId === e.run_id) {
        const { [e.conversation_id]: _ended, ...rest } = runs;
        runs = rest;
        // What it ended before reading goes back in the box, ahead of anything typed since.
        const unread = (e.unread_notes ?? []).map((n) => n.text).join('\n\n');
        if (unread) {
          draft = draft.trim() ? `${unread}\n\n${draft}` : unread;
          const where = e.conversation_id === activeId ? '' : ` (from “${conversations.find((c) => c.id === e.conversation_id)?.title ?? 'another conversation'}”)`;
          toastStore.show(`Imber finished before reading your message${where}: it is back in the box.`, 'info');
        }
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
    get buildIsolation() { return buildIsolation; },
    setBuildIsolation(v: 'shared' | 'separate') { buildIsolation = v; },
    /** Switch between agent conversations and chats. The open conversation belongs to one of them, so
     *  changing mode goes back to the home screen of the other. */
    setChatOnly(on: boolean) {
      if (chatOnly === on) return;
      chatOnly = on;
      persist();
      if (active && !!active.chat !== on) activeId = null;
    },
    get familiesOff() { return familiesOff; },
    get bevyWrite() { return bevyWrite; },
    setBevyWrite(on: boolean) { bevyWrite = on; persist(); },
    get debugMutate() { return debugMutate; },
    setDebugMutate(on: boolean) { debugMutate = on; persist(); },
    familyOn(id: string) { return !familiesOff.includes(id); },
    setFamily(id: string, on: boolean) {
      familiesOff = on ? familiesOff.filter((f) => f !== id) : [...familiesOff.filter((f) => f !== id), id];
      persist();
    },
    // ── Skills ──
    get skillsOn() { return skillsOn; },
    skillOn(id: string) { return skillsOn.includes(id); },
    /** Switch skills on or off together; ids no longer on disk stay as they are, so a skill that is
     *  temporarily missing (a branch without it) keeps the user's choice. */
    setSkills(ids: string[], on: boolean) {
      const rest = skillsOn.filter((i) => !ids.includes(i));
      skillsOn = on ? [...rest, ...ids] : rest;
      persist();
    },
    setSkill(id: string, on: boolean) { this.setSkills([id], on); },
    get skillListing() { return skillListing; },
    get skillsLoading() { return skillsLoading; },
    /** The skills the model is offered: enabled, and not hidden by a project skill of the same name. */
    get enabledSkills(): SkillInfo[] {
      return (skillListing?.skills ?? []).filter((s) => !s.shadowed && skillsOn.includes(s.id));
    },
    /** How many of the skills on disk are enabled, for "3 of 12 enabled". */
    get skillCounts(): { on: number; total: number } {
      const all = skillListing?.skills ?? [];
      return { on: all.filter((s) => skillsOn.includes(s.id)).length, total: all.length };
    },
    /** Look on disk again. The projects searched are the ones the run would work across. */
    async refreshSkills() {
      const root = projectStore.project?.root;
      const wide = scope === 'workspace' || scope === 'disk';
      const open = wide ? projectStore.workspaceRoots : [];
      const roots = [...new Set([...(root ? [root] : []), ...open])].slice(0, wide ? maxProjects : 1);
      skillsLoading = true;
      try { skillListing = await listSkills(roots, skillFolders); }
      catch (e) { toastStore.show(`Could not look for skills: ${e}`, 'error'); }
      finally { skillsLoading = false; }
    },
    get skillsOpen() { return skillsOpen; },
    openSkills() { skillsOpen = true; void this.refreshSkills(); },
    closeSkills() { skillsOpen = false; },

    get maxProjects() { return maxProjects; },
    setMaxProjects(n: number) { maxProjects = Math.min(20, Math.max(1, Math.round(n))); persist(); },
    /** The standing context sent with every conversation (Settings › Imber, the panel's menu). */
    get generalContext() { return generalContext; },
    setGeneralContext(v: string) { generalContext = v.slice(0, MAX_GENERAL_CONTEXT); persist(); },
    get contextOpen() { return contextOpen; },
    openContext() { contextOpen = true; },
    closeContext() { contextOpen = false; },
    get decisionOpen() { return decisionOpen; },
    openDecision() { decisionOpen = true; },
    closeDecision() { decisionOpen = false; },
    get decisionLevel() { return decisionLevel; },
    setDecisionLevel(v: DecisionLevel) { decisionLevel = v; persist(); },
    get decisionNotes() { return decisionNotes; },
    setDecisionNotes(v: string) { decisionNotes = v.slice(0, MAX_DECISION_NOTES); persist(); },
    /** The open conversation's own level ('' inherits the setting) and notes; applied from its next message. */
    get chatDecisionLevel() { return active?.decisionLevel ?? ''; },
    get chatDecisionNotes() { return active?.decisionNotes ?? ''; },
    setChatDecisionLevel(v: string) { changeChat((c) => ({ ...c, decisionLevel: v })); },
    setChatDecisionNotes(v: string) { changeChat((c) => ({ ...c, decisionNotes: v.slice(0, MAX_DECISION_NOTES) })); },
    /** The open conversation's own switch for starting a run when its debugged program stops ('' follows the setting). */
    get chatAutoContinue() { return active?.autoContinue ?? ''; },
    setChatAutoContinue(v: string) { changeChat((c) => ({ ...c, autoContinue: v })); },
    /** Whether a conversation may start a run by itself on a stop: its own switch, else the setting. */
    autoContinueFor(id: string): boolean | undefined { const own = conversations.find((c) => c.id === id)?.autoContinue; return own === 'on' ? true : own === 'off' ? false : undefined; },
    get confirmCostChanges() { return confirmCostChanges; },
    setConfirmCostChanges(on: boolean) { confirmCostChanges = on; persist(); },
    get subagentsEnabled() { return subagentsEnabled; },
    setSubagentsEnabled(on: boolean) { subagentsEnabled = on; persist(); },
    /** This conversation's own sub-agents switch (`''` follows the setting); kept with the conversation on its next send. */
    get chatSubagents() { return parseSubagentsWord(active?.subagents); },
    setChatSubagents(v: SubagentsWord) { changeChat((c) => ({ ...c, subagents: v })); },
    cycleChatSubagents() { this.setChatSubagents(nextSubagentsWord(active?.subagents)); },
    /** Non-stop for the conversation in front of you: no step cap. Sent with its next message, kept with it. */
    get chatNonStop() { return !!active?.nonStop; },
    setChatNonStop(on: boolean) { changeChat((c) => ({ ...c, nonStop: on })); },
    /** Whether the open conversation is offered sub-agents right now. */
    get subagentsEffective() { return subagentsEffective(subagentsEnabled, active?.subagents); },
    get debugIdleMinutes() { return debugIdleMinutes; },
    setDebugIdleMinutes(n: number) { debugIdleMinutes = Math.min(240, Math.max(5, Math.round(n))); persist(); },
    get maxSteps() { return maxSteps; },
    setMaxSteps(n: number) { maxSteps = clampMaxSteps(n); persist(); },
    get autoContinueOnStop() { return autoContinueOnStop; },
    setAutoContinueOnStop(on: boolean) { autoContinueOnStop = on; persist(); },
    get lastRequest() { return lastRequest; },
    get longCache() { return longCache; },
    setLongCache(on: boolean) { longCache = on; persist(); },
    get sideModel() { return sideModel; },
    setSideModel(id: string) { sideModel = id; persist(); },
    get wrapPastedCode() { return wrapPastedCode; },
    setWrapPastedCode(on: boolean) { wrapPastedCode = on; persist(); },
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
    /** The open conversation has a run going. Another conversation's run does not count: it works on its own. */
    get running() { return running; },
    /** Whether `conversationId` has a run going. */
    isRunning(conversationId: string): boolean { return !!runs[conversationId]; },
    /** The conversations with a run going. */
    get runningIds() { return Object.keys(runs); },
    /** What the open conversation's run is doing right now. */
    get activity() { return current?.activity ?? null; },
    /** What the running tool last said about itself; null outside a tool, or before it says anything. */
    get toolProgress() { return current?.toolProgress ?? null; },
    get stepStartedAt() { return current?.stepStartedAt ?? null; },
    /** The run is waiting to send a failed request again, or null. */
    get retrying() { return current?.retrying ?? null; },
    get canRetry() { return canRetry; },
    get runStartedAt() { return current?.startedAt ?? null; },
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
    /** Manual → Accept edits → Autonomous → Manual, for a key that steps through them. */
    cycleMode() { mode = nextMode(mode); persist(); },
    setDryRun(on: boolean) { dryRun = on; },
    addFolder(path: string) {
      const clean = path.trim();
      if (clean && !extraFolders.includes(clean)) { extraFolders = [...extraFolders, clean]; persist(); }
    },
    removeFolder(path: string) { extraFolders = extraFolders.filter((f) => f !== path); persist(); },
    get skillFolders() { return skillFolders; },
    get externalTools() { return externalTools; },
    get openProjects() { return openProjects; },
    setOpenProjects(v: OpenProjectsWord) { openProjects = v; persist(); },
    get notifyQuestions() { return notifyQuestions; },
    setNotifyQuestions(v: boolean) { notifyQuestions = v; persist(); },
    get notifyDone() { return notifyDone; },
    setNotifyDone(v: boolean) { notifyDone = v; persist(); },
    /** Add a tool (`index` null) or replace the one at `index`. */
    saveExternalTool(index: number | null, tool: ExternalTool) {
      externalTools = index === null ? [...externalTools, tool] : externalTools.map((t, i) => (i === index ? tool : t));
      persist();
    },
    removeExternalTool(index: number) {
      externalTools = externalTools.filter((_, i) => i !== index);
      persist();
    },
    setExternalToolEnabled(index: number, enabled: boolean) {
      externalTools = externalTools.map((t, i) => (i === index ? { ...t, enabled } : t));
      persist();
    },
    /** A folder of skills picked by hand. The listing is looked for again, so its skills show at once. */
    addSkillFolder(path: string) {
      const clean = path.trim();
      if (!clean || skillFolders.includes(clean)) return;
      skillFolders = [...skillFolders, clean];
      persist();
      void this.refreshSkills();
    },
    removeSkillFolder(path: string) {
      skillFolders = skillFolders.filter((f) => f !== path);
      persist();
      void this.refreshSkills();
    },

    /** Changing model keeps the window if the new model can hold it, and shrinks it to the model's own
     *  largest size when it cannot. */
    /** A model or context change waiting for the user's yes, with what it would cost. */
    get costPending() { return costPending; },
    confirmCost() { const p = costPending; costPending = null; p?.apply(); },
    cancelCost() { costPending = null; },

    setModel(id: string) {
      const next = models.find((m) => m.id === id);
      if (!next || id === modelId) return;
      const change = confirmCostChanges && active
        ? modelChange({ used, inputPricePerM: next.inputPrice, eurPerUsd: limits.eur_per_usd, })
        : null;
      if (change) { costPending = { change, to: next.name, apply: () => this.applyModel(id) }; return; }
      this.applyModel(id);
    },

    applyModel(id: string) {
      const next = models.find((m) => m.id === id);
      if (!next) return;
      modelId = id;
      if (maxContext > next.contextMax) {
        maxContext = CONTEXT_SIZES.filter((s) => s <= next.contextMax).at(-1) ?? next.contextMax;
      }
      persist();
    },

    setMaxContext(tokens: number) {
      if (tokens > model.contextMax || tokens === maxContext) return;
      const change = confirmCostChanges && active
        ? contextChange({ used, inputPricePerM: model.inputPrice, eurPerUsd: limits.eur_per_usd, newSize: tokens })
        : null;
      const apply = () => { maxContext = tokens; persist(); };
      if (change) { costPending = { change, to: formatTokens(tokens), apply }; return; }
      apply();
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
      // Backends narrate long work on one topic. Only what arrives while one of Imber's tools runs is the tool's.
      // The topic says nothing of which run it is about: it goes to the open conversation's, when that one is in a tool.
      await onBackendProgress((p) => {
        const id = activeId;
        const r = id ? runs[id] : undefined;
        if (id && r && r.activity?.phase === 'tool' && p.message) runs = { ...runs, [id]: { ...r, toolProgress: p } };
      });
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

    /** Fetch a conversation again: the backend changed it behind the panel's back (an isolated copy was applied or discarded). */
    async reload(id: string) {
      const next = new Set(loaded);
      next.delete(id);
      loaded = next;
      await fetchConversation(id);
    },

    /** Back to the home screen of the current mode. */
    goHome() { activeId = null; },

    newChat() {
      newChatNow();
    },

    async rename(id: string, title: string) {
      const clean = title.trim();
      if (!clean) return;
      update(id, (c) => ({ ...c, title: clean }));
      await renameConversation(id, clean).catch(() => {});
    },

    /** The conversation waiting for a yes before it is deleted: every route to deleting one goes through here. */
    get deleteAsked() { return deleteAsked ? conversations.find((c) => c.id === deleteAsked) ?? null : null; },
    askRemove(id: string) { deleteAsked = id; },
    cancelRemove() { deleteAsked = null; },

    async remove(id: string) {
      deleteAsked = null;
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
    async send(text: string, opts: { retry?: boolean; attachments?: { params: AttachmentParam[]; items: AttachmentItem[] } } = {}): Promise<boolean> {
      const retry = opts.retry === true;
      const ctx = context();
      if (!ctx) {
        toastStore.show('Open a project first — Imber works inside one.', 'warning');
        return false;
      }
      if (running) return false;
      if (!active) this.newChat();
      const conv = conversations.find((c) => c.id === activeId)!;

      const params = buildParams(conv, ctx, text, retry);
      const attached = opts.attachments?.params.length ? opts.attachments : null;
      if (attached) params.attachments = attached.params;

      if (dryRun && !retry) {
        await this.showPreview(text);
        return false;
      }

      // The user's message appears at once; the run's reply streams in behind it. A retry has none: it picks up
      // the conversation where the failed request left it.
      const now = Date.now();
      if (!retry) {
        const blocks: MessageBlock[] = [];
        if (attached) blocks.push({ type: 'attachments', items: attached.items });
        if (text.trim()) blocks.push({ type: 'text', text });
        putMessage(conv.id, { id: `u${now}`, role: 'user', at: now, blocks });
      }
      try {
        const result = await sendAssistantMessage(params);
        if ('run_id' in result) {
          runs = {
            ...runs,
            [result.conversation_id]: {
              runId: result.run_id, startedAt: Date.now(), activity: { phase: 'thinking', iteration: 1 },
              toolProgress: null, stepStartedAt: Date.now(), retrying: null, notes: [],
            },
          };
          // The first words of a first message name the tab now; the model's `set_title` replaces them mid-run.
          const title = result.title;
          if (title) update(result.conversation_id, (c) => (c.title === 'New conversation' ? { ...c, title } : c));
        }
        return true;
      } catch (e) {
        toastStore.show(String(e), 'error');
        // A refused attachment (too large, binary) leaves the draft and the files where they were: nothing was sent.
        if (attached) {
          update(conv.id, (c) => ({ ...c, messages: c.messages.filter((m) => m.id !== `u${now}`) }));
          return false;
        }
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
          model: modelId, effort: effectiveEffort, mode, scope,
          allow_edits: allowEdits, allow_temp: allowTemp, extra_folders: extraFolders,
          workspace_roots: projectStore.workspaceRoots,
          max_projects: maxProjects,
          max_context: maxContext, periods: periodStarts(), chat: chatOnly, instructions: useInstructions, git: gitEnabled, hide_authors: hideAuthors, plan: planEnabled, families_off: familiesOff, bevy_write: bevyWrite, debug_mutate: debugMutate, skills_on: skillsOn, skill_folders: skillFolders, subagents_enabled: subagentsEnabled, chat_subagents: active?.subagents ?? '', dry_run: true,
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
      if (current) await cancelAssistantRun(current.runId).catch(() => {});
    },

    /** What the user wrote while the open conversation's run worked, not read by it yet. */
    get pendingNotes() { return current?.notes ?? []; },

    /**
     * Send a message while the run works: it reads it after the step it is in. Returns whether it was taken —
     * false when the run has just ended, and the text stays in the box to send as an ordinary message.
     */
    async sendNote(text: string): Promise<boolean> {
      const clean = text.trim();
      const conversationId = activeId;
      const run = conversationId ? runs[conversationId] : undefined;
      if (!conversationId || !run || !clean) return false;
      const id = `n${Date.now()}`;
      const taken = await sendAssistantNote(run.runId, id, clean).catch(() => false);
      if (taken) changeRun(conversationId, run.runId, (r) => ({ ...r, notes: [...r.notes, { id, text: clean }] }));
      return taken;
    },

    /** Skip the wait while the run is retrying: send the request again now. */
    async retryNow() {
      if (current?.retrying) await retryAssistantRunNow(current.runId).catch(() => {});
    },

    /** Send the failed request again, as the conversation stands: no new message from the user. */
    async retry(): Promise<boolean> {
      if (!canRetry) return false;
      return this.send('', { retry: true });
    },

    /** Take back a file the agent opened for itself. The run answers by pushing the thread with the grant revoked. */
    async revokeGrant(grantId: string) {
      if (!current) return;
      const done = await revokeAssistantGrant(current.runId, grantId).catch(() => false);
      if (!done) toastStore.show('That access has already ended.', 'info');
    },

    /** Apply or reject an edit proposal. */
    async resolveProposal(conversationId: string, proposalId: string, status: Extract<ProposalStatus, 'applied' | 'rejected'>) {
      let result: DecideResult;
      try {
        result = await decideProposal(conversationId, proposalId, status === 'applied');
      } catch (e) {
        // The proposal is still held by the backend (nothing was written, or what was is put back).
        toastStore.show(String(e), 'error');
        return;
      }
      changeProposal(conversationId, proposalId, (p) => withDecision(p, result));
      if (result.status === 'outdated') {
        toastStore.show('The file changed since this was proposed — nothing was written. Re-apply it on the current file, or ask Imber to redo it.', 'warning');
      }
    },

    /** Whether an applied proposal can still be undone from here (see the backend: it is kept in memory for a while). */
    async canUndo(proposalId: string): Promise<boolean> {
      return canUndoProposal(proposalId).catch(() => false);
    },

    /** Put back the files an applied proposal wrote, exactly as they were. All of them or none. */
    async undoProposal(conversationId: string, proposalId: string) {
      let result: UndoResult;
      try {
        result = await undoProposalIpc(conversationId, proposalId);
      } catch (e) {
        toastStore.show(String(e), 'error');
        return;
      }
      changeProposal(conversationId, proposalId, (p) => withUndo(p, result));
      if (result.status === 'undone') toastStore.show(`Put back ${result.files.length} file${result.files.length === 1 ? '' : 's'} as they were.`, 'info');
      else toastStore.show(`Not undone — changed since it was applied: ${result.stale.join(', ')}. Nothing was touched; Local History has the earlier text.`, 'warning');
    },

    /** Make an outdated proposal again on the files as they are now, to approve it from there. */
    async remakeProposal(conversationId: string, proposalId: string) {
      let result: RemakeResult;
      try {
        result = await remakeProposalIpc(conversationId, proposalId);
      } catch (e) {
        toastStore.show(`It no longer applies cleanly — ${e}`, 'warning');
        return;
      }
      changeProposal(conversationId, proposalId, (p) => withRemade(p, result));
      if (result.status === 'rejected') toastStore.show('Everything proposed is already in the files — nothing left to apply.', 'info');
      else if (result.already_there.length) toastStore.show(`Already in place, left out: ${result.already_there.join(', ')}.`, 'info');
    },

    /** Drop an outdated proposal and ask the assistant, in the conversation, to propose it again. */
    async askToRedo(conversationId: string, proposalId: string) {
      if (runs[conversationId]) {
        toastStore.show('Wait for Imber to finish, or stop it first.', 'warning');
        return;
      }
      const p = findProposal(conversationId, proposalId);
      if (!p) return;
      const request = redoRequest(p);
      await this.resolveProposal(conversationId, proposalId, 'rejected');
      if (activeId === conversationId) await this.send(request);
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

    /**
     * Answer a script the assistant wants to run. "Add" saves its declaration among the programs Imber may run
     * first — under a free name, so an existing program is never replaced — then lets the run go on.
     */
    async answerScript(conversationId: string, script: ScriptRequest, answer: ScriptAnswer) {
      if (answer === 'add') {
        const taken = new Set(externalTools.map((t) => t.name));
        let name = script.declaration.name;
        for (let n = 2; taken.has(name); n++) name = `${script.declaration.name}-${n}`;
        this.saveExternalTool(null, { ...script.declaration, name });
      }
      await reply(promptOf(conversationId, (b) => b.type === 'script' && b.script.id === script.id), { answer });
    },

    /**
     * Whether a run of this conversation is going and so could be waiting for an answer. A question still
     * unanswered when it is not (cancelled, timed out, or loaded back from disk) has no one to reach.
     */
    waitingOn(conversationId: string): boolean {
      return !!runs[conversationId];
    },
  };
}

export const bennuAssistantStore = createAssistantStore();
