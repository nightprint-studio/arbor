<script lang="ts">
  /**
   * BennuEncodingConvertModal — rewrite files from one encoding to another.
   *
   * This REWRITES bytes. Changing only how a file is READ is a different, reversible verb:
   * `BennuReinterpretModal` ("Reinterpret as…").
   *
   * The preview is live: every change of an option re-runs the dry run (debounced), so the list on the
   * right is always what the button on the bottom will do. Nothing is written until the button is
   * pressed; every file is backed up first and the whole run is undone in one step.
   *
   * Keyboard-first: Esc cancels (Modal owns it), Ctrl/Cmd+Enter converts, Tab walks the options.
   */
  import { untrack } from 'svelte';
  import { Languages } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import {
    encodingConvertPlan, encodingConvertApply, encodingConvertUndo,
    type ApplyResult, type BomPolicy, type ConvertPlan, type ConvertRequest, type Fallback,
  } from '$lib/ipc/bennu/encoding-convert';
  import EncodingConvertPlanList from './EncodingConvertPlanList.svelte';
  import { BENNU_ENCODINGS, canonicalEncoding, supportsBom } from './encodings';
  import { afterConversionChange, reportUndo } from './conversion-effects';

  let { onClose, targets: initialTargets }: { onClose: () => void; targets?: string[] } = $props();

  const root = $derived(projectStore.project?.root ?? '');
  const baseName = (p: string) => p.replace(/\/+$/, '').split('/').pop() ?? p;

  // What the dialog opened on: the tree row it was asked from, else the open file, else the project.
  const startTargets = untrack(() =>
    initialTargets?.length ? initialTargets : projectStore.activeFilePath ? [projectStore.activeFilePath] : [],
  );

  let scope = $state(untrack(() => (startTargets.length ? 'targets' : 'project')));
  let from = $state('');
  let to = $state('UTF-8');
  let bom = $state<BomPolicy>('keep');
  let fallback = $state<Fallback>('abort');
  let rewriteDeclarations = $state(true);
  let declareProject = $state(true);

  let plan = $state<ConvertPlan | null>(null);
  let planning = $state(false);
  let error = $state<string | null>(null);
  let phase = $state<'edit' | 'running' | 'done'>('edit');
  let result = $state<ApplyResult | null>(null);
  let undone = $state(false);
  let undoing = $state(false);

  const scopeOptions = $derived([
    ...(startTargets.length
      ? [{ value: 'targets', label: startTargets.length === 1 ? baseName(startTargets[0]) : `${startTargets.length} selected items` }]
      : []),
    ...(projectStore.project?.modules ?? []).map((m) => ({ value: `module:${m}`, label: `Module ${m}` })),
    { value: 'project', label: 'Entire project' },
  ]);

  const scopeTargets = $derived(
    scope === 'targets' ? startTargets
      : scope === 'project' ? [root]
        : [`${root}/${scope.slice('module:'.length)}`],
  );

  const request = $derived<ConvertRequest>({
    root,
    targets: scopeTargets,
    from: from || undefined,
    to,
    bom,
    fallback,
    rewrite_declarations: rewriteDeclarations,
  });

  const toOptions = BENNU_ENCODINGS.map((e) => ({ value: e.value, label: e.label }));
  const fromOptions = [{ value: '', label: 'As each file is read now' }, ...toOptions];

  const bomOptions = [
    { value: 'keep', label: 'Keep as is', description: 'Written only if the file had one.' },
    { value: 'add', label: 'Add a BOM' },
    { value: 'remove', label: 'Remove the BOM' },
  ];
  const fallbackOptions = [
    { value: 'abort', label: 'Stop', description: 'Nothing is written if any character does not fit.' },
    { value: 'replace', label: 'Replace with “?”' },
    { value: 'escape', label: 'Write as an escape', description: '\\uXXXX in Java and properties, &#xXXXX; in XML and JSP.' },
  ];

  const canDeclare = $derived(!!plan?.project.can_declare && !!plan.project.whole_project);
  const blockedStop = $derived(fallback === 'abort' && (plan?.blocked ?? 0) > 0);
  const canRun = $derived(!!plan && !planning && plan.to_convert > 0 && !blockedStop && phase === 'edit');
  const detectedHint = $derived.by(() => {
    const first = plan?.files.find((f) => f.status === 'invalid');
    return first && first.detected ? canonicalEncoding(first.detected) ?? first.detected : null;
  });

  // ── the live preview ──────────────────────────────────────────────────────────────────────
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  /** Unsaved edits would be missing from what the preview reads, and lost to the rewrite. */
  async function ensureSaved() {
    if (projectStore.dirtyPaths.length) await projectStore.saveAllDirty();
  }

  async function runPlan(req: ConvertRequest) {
    const mine = ++seq;
    planning = true;
    error = null;
    try {
      await ensureSaved();
      const next = await encodingConvertPlan(req);
      if (mine === seq) plan = next;
    } catch (e) {
      if (mine === seq) { plan = null; error = String(e); }
    } finally {
      if (mine === seq) planning = false;
    }
  }

  $effect(() => {
    const req = request;
    if (!req.root || phase !== 'edit') return;
    clearTimeout(timer);
    timer = setTimeout(() => void runPlan(req), 250);
    return () => clearTimeout(timer);
  });

  // BOM only means something for the encodings that have one.
  $effect(() => {
    if (!supportsBom(to) && bom !== 'keep') bom = 'keep';
  });

  // ── run / undo ────────────────────────────────────────────────────────────────────────────
  const afterChange = (files: string[]) => afterConversionChange(files, root);

  async function run() {
    if (!canRun) return;
    phase = 'running';
    error = null;
    try {
      await ensureSaved();
      const res = await encodingConvertApply(request, declareProject && canDeclare);
      result = res;
      await afterChange(res.converted_files);
      phase = 'done';
      if (res.cancelled) toastStore.show(`Encoding conversion cancelled after ${res.converted} files — undo restores them`, 'warning');
    } catch (e) {
      error = String(e);
      phase = 'edit';
    }
  }

  async function undo() {
    if (!result || undoing) return;
    undoing = true;
    try {
      const res = await encodingConvertUndo(root, result.run_id);
      await afterChange(res.restored);
      undone = true;
      reportUndo(res);
    } catch (e) {
      toastStore.show(`Undo failed: ${e}`, 'error');
    } finally {
      undoing = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      if (phase === 'done') onClose();
      else void run();
    }
  }
