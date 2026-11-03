<script lang="ts">
  /**
   * The plan, large: phases left to right, the gates between them, a `finally` at the end. Compose
   * it with the keyboard (arrows, A, G, Delete, Alt+arrows), run it with Ctrl+Enter, and watch it
   * run on the same board.
   */
  import { ListChecks, Play, RotateCcw, Square, Trash2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import PromptModal from '$lib/components/shared/PromptModal.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import type { GateSpec, Plan, PlanStep, StepKind } from '$lib/types/corvus/plan';
  import { plansStore } from '$lib/stores/corvus/plans.svelte';
  import PlanAddStep, { type PickedRepo } from './PlanAddStep.svelte';
  import PlanBoard from './PlanBoard.svelte';
  import { FINALLY, columns, locate, moveAcross, moveWithin, neighbour, removeStep, setGate } from './plan-edit';
  import { defaultSpec } from './plan-defaults';
  import { GATE_META, STEP_META, earlierMrSteps, stepDetail } from './plan-meta';

  const plan = $derived(plansStore.plans.find(p => p.id === plansStore.modalPlanId) ?? null);
  const interactive = $derived(plan !== null && plan.status !== 'running');

  let selection = $state<string | null>(null);
  let adding = $state<{ columnId: string; kind: StepKind | null } | null>(null);
  let gateMenu = $state<{ phaseId: string; x: number; y: number } | null>(null);
  let tagPrompt = $state<{ columnId: string; repos: PickedRepo[] } | null>(null);

  // The palette and the gate menu are not modals, so the Escape that dismisses one also reaches
  // this modal's own handler. A close that follows an overlay's by a few ms is that same keystroke.
  let overlayClosedAt = 0;
  const overlayClosed = () => { overlayClosedAt = performance.now(); };
  function closeModal() {
    if (performance.now() - overlayClosedAt < 150) return;
    plansStore.closeModal();
  }

  const selectedStep = $derived<PlanStep | null>(
    plan && selection && !selection.startsWith('gate:') ? columns(plan).flatMap(c => c.steps).find(s => s.id === selection) ?? null : null,
  );

  // `openModal('')` means "the plan being composed": resolve it, or start a new one.
  let resolving = false;
  $effect(() => {
    if (plansStore.modalPlanId !== '' || resolving) return;
    resolving = true;
    void (async () => {
      const target = plansStore.current ?? (await plansStore.create());
      if (target) plansStore.openModal(target.id);
      else plansStore.closeModal();
      resolving = false;
    })();
  });

  // Start on the first step, so the arrows work straight away.
  $effect(() => {
    if (plan && !selection) selection = columns(plan).flatMap(c => c.steps)[0]?.id ?? null;
  });

  /** Clone the plan, apply `fn`, save. The backend validates; a refusal arrives as a toast. */
  async function edit(fn: (draft: Plan) => boolean | void) {
    if (!plan || !interactive) return;
    const draft = $state.snapshot(plan) as Plan;
    if (fn(draft) === false) return;
    await plansStore.save(draft);
  }

  // ── adding steps ──────────────────────────────────────────────────────────

  function openAdd(columnId?: string) {
    if (!plan || !interactive) return;
    const at = columnId ?? (selection && locate(plan, selection) ? columns(plan)[locate(plan, selection)!.column].id : plan.phases.at(-1)?.id ?? FINALLY);
    adding = { columnId: at, kind: null };
  }

  /** `finally` is not a phase: the backend takes it as a flag. */
  const placement = (columnId: string) => (columnId === FINALLY ? { toFinally: true } : { phaseId: columnId });

  async function onPick(kind: StepKind, repos: PickedRepo[]) {
    if (!plan || !adding) return;
    const columnId = adding.columnId;
    overlayClosed();
    adding = null;
    if (kind === 'create_mr') {
      if (columnId === FINALLY) return plansStore.toastError('A merge request is created in a phase, not in Finally');
      return startMrChain(columnId, repos);
    }
    if (kind === 'tag') {
      tagPrompt = { columnId, repos };
      return;
    }
    const index = columnId === FINALLY ? plan.phases.length : plan.phases.findIndex(p => p.id === columnId);
    const items = [];
    for (const r of repos) {
      const d = defaultSpec(kind, r, plan, index);
      if ('error' in d) plansStore.toastError(d.error);
      else items.push({ repo: r, spec: d.spec });
    }
    if (items.length) await plansStore.addSteps(items, { planId: plan.id, ...placement(columnId) });
  }

  async function addTags(name: string) {
    const prompt = tagPrompt;
    tagPrompt = null;
    if (!plan || !prompt) return;
    const items = prompt.repos.map(repo => ({ repo, spec: { kind: 'tag', name, message: null, target: repo.branch } as const }));
    await plansStore.addSteps(items, { planId: plan.id, ...placement(prompt.columnId) });
  }

  /** The Create-PR form once per repo: each Add moves on to the next, Cancel ends the chain. */
  function startMrChain(columnId: string, repos: PickedRepo[]) {
    if (!plan) return;
    const planId = plan.id;
    const [head, ...rest] = repos;
    if (!head) return;
    plansStore.openMrDraft({
      repo: head,
      tabId: `path:${head.path}`,
      source: head.branch ?? undefined,
      planId,
      ...(columnId === FINALLY ? {} : { phaseId: columnId }),
      onDone: (added) => { if (added && rest.length) startMrChain(columnId, rest); },
    });
  }

  async function addPhase() {
    // A phase is born with its first step; ask for one.
    if (!plan || !interactive) return;
    await edit(d => {
      d.phases.push({ id: `ph-new-${Date.now().toString(36)}`, name: `Phase ${d.phases.length + 1}`, gate: { kind: 'phase_done', state: 'idle' }, steps: [] });
    });
    const next = plansStore.plans.find(p => p.id === plan?.id)?.phases.at(-1);
    if (next) adding = { columnId: next.id, kind: null };
  }

  // ── gates ─────────────────────────────────────────────────────────────────

  function openGate(phaseId: string, at: { x: number; y: number }) {
    if (interactive) gateMenu = { phaseId, ...at };
  }

  const gateItems = $derived.by<MenuItem[]>(() => {
    if (!plan || !gateMenu) return [];
    const index = plan.phases.findIndex(p => p.id === gateMenu!.phaseId);
    const items: MenuItem[] = [{ id: 'phase_done', label: GATE_META.phase_done.label, icon: GATE_META.phase_done.icon }];
    for (const mr of earlierMrSteps(plan, index)) {
      if (mr.kind !== 'create_mr') continue;
      items.push({ id: `mr:${mr.id}`, label: `MR “${mr.title}” is merged (${mr.repo.name})`, icon: GATE_META.mr_merged.icon });
    }
    items.push(
      { id: 'delay:300', label: 'After 5 minutes', icon: GATE_META.delay.icon },
      { id: 'delay:1800', label: 'After 30 minutes', icon: GATE_META.delay.icon },
      { id: 'manual', label: GATE_META.manual.label, icon: GATE_META.manual.icon },
    );
    return items;
  });

  function onGatePick(id: string) {
    const phaseId = gateMenu?.phaseId;
    overlayClosed();
    gateMenu = null;
    if (!phaseId) return;
    let spec: GateSpec;
    if (id === 'phase_done' || id === 'manual') spec = { kind: id };
    else if (id.startsWith('delay:')) spec = { kind: 'delay', secs: Number(id.slice(6)) };
    else spec = { kind: 'mr_merged', step_id: id.slice(3), poll_secs: 30, timeout_secs: 7200 };
    void edit(d => setGate(d, phaseId, spec));
  }

  // ── running ───────────────────────────────────────────────────────────────

  const canRun = $derived(
    !!plan && plan.status !== 'running' && (plan.phases.some(p => p.steps.length > 0)),
  );
  const runLabel = $derived(plan?.status === 'paused' ? 'Resume' : plan && ['done', 'failed', 'cancelled'].includes(plan.status) ? 'Run again' : 'Run');

  function run() {
    if (plan && canRun) void plansStore.run(plan.id);
  }

  // ── keyboard ──────────────────────────────────────────────────────────────

  function onKeydown(e: KeyboardEvent) {
    if (!plan || adding || gateMenu || tagPrompt || plansStore.mrDraft) return;
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;

    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); run(); return; }
    if (e.ctrlKey || e.metaKey) return;

    const arrow = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key] as [number, number] | undefined;
    if (arrow) {
      e.preventDefault();
      const [dx, dy] = arrow;
      if (e.altKey && selection && interactive) {
        const id = selection;
        void edit(d => (dy ? moveWithin(d, id, dy as -1 | 1) : moveAcross(d, id, dx as -1 | 1)));
      } else {
        selection = neighbour(plan, selection, dx, dy);
        scrollToSelection();
      }
      return;
    }
    const key = e.key.toLowerCase();
    if (key === 'a') { e.preventDefault(); openAdd(); }
    else if (key === 'g') { e.preventDefault(); openGateForSelection(); }
    else if ((e.key === 'Delete' || e.key === 'Backspace') && selection && interactive) { e.preventDefault(); removeSelected(); }
  }

  function scrollToSelection() {
    queueMicrotask(() => document.querySelector(`[data-step-id="${selection}"]`)?.scrollIntoView({ block: 'nearest', inline: 'center' }));
  }

  /** G edits the condition that starts the selected step's phase (the next one for the first phase). */
  function openGateForSelection() {
    if (!plan || !interactive) return;
    const at = selection ? locate(plan, selection) : null;
    let index = at ? at.column : 1;
    if (index === 0) index = 1;
    const phase = plan.phases[index];
    if (!phase) return;
    const el = (selection && document.querySelector(`[data-step-id="${selection}"]`)) as HTMLElement | null;
    const r = el?.getBoundingClientRect();
    selection = `gate:${phase.id}`;
    gateMenu = { phaseId: phase.id, x: r ? r.left : 200, y: r ? r.bottom + 6 : 200 };
  }

  function removeSelected() {
    const id = selection;
    if (!id || !plan) return;
    const next = neighbour(plan, id, 0, 1) !== id ? neighbour(plan, id, 0, 1) : neighbour(plan, id, 0, -1);
    selection = next === id ? null : next;
    void edit(d => removeStep(d, id));
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if plan}
  <Modal onClose={closeModal} width="min(1400px, 96vw)" height="min(820px, 92vh)" padBody={false} ariaLabel="Plan">
    {#snippet header()}
      <ModalHeader onClose={closeModal}>
        <ListChecks size={15} />
        <span class="modal-title">{plan.name}</span>
        <Badge
          variant="tone"
          size="sm"
          tone={plan.status === 'running' ? 'accent' : plan.status === 'paused' || plan.status === 'failed' ? 'warning' : plan.status === 'done' ? 'success' : 'neutral'}
          label={plan.status}
        />
      </ModalHeader>
    {/snippet}

    <div class="layout">
      <div class="main">
        {#if plan.status === 'paused' && plan.pause_reason}
          <div class="banner"><Alert variant="warning" compact text={plan.pause_reason} /></div>
        {/if}
        <div class="scroll">
          <PlanBoard
            {plan}
            {selection}
            {interactive}
            onSelect={(id) => (selection = id)}
            onAddStep={openAdd}
            onAddPhase={addPhase}
            onEditGate={openGate}
            onReleaseGate={(phaseId) => plansStore.releaseGate(plan.id, phaseId)}
            onRetry={(id) => plansStore.retryStep(plan.id, id)}
            onSkip={(id) => plansStore.skipStep(plan.id, id)}
          />
        </div>
        <div class="keys">
          <span><Kbd keys={['←', '→', '↑', '↓']} /> navigate</span>
          <span><Kbd keys={['A']} /> add step</span>
          <span><Kbd keys={['G']} /> start condition</span>
          <span><Kbd keys={['Alt', '←→↑↓']} /> move</span>
          <span><Kbd keys={['Del']} /> remove</span>
        </div>
      </div>

      {#if selectedStep}
        <aside class="inspector">
          <div class="i-head">
            <span>{STEP_META[selectedStep.kind].label}</span>
          </div>
          <dl>
            <dt>Repository</dt><dd>{selectedStep.repo.name}</dd>
            {#if selectedStep.repo.workspace}<dt>Workspace</dt><dd>{selectedStep.repo.workspace}</dd>{/if}
            <dt>Does</dt><dd class="mono">{stepDetail(selectedStep)}</dd>
            {#if selectedStep.result?.url}<dt>Result</dt><dd class="mono">{selectedStep.result.url}</dd>{/if}
            {#if selectedStep.error}<dt>Error</dt><dd class="mono err">{selectedStep.error}</dd>{/if}
          </dl>
          {#if interactive}
            <div class="i-actions">
              {#if selectedStep.status === 'failed' || selectedStep.status === 'skipped'}
                <Button size="sm" variant="secondary" onclick={() => plansStore.retryStep(plan.id, selectedStep!.id)}>
                  <RotateCcw size={12} /> Retry
                </Button>
              {/if}
              <Button size="sm" variant="ghost" onclick={removeSelected}><Trash2 size={12} /> Remove</Button>
            </div>
          {/if}
        </aside>
      {/if}
    </div>

    {#snippet footer()}
      <ModalFooter hint="to run">
        {#if plan.status === 'running'}
          <Button variant="secondary" onclick={() => plansStore.stop(plan.id)}><Square size={12} /> Stop</Button>
        {:else}
          {#if plan.status === 'paused'}
            <Button variant="ghost" onclick={() => plansStore.stop(plan.id)}><Square size={12} /> Stop &amp; finish</Button>
          {/if}
          <Button variant="primary" disabled={!canRun} onclick={run}><Play size={12} /> {runLabel}</Button>
        {/if}
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

{#if adding}
  <PlanAddStep kind={adding.kind} onPick={onPick} onClose={() => { overlayClosed(); adding = null; }} />
{/if}

{#if gateMenu}
  <ContextMenu items={gateItems} x={gateMenu.x} y={gateMenu.y} onSelect={onGatePick} onClose={() => { overlayClosed(); gateMenu = null; }} />
{/if}

{#if tagPrompt}
  <PromptModal
    title="Tag"
    label="Tag name:"
    placeholder="v1.0.0"
    confirmLabel="Add"
    onConfirm={addTags}
    onCancel={() => (tagPrompt = null)}
  />
{/if}

<style>
  .layout { display: flex; height: 100%; min-height: 0; }
  .main { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background-image: radial-gradient(color-mix(in srgb, var(--text-muted) 14%, transparent) 1px, transparent 1.2px);
    background-size: 22px 22px;
  }
  .banner { padding: 10px 16px 0; }
  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    padding: 7px 16px;
    border-top: 1px solid var(--border-subtle);
    color: var(--text-muted);
    font-size: 11.5px;
  }
  .keys span { display: inline-flex; align-items: center; gap: 6px; }

  .inspector { width: 300px; flex: none; padding: 14px 16px; border-left: 1px solid var(--border-subtle); overflow: auto; }
  .i-head { font-weight: 600; margin-bottom: 10px; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 6px 12px; margin: 0; font-size: var(--font-size-sm); }
  dt { color: var(--text-muted); }
  dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
  .mono { font-family: var(--font-ui-mono, monospace); font-size: 12px; }
  .err { color: var(--danger); white-space: pre-wrap; }
  .i-actions { display: flex; gap: 6px; margin-top: 14px; }
</style>
