<!--
  PipelineSequence — middle column. Renders the optional breadcrumb (drill-
  down nav), the "+ stage / import stage" header buttons, and the list of
  stages. Each stage contains a flat list of steps; an `if_block` step
  embeds collapsible branches that recursively reuse the same step row
  snippet.

  Owns the `exprBuffer` local state used by the branch expression input
  (Svelte 5 doesn't let us bind:value to a derived prop without losing
  changes on the next prop refresh).
-->
<script lang="ts">
  import {
    Plus, Minus,
    ChevronUp, ChevronDown, ChevronRight, Copy, Settings2,
    Upload, Download,
    AlignHorizontalSpaceBetween, AlignVerticalSpaceBetween,
    ArrowLeft, FolderOpen, Home,
  } from 'lucide-svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  import type { Stage, Step, Branch, OpCategory, Crumb, FireAction } from './types';
  import { catColor, iconFor, forEachBranch, makeFire } from './helpers';

  interface Props {
    stages:             Stage[];
    operations:         OpCategory[];
    selectedStepId?:    string;
    selectedStageId?:   string;
    selectedBranchId?:  string;
    breadcrumb?:        Crumb[];
    hideAddStage?:      boolean;
    actions:            Record<string, string>;
    iconMap?:           Record<string, any>;
    fireAction:         FireAction;
  }
  let {
    stages,
    operations,
    selectedStepId,
    selectedStageId,
    selectedBranchId,
    breadcrumb,
    hideAddStage,
    actions,
    iconMap,
    fireAction,
  }: Props = $props();

  const fire = $derived(makeFire(actions, fireAction));

  // ── Inline expression editor — local buffer per branch ───────────────────
  // bind:value on a prop is one-way only: typing in the input would write
  // back to a derived prop and lose changes on the next prop refresh. Keep a
  // local map keyed by branch id; seed/refresh it via `$effect` (mutating
  // $state inside a template expression like `value={…}` is illegal under
  // Svelte 5 — see https://svelte.dev/e/state_unsafe_mutation).
  let exprBuffer = $state<Record<string, string>>({});

  // Seed the buffer for newly-arrived branches and re-sync any whose
  // upstream value changed externally (re-import, undo). We deliberately
  // skip overriding the value while the user is mid-typing in that
  // particular input (focus check on the matching `id`).
  $effect(() => {
    const seen = new Set<string>();
    for (const stage of stages) {
      forEachBranch(stage.steps, (b) => {
        seen.add(b.id);
        const focused = typeof document !== 'undefined'
          && document.activeElement?.id === `pe-expr-${b.id}`;
        const upstream = b.expression ?? '';
        if (!(b.id in exprBuffer) || (!focused && exprBuffer[b.id] !== upstream)) {
          exprBuffer[b.id] = upstream;
        }
      });
    }
    // Garbage-collect entries for branches that no longer exist (else the
    // buffer grows unbounded across edits).
    for (const k of Object.keys(exprBuffer)) {
      if (!seen.has(k)) delete exprBuffer[k];
    }
  });

  function exprValue(branch: Branch): string {
    return exprBuffer[branch.id] ?? branch.expression ?? '';
  }

  function commitExpression(stageId: string, stepId: string, branch: Branch) {
    const next = exprBuffer[branch.id] ?? '';
    if (next === (branch.expression ?? '')) return;
    fire('update_branch_expression', {
      stage_id: stageId, step_id: stepId, branch_id: branch.id, expr: next,
    });
  }
</script>

<!--
  Recursive snippet for a single step row. Reused at every depth — top-level
  stage steps AND nested children inside an `if_block` branch. The plugin
  emits a globally-unique `step.id` so the same handler set
  (move_step_up/down, remove_step, …) works at any depth without needing a
  branch path on the action payload — the plugin walks the tree by id.
