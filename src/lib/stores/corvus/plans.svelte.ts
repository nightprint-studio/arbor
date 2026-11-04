import type { Plan, PlanRepo, PlanStep, StepSpec } from '$lib/types/corvus/plan';
import type { CreateMrParams } from '$lib/types/corvus/mr';
import {
  addPlanStep, copyPlan, createPlan, deletePlan, listPlans, releasePlanGate, retryPlanStep, runPlan,
  savePlan, skipPlanStep, stopPlan, type AddStepOptions,
} from '$lib/ipc/corvus/plan';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';
import { uiStore } from '../ui.svelte';
import { tabsStore } from './tabs.svelte';
import { workspacesStore } from './workspaces.svelte';

/** The repo behind the active tab, in the shape a plan step carries. */
export function activeRepoRef(): PlanRepo | null {
  const tab = tabsStore.activeTab;
  if (!tab) return null;
  return { path: tab.path, name: tab.name, workspace: workspacesStore.active?.name ?? null };
}

/** A Create-MR form to fill in plan mode: which repo it is for, and where in the plan the step lands. */
export interface MrDraft {
  repo: PlanRepo;
  /** Tab id (or `path:<dir>` headless id) the modal reads branches from. */
  tabId: string;
  source?: string;
  /** The branch is not on the remote yet, so the plan gets its push too — one phase earlier. */
  pushFirst?: boolean;
  planId?: string;
  phaseId?: string;
  /** Put the MR in a phase of its own, after the last — it must wait for the push before it. */
  newPhaseAfter?: boolean;
  /** Called once the modal is gone: `true` when the MR joined the plan, `false` when it was cancelled. */
  onDone?: (added: boolean) => void;
}

/** The plan step a Create-MR form describes. */
const mrSpec = (p: CreateMrParams): StepSpec => ({
  kind: 'create_mr',
  source: p.sourceBranch,
  target: p.targetBranch,
  title: p.title,
  description: p.description ?? '',
  labels: p.labels,
  draft: p.isDraft,
  auto_merge: p.autoMerge,
  squash: p.squash,
  delete_branch: p.deleteBranch,
});

/** Plans a user can still add steps to: the one being composed or one paused on a problem. */
const isOpen = (p: Plan) => !p.template && (p.status === 'draft' || p.status === 'paused');