</script>

<Modal {onClose} width="900px" height="620px" padBody={false} ariaLabel="Convert file encoding">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Languages size={14} />
      <span class="modal-title">Convert encoding</span>
    </ModalHeader>
  {/snippet}

  <div class="body" onkeydown={onKey} role="presentation">
    {#if !root}
      <EmptyState message="Open a project to convert its files." />
    {:else if phase === 'done' && result}
      <div class="done">
        <Alert
          variant={result.failed.length || result.cancelled ? 'warning' : 'success'}
          title={undone ? 'Conversion undone' : `Converted ${result.converted} file${result.converted === 1 ? '' : 's'} to ${to}`}
        >
          {#if undone}
            Every file is back as it was, and so is how the project reads them.
          {:else}
            {result.unchanged} already right, {result.skipped} skipped.
            {#if result.poms_declared}
              {result.poms_declared} pom{result.poms_declared === 1 ? '' : 's'} now declare{result.poms_declared === 1 ? 's' : ''}
              <code>project.build.sourceEncoding</code> = {to}.
            {/if}
            {#if result.declare_note}{result.declare_note}{/if}
          {/if}
        </Alert>
        {#if !undone}
          <p class="muted">
            Each file was backed up first, and also recorded in Local History as one change. <strong>Undo</strong> puts back every
            file that has not been edited since.
          </p>
          <p class="muted path">Backup: <code>{result.backup_dir}</code></p>
        {/if}
        {#if result.failed.length}
          <Alert variant="error" title={`${result.failed.length} file${result.failed.length === 1 ? '' : 's'} could not be written`}>
            {#each result.failed.slice(0, 8) as f (f.file)}
              <div><code>{baseName(f.file)}</code> — {f.error}</div>
            {/each}
          </Alert>
        {/if}
      </div>
    {:else}
      <div class="cols">
        <div class="opts">
          <FormField label="Convert">
            <Select bind:value={scope} options={scopeOptions} fill ariaLabel="What to convert" />
          </FormField>
          <FormField label="From" hint="Leave on “as each file is read” unless the preview says files are not valid.">
            <Select bind:value={from} options={fromOptions} fill searchable ariaLabel="Source encoding" />
          </FormField>
          <FormField label="To">
            <Select bind:value={to} options={toOptions} fill searchable ariaLabel="Target encoding" />
          </FormField>
          {#if supportsBom(to)}
            <FormField label="Byte-order mark">
              <RadioGroup bind:value={bom} options={bomOptions} appearance="radio" direction="vertical" size="sm" />
            </FormField>
          {/if}
          <FormField label="A character that does not fit">
            <RadioGroup bind:value={fallback} options={fallbackOptions} appearance="radio" direction="vertical" size="sm" />
          </FormField>
          <Toggle bind:checked={rewriteDeclarations} size="sm" label="Update the encoding files declare about themselves"
            description="XML prolog, JSP page directive." />
          {#if canDeclare}
            <Toggle bind:checked={declareProject} size="sm" label="Declare it in the pom"
              description={`Set project.build.sourceEncoding to ${to} (now: ${plan?.project.declared}).`} />
          {/if}
        </div>

        <div class="preview">
          {#if error}
            <div class="pad"><Alert variant="error" title="Could not convert" text={error} /></div>
          {:else if !plan}
            <div class="center"><Spinner /> <span>Reading files…</span></div>
          {:else}
            <div class="summary">
              {#if planning}<Spinner size="sm" />{/if}
              <span><strong>{plan.to_convert}</strong> to convert</span>
              <span class="dim">· {plan.unchanged} already right</span>
              {#if plan.skipped}<span class="dim">· {plan.skipped} skipped</span>{/if}
              {#if plan.blocked}<span class="bad">· {plan.blocked} blocked</span>{/if}
            </div>
            {#if blockedStop}
              <div class="pad">
                <Alert variant="warning" compact
                  text={`${plan.blocked} file${plan.blocked === 1 ? '' : 's'} contain characters ${to} cannot hold. Pick another encoding, or replace or escape them.`} />
              </div>
            {/if}
            {#if detectedHint}
              <div class="pad">
                <Alert variant="info" compact
                  text={`Some files are not valid in their current encoding — they look like ${detectedHint}. Set “From” to ${detectedHint}.`} />
              </div>
            {/if}
            <div class="list">
              {#if plan.files.length}
                <EncodingConvertPlanList {plan} {fallback} />
              {:else}
                <EmptyState message="Nothing to convert — every file is already in this encoding." />
              {/if}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end" hint={phase === 'done' ? undefined : 'to convert'}>
      {#if phase === 'done'}
        {#if !undone}
          <Button variant="secondary" onclick={undo} loading={undoing}>Undo conversion</Button>
        {/if}
        <Button variant="primary" onclick={onClose}>Close</Button>
      {:else}
        <Button variant="ghost" onclick={onClose}>Cancel</Button>
        <Button variant="primary" onclick={run} disabled={!canRun} loading={phase === 'running'}>
          {plan && plan.to_convert ? `Convert ${plan.to_convert} file${plan.to_convert === 1 ? '' : 's'}` : 'Convert'}
        </Button>
      {/if}
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .body {
    height: 100%;
    min-height: 0;
  }
  .cols {
    display: grid;
    grid-template-columns: 290px 1fr;
    height: 100%;
    min-height: 0;
  }
  .opts {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    overflow-y: auto;
    border-right: 1px solid var(--border-subtle);
  }
  .preview {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    font-size: var(--font-size-sm);
    border-bottom: 1px solid var(--border-subtle);
  }
  .dim {
    color: var(--text-tertiary);
  }
  .bad {
    color: var(--error);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .pad {
    padding: 8px 12px 0;
  }
  .center {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    flex: 1;
    color: var(--text-secondary);
  }
  .done {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
  }
  .muted {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
  }
  .path code {
    font-family: var(--font-code);
    word-break: break-all;
  }
</style>