-->
{#snippet stepRow(step: Step, stageId: string)}
  {@const op = operations.flatMap(c => Array.isArray(c.ops) ? c.ops : []).find(o => o.kind === step.kind)}
  {@const StepIcon = iconFor(iconMap, op?.icon)}
  {@const isSel = step.id === selectedStepId}
  <li class="pe-step"
      class:pe-step-selected={isSel}
      class:pe-step-has-body={step.has_body}
      style="--cat-color: {catColor(op?.category)};">
    <button type="button" class="pe-step-main"
            onclick={() => fire('select_step', { stage_id: stageId, step_id: step.id })}>
      <StepIcon size={13} class="pe-step-icon" />
      <span class="pe-step-name">{step.name || step.id}</span>
      <span class="pe-step-kind">{op?.label ?? step.kind}</span>
      {#if step.allow_failure}
        <span class="pe-step-flag" use:tooltip={'Allow failure'}>⚠</span>
      {/if}
      {#if step.summary}
        <span class="pe-step-summary" title={step.summary}>{step.summary}</span>
      {/if}
    </button>
    <div class="pe-row-actions pe-step-actions">
      {#if step.has_body && actions?.enter_step}
        <button type="button" class="pe-step-enter"
                use:tooltip={'Apri il blocco'}
                onclick={(e) => { e.stopPropagation(); fire('enter_step', { stage_id: stageId, step_id: step.id }); }}>
          <FolderOpen size={11} />
        </button>
      {/if}
      <button type="button" use:tooltip={'Su'}
              onclick={(e) => { e.stopPropagation(); fire('move_step_up', { stage_id: stageId, step_id: step.id }); }}>
        <ChevronUp size={11} />
      </button>
      <button type="button" use:tooltip={'Giù'}
              onclick={(e) => { e.stopPropagation(); fire('move_step_down', { stage_id: stageId, step_id: step.id }); }}>
        <ChevronDown size={11} />
      </button>
      <button type="button" use:tooltip={'Duplica'}
              onclick={(e) => { e.stopPropagation(); fire('duplicate_step', { stage_id: stageId, step_id: step.id }); }}>
        <Copy size={11} />
      </button>
      <button type="button" class="pe-row-act-danger" use:tooltip={'Rimuovi'}
              onclick={(e) => { e.stopPropagation(); fire('remove_step', { stage_id: stageId, step_id: step.id }); }}>
        <Minus size={11} />
      </button>
    </div>
  </li>

  {#if Array.isArray(step.branches) && step.branches.length > 0}
    <li class="pe-branches" aria-label="If-block branches">
      {#each step.branches as branch (branch.id)}
        {@render branchSection(step, branch, stageId, false)}
      {/each}
      {#if step.else_branch}
        {@render branchSection(step, step.else_branch, stageId, true)}
      {/if}
      <div class="pe-branch-add-row">
        {#if actions?.add_elif_branch}
          <button type="button" class="pe-branch-add-btn"
                  onclick={() => fire('add_elif_branch', { stage_id: stageId, step_id: step.id })}>
            <Plus size={11} /> elif
          </button>
        {/if}
        {#if !step.else_branch && actions?.add_else_branch}
          <button type="button" class="pe-branch-add-btn"
                  onclick={() => fire('add_else_branch', { stage_id: stageId, step_id: step.id })}>
            <Plus size={11} /> else
          </button>
        {/if}
      </div>
    </li>
  {/if}
{/snippet}

<!--
  Single branch (if / elif / else) inside an if_block step. Renders through
  the shared `Collapsible` primitive (chevron rotation + slide+overflow:hidden
  body — the latter is what eliminates the "snap-at-end" artefact a
  hand-rolled `transition:slide` on a non-clipping list would produce).
  Clicking anywhere on the header BOTH toggles collapse AND sets the branch
  as the active palette drop target — natural pairing since you usually
  expand a branch when you're about to add steps to it. Form controls inside
  the header (expression input, remove button) call `e.stopPropagation()`
  so they don't trigger toggle/select.
-->
{#snippet branchSection(parent: Step, branch: Branch, stageId: string, isElse: boolean)}
  {@const isActiveBranch = selectedBranchId === branch.id}
  <section class="pe-branch" class:pe-branch-active={isActiveBranch}>
    <Collapsible
      chevron
      open={!branch.collapsed}
      onopen={() => fire('toggle_branch', { stage_id: stageId, step_id: parent.id, branch_id: branch.id })}
      onclose={() => fire('toggle_branch', { stage_id: stageId, step_id: parent.id, branch_id: branch.id })}
    >
      {#snippet header()}
        <div class="pe-branch-header-row"
             onclick={() => fire('select_branch', { stage_id: stageId, step_id: parent.id, branch_id: branch.id })}
             role="presentation">
          <span class="pe-branch-label" class:pe-branch-label-else={isElse}>{branch.label}</span>
          {#if !isElse}
            <input type="text"
                   id="pe-expr-{branch.id}"
                   class="pe-branch-expr"
                   value={exprValue(branch)}
                   oninput={(e) => { exprBuffer[branch.id] = (e.target as HTMLInputElement).value; }}
                   onclick={(e) => e.stopPropagation()}
                   onblur={() => commitExpression(stageId, parent.id, branch)}
                   onkeydown={(e) => {
                     e.stopPropagation();
                     if (e.key === 'Enter') { (e.target as HTMLInputElement).blur(); }
                     if (e.key === 'Escape') { exprBuffer[branch.id] = branch.expression ?? ''; (e.target as HTMLInputElement).blur(); }
                   }}
                   placeholder={'es. ${var} == "value"'}
                   spellcheck="false" autocomplete="off" />
          {/if}
          <span class="pe-branch-count">{Array.isArray(branch.steps) ? branch.steps.length : 0} step</span>
          <button type="button" class="pe-row-act-danger pe-branch-rm"
                  use:tooltip={isElse ? 'Rimuovi else' : 'Rimuovi ramo'}
                  onclick={(e) => { e.stopPropagation(); fire('remove_branch', { stage_id: stageId, step_id: parent.id, branch_id: branch.id }); }}>
            <Minus size={11} />
          </button>
        </div>
      {/snippet}
      {#snippet children()}
        {@const branchSteps = Array.isArray(branch.steps) ? branch.steps : []}
        <ul class="pe-step-list pe-branch-list">
          {#if branchSteps.length === 0}
            <li class="pe-branch-empty">
              {#if isActiveBranch}
                Ramo selezionato. Clicca un'op nella palette per aggiungerla qui.
              {:else}
                Vuoto. Clicca l'header per selezionare il ramo, poi scegli un'op dalla palette.
              {/if}
            </li>
          {:else}
            {#each branchSteps as childStep (childStep.id)}
              {@render stepRow(childStep, stageId)}
            {/each}
          {/if}
        </ul>
      {/snippet}
    </Collapsible>
  </section>
{/snippet}

<section class="pe-col pe-col-sequence">
  <header class="pe-col-header">
    {#if breadcrumb && breadcrumb.length > 1}
      <button class="pe-head-btn pe-head-back" type="button"
              use:tooltip={'Torna indietro'}
              onclick={() => fire('navigate_to', { level: (breadcrumb!.length - 2) })}>
        <ArrowLeft size={14} />
      </button>
      <nav class="pe-breadcrumb" aria-label="Pipeline path">
        {#each breadcrumb as crumb, i (i + ':' + crumb.label)}
          {@const last = i === breadcrumb!.length - 1}
          {#if i > 0}<ChevronRight size={11} class="pe-bc-sep" />{/if}
          {#if last}
            <span class="pe-bc-current">{crumb.label}</span>
          {:else}
            <button class="pe-bc-crumb" type="button"
                    onclick={() => fire('navigate_to', { level: i })}>
              {#if i === 0}<Home size={11} />{/if}
              {crumb.label}
            </button>
          {/if}
        {/each}
      </nav>
    {:else}
      <span class="pe-col-title">Sequenza operazioni</span>
    {/if}
    <span class="pe-col-spacer"></span>
    {#if actions?.import_stage && !hideAddStage}
      <button class="pe-head-btn" type="button" use:tooltip={'Importa gruppo da JSON'}
              onclick={() => fire('import_stage')}>
        <Download size={13} />
      </button>
    {/if}
    {#if !hideAddStage}
      <button class="pe-head-btn" type="button" use:tooltip={'Aggiungi gruppo'}
              onclick={() => fire('add_stage')}>
        <Plus size={14} />
      </button>
    {/if}
  </header>

  <div class="pe-seq-body">
    {#if stages.length === 0}
      <div class="pe-seq-empty">
        <p>Nessun gruppo definito.</p>
        <p class="pe-muted">Aggiungi un gruppo e trascina le operazioni dalla palette.</p>
        <button class="pe-btn-primary" type="button" onclick={() => fire('add_stage')}>
          <Plus size={13} /> Crea primo gruppo
        </button>
      </div>
    {:else}
      {#each stages as stage, si (stage.id)}
        {@const ModeIcon = stage.mode === 'parallel'
          ? AlignHorizontalSpaceBetween : AlignVerticalSpaceBetween}
        {@const stageSteps = Array.isArray(stage.steps) ? stage.steps : []}
        <div class="pe-stage"
             class:pe-stage-selected={stage.id === selectedStageId}>
          <header class="pe-stage-header"
                  onclick={() => fire('select_stage', { stage_id: stage.id })}
                  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); fire('select_stage', { stage_id: stage.id }); } }}
                  role="button" tabindex="0"
                  use:tooltip={{ content: 'Clic per selezionare il gruppo', description: 'Le nuove operazioni vengono aggiunte qui' }}>
            <span class="pe-stage-idx">{si + 1}</span>
            <ModeIcon size={13} class="pe-stage-mode" />
            <span class="pe-stage-name">{stage.name || stage.id}</span>
            <span class="pe-stage-count">{Array.isArray(stage.steps) ? stage.steps.length : 0} step</span>
            <span class="pe-col-spacer"></span>
            <div class="pe-row-actions">
              <button type="button" use:tooltip={'Su'}
                      onclick={(e) => { e.stopPropagation(); fire('move_stage_up', { stage_id: stage.id }); }}>
                <ChevronUp size={12} />
              </button>
              <button type="button" use:tooltip={'Giù'}
                      onclick={(e) => { e.stopPropagation(); fire('move_stage_down', { stage_id: stage.id }); }}>
                <ChevronDown size={12} />
              </button>
              {#if actions?.export_stage}
                <button type="button" use:tooltip={'Esporta gruppo (JSON)'}
                        onclick={(e) => { e.stopPropagation(); fire('export_stage', { stage_id: stage.id }); }}>
                  <Upload size={12} />
                </button>
              {/if}
              <button type="button" use:tooltip={'Impostazioni gruppo'}
                      onclick={(e) => { e.stopPropagation(); fire('edit_stage', { stage_id: stage.id }); }}>
                <Settings2 size={12} />
              </button>
              <button type="button" class="pe-row-act-danger" use:tooltip={'Rimuovi gruppo'}
                      onclick={(e) => { e.stopPropagation(); fire('remove_stage', { stage_id: stage.id }); }}>
                <Minus size={12} />
              </button>
            </div>
          </header>

          {#if stageSteps.length === 0}
            <p class="pe-stage-empty">Nessuno step. Clic su una voce della palette per aggiungerlo qui.</p>
          {:else}
            <ul class="pe-step-list">
              {#each stageSteps as step (step.id)}
                {@render stepRow(step, stage.id)}
              {/each}
            </ul>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</section>