function createPlansStore() {
  let plans = $state<Plan[]>([]);
  let loaded = $state(false);
  /** The plan shown in the big modal, or null when it is closed. `''` opens it on the plan being composed. */
  let modalPlanId = $state<string | null>(null);
  /** The Create-MR modal opened in plan mode, or null. */
  let mrDraft = $state<MrDraft | null>(null);
  /** The plan the user asked to delete, until they confirm or cancel. */
  let deleting = $state<Plan | null>(null);

  const upsert = (plan: Plan) => {
    const i = plans.findIndex(p => p.id === plan.id);
    if (i >= 0) plans[i] = plan;
    else plans.push(plan);
  };

  const current = $derived(
    plans.filter(isOpen).sort((a, b) => b.updated_ms - a.updated_ms)[0] ?? null,
  );

  /** Run a backend call and report a failure as a toast — the plan itself arrives by event. */
  async function attempt<T>(what: string, call: () => Promise<T>): Promise<T | null> {
    try {
      return await call();
    } catch (e) {
      uiStore.showToast(`${what}: ${e}`, 'error');
      return null;
    }
  }

  const store = {
    get plans() { return plans; },
    get loaded() { return loaded; },
    get modalPlanId() { return modalPlanId; },
    get mrDraft() { return mrDraft; },
    get deleting() { return deleting; },
    /** The plan entry points add to when the user does not pick one. */
    get current() { return current; },
    get running() { return plans.filter(p => p.status === 'running'); },
    get attention() { return plans.filter(p => p.status === 'paused'); },

    async load() {
      try { plans = await listPlans(); } catch { /* the backend is not up yet; events will fill it */ }
      loaded = true;
    },

    setupListeners(): () => void {
      return setupTauriListeners([
        { event: 'arbor://plan-updated', handler: (e: { payload: Plan }) => upsert(e.payload) },
        {
          event: 'arbor://plan-removed',
          handler: (e: { payload: { id: string } }) => { plans = plans.filter(p => p.id !== e.payload.id); },
        },
      ]);
    },

    openModal(planId = '') { modalPlanId = planId; },
    closeModal() { modalPlanId = null; },
    openMrDraft(draft: MrDraft) { mrDraft = draft; },
    closeMrDraft() { mrDraft = null; },

    /** Add a step to a plan (the one being composed by default) and say where it went. */
    addStep: (repo: PlanRepo, spec: StepSpec, opts: AddStepOptions = {}) => store.addSteps([{ repo, spec }], opts),

    /** Add several steps in a row to the same plan and phase, with one toast for the lot. */
    async addSteps(items: { repo: PlanRepo; spec: StepSpec }[], opts: AddStepOptions = {}): Promise<Plan | null> {
      let plan: Plan | null = null;
      let where = opts;
      for (const { repo, spec } of items) {
        const at = { ...where, planId: plan?.id ?? where.planId };
        plan = await attempt('Could not add to the plan', () => addPlanStep(repo, spec, at));
        if (!plan) return null;
        // The first step of a "new phase" opens it; the rest of the batch joins that phase.
        if (where.newPhaseAfter) where = { ...where, newPhaseAfter: false, phaseId: plan.phases.at(-1)?.id };
      }
      if (!plan) return null;
      const target = plan;
      const what = items.length === 1 ? 'Added to' : `Added ${items.length} steps to`;
      uiStore.showToast(`${what} ${target.name}`, 'success', 3500, { label: 'Open', onClick: () => { modalPlanId = target.id; } });
      return target;
    },

    /** Queue a push of `branch` instead of running it. */
    addPush: (repo: PlanRepo, branch: string, opts: AddStepOptions = {}) =>
      store.addStep(repo, { kind: 'push', branch }, opts),

    /** Queue a Create-MR form; when the branch is not pushed yet its push goes in a phase before. */
    async addMergeRequest(draft: MrDraft, params: CreateMrParams): Promise<Plan | null> {
      const where = { planId: draft.planId, phaseId: draft.phaseId, newPhaseAfter: draft.newPhaseAfter };
      if (draft.pushFirst) {
        const pushed = await store.addPush(draft.repo, params.sourceBranch, { planId: draft.planId, phaseId: draft.phaseId });
        if (!pushed) return null;
        return store.addStep(draft.repo, mrSpec(params), { planId: pushed.id, newPhaseAfter: true });
      }
      return store.addStep(draft.repo, mrSpec(params), where);
    },

    toastError: (message: string) => uiStore.showToast(message, 'error'),
    save: (plan: Plan) => attempt('Could not save the plan', () => savePlan(plan)),
    create: (name?: string) => attempt('Could not create the plan', () => createPlan(name)),
    copy: (id: string, opts: { name?: string; template?: boolean } = {}) =>
      attempt('Could not copy the plan', () => copyPlan(id, opts)),
    /** Ask for confirmation before deleting; the confirm modal is hosted by `PlanHost`. */
    requestDelete(plan: Plan) { if (plan.status !== 'running') deleting = plan; },
    cancelDelete() { deleting = null; },
    async confirmDelete() {
      const plan = deleting;
      deleting = null;
      if (!plan) return;
      if (await store.remove(plan.id) !== null && modalPlanId === plan.id) modalPlanId = null;
    },
    remove: (id: string) => attempt('Could not delete the plan', () => deletePlan(id)),
    run: (id: string) => attempt('Could not run the plan', () => runPlan(id)),
    stop: (id: string) => attempt('Could not stop the plan', () => stopPlan(id)),
    retryStep: (id: string, stepId: string) => attempt('Could not retry the step', () => retryPlanStep(id, stepId)),
    skipStep: (id: string, stepId: string) => attempt('Could not skip the step', () => skipPlanStep(id, stepId)),
    releaseGate: (id: string, phaseId: string) => attempt('Could not release the gate', () => releasePlanGate(id, phaseId)),
  };
  return store;
}

export const plansStore = createPlansStore();

/** Every step of a plan, `finally` included, in reading order. */
export const planSteps = (plan: Plan): PlanStep[] => [...plan.phases.flatMap(p => p.steps), ...plan.finally];
