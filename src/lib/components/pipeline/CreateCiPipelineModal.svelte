<script lang="ts">
  import { Plus, Trash2, Play, AlertCircle, GitBranch, ChevronDown } from 'lucide-svelte';
  import BrandIcon from '$lib/components/shared/ui/BrandIcon.svelte';
  import { listLocalBranches } from '$lib/ipc/branch';
  import { listCiWorkflows, createCiPipeline } from '$lib/ipc/pipeline';
  import type { CiProviderInfo, CiWorkflow } from '$lib/types/pipeline';
  import type { BranchInfo } from '$lib/types/git';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    provider:    CiProviderInfo;
    tabId:       string;
    onClose:     () => void;
    onCreated:   () => void;
  }

  let { provider, tabId, onClose, onCreated }: Props = $props();

  // ── State ──────────────────────────────────────────────────────────────────
  let branches      = $state<BranchInfo[]>([]);
  let workflows     = $state<CiWorkflow[]>([]);
  let loadingMeta   = $state(true);
  let loadError     = $state<string | null>(null);

  let selectedBranch   = $state('');
  let selectedWorkflow = $state('');
  let variables        = $state<{ key: string; value: string }[]>([]);

  let submitting  = $state(false);
  let submitError = $state<string | null>(null);

  // After a successful GitHub dispatch we show a notice (no immediate run ID).
  let githubDispatched = $state(false);

  // ── Load branches + workflows on mount ────────────────────────────────────
  $effect(() => {
    loadingMeta = true;
    loadError   = null;

    const branchPromise   = listLocalBranches(tabId).catch(() => [] as BranchInfo[]);
    const workflowPromise = provider.provider === 'github'
      ? listCiWorkflows(tabId).catch(() => [] as CiWorkflow[])
      : Promise.resolve([] as CiWorkflow[]);

    Promise.all([branchPromise, workflowPromise]).then(([bs, wfs]) => {
      branches  = bs;
      workflows = wfs;
      // Default branch = HEAD
      const head = bs.find(b => b.is_head);
      if (head) selectedBranch = head.name;
      else if (bs.length > 0) selectedBranch = bs[0].name;
      // Default workflow = first active one
      if (wfs.length > 0) selectedWorkflow = wfs[0].id;
      loadingMeta = false;
    }).catch(e => {
      loadError   = String(e);
      loadingMeta = false;
    });
  });

  // ── Variable helpers ───────────────────────────────────────────────────────
  function addVariable() {
    variables = [...variables, { key: '', value: '' }];
  }

  function removeVariable(i: number) {
    variables = variables.filter((_, idx) => idx !== i);
  }

  function updateVariable(i: number, field: 'key' | 'value', v: string) {
    variables = variables.map((row, idx) => idx === i ? { ...row, [field]: v } : row);
  }

  // ── Submit ────────────────────────────────────────────────────────────────
  async function handleSubmit() {
    if (!selectedBranch) return;
    if (provider.provider === 'github' && !selectedWorkflow) return;

    submitting  = true;
    submitError = null;

    const vars: [string, string][] = variables
      .filter(v => v.key.trim() !== '')
      .map(v => [v.key.trim(), v.value]);

    try {
      await createCiPipeline(
        tabId,
        selectedBranch,
        vars,
        provider.provider === 'github' ? selectedWorkflow : undefined,
      );

      if (provider.provider === 'github') {
        // GitHub dispatch is async — the run appears after a few seconds.
        githubDispatched = true;
        // Notify parent to refresh after a small delay to give GitHub time.
        setTimeout(() => { onCreated(); }, 3000);
      } else {
        onCreated();
      }
    } catch (e) {
      submitError = String(e);
      submitting  = false;
    }
  }

  // ── Derived ───────────────────────────────────────────────────────────────
  const isGitHub = $derived(provider.provider === 'github');
  const canSubmit = $derived(
    !submitting &&
    selectedBranch !== '' &&
    (!isGitHub || selectedWorkflow !== '')
  );

  const providerLabel = $derived(isGitHub ? 'GitHub Actions' : 'GitLab CI');

  const branchItems = $derived<DropdownItem[]>(
    branches.map(b => ({
      kind:    'item',
      id:      b.name,
      label:   b.is_head ? `${b.name} (current)` : b.name,
      active:  selectedBranch === b.name,
      onclick: () => { selectedBranch = b.name; },
    })),
  );
  const branchLabel = $derived(
    selectedBranch
      ? (branches.find(b => b.name === selectedBranch)?.is_head ? `${selectedBranch} (current)` : selectedBranch)
      : '— pick a branch —',
  );

  const workflowItems = $derived<DropdownItem[]>(
    workflows.map(wf => ({
      kind:     'item',
      id:       wf.id,
      label:    wf.name,
      subtitle: wf.path,
      active:   selectedWorkflow === wf.id,
      onclick:  () => { selectedWorkflow = wf.id; },
    })),
  );
  const workflowLabel = $derived(
    workflows.find(wf => wf.id === selectedWorkflow)?.name ?? '— pick a workflow —',
  );
</script>

<Modal {onClose} size="md" ariaLabel="Create pipeline run">
  {#snippet header()}
    <ModalHeader {onClose}>
      <!-- Provider glyph: GitHub follows currentColor, GitLab uses its absolute
           brand orange via `.provider-icon-gitlab`. <BrandIcon> inherits the
           wrapper's `color`, so the rule still applies. -->
      <span class="provider-icon" class:provider-icon-gitlab={!isGitHub}>
        <BrandIcon brand={provider.provider} size={16} />
      </span>
      <span class="modal-title">New {providerLabel} Run</span>
      {#if provider.owner && provider.repo_name}
        <span class="modal-repo">{provider.owner}/{provider.repo_name}</span>
      {:else if provider.project_path}
        <span class="modal-repo">{provider.project_path}</span>
      {/if}
    </ModalHeader>
  {/snippet}

  {#if loadingMeta}
    <div class="center-state">
      <Spinner size="md" label="Loading branches…" />
    </div>

  {:else if loadError}
    <div class="center-state error-state">
      <AlertCircle size={20} />
      <span>{loadError}</span>
    </div>

  {:else if githubDispatched}
    <!-- Success notice for GitHub (async dispatch) -->
    <div class="center-state success-state">
      <Play size={28} />
      <p class="success-title">Workflow dispatched!</p>
      <p class="success-hint">
        GitHub is queuing the run. The list will refresh automatically in a moment.
      </p>
    </div>

  {:else}
    <div class="form">

      <!-- Branch -->
      <FormField label="Branch">
        {#snippet icon()}<GitBranch size={12} />{/snippet}
        <div class="select-wrap">
          <Dropdown
            position="fixed"
            direction="down"
            matchTriggerWidth
            searchable={branches.length > 12}
            searchPlaceholder="Filter branches…"
            items={branchItems}
          >
            {#snippet trigger({ open, toggle })}
              <button
                class="field-select"
                onclick={toggle}
                type="button"
                aria-haspopup="listbox"
                aria-expanded={open}
              >
                <span class="field-select-label">{branchLabel}</span>
                <ChevronDown size={12} />
              </button>
            {/snippet}
          </Dropdown>
        </div>
      </FormField>

      <!-- Workflow picker (GitHub only) -->
      {#if isGitHub}
        <FormField label="Workflow">
          {#if workflows.length === 0}
            <p class="field-hint warn-hint">
              No active <code>workflow_dispatch</code> workflows found in this repository.
              Add <code>on: workflow_dispatch</code> to a workflow file to enable manual runs.
            </p>
          {:else}
            <div class="select-wrap">
              <Dropdown
                position="fixed"
                direction="down"
                matchTriggerWidth
                searchable={workflows.length > 12}
                searchPlaceholder="Filter workflows…"
                items={workflowItems}
              >
                {#snippet trigger({ open, toggle })}
                  <button
                    class="field-select"
                    onclick={toggle}
                    type="button"
                    aria-haspopup="listbox"
                    aria-expanded={open}
                  >
                    <span class="field-select-label">{workflowLabel}</span>
                    <ChevronDown size={12} />
                  </button>
                {/snippet}
              </Dropdown>
            </div>
          {/if}
        </FormField>
      {/if}

      <!-- Variables -->
      <FormField label="Variables">
        {#snippet actions()}
          <button class="add-var-btn" onclick={addVariable} type="button">
            <Plus size={11} /> Add
          </button>
        {/snippet}

        {#if variables.length === 0}
          <p class="field-hint">No variables — click <strong>Add</strong> to inject environment variables into this run.</p>
        {:else}
          <div class="var-table">
            <div class="var-row var-header">
              <span>Key</span>
              <span>Value</span>
              <span></span>
            </div>
            {#each variables as v, i (i)}
              <div class="var-row">
                <input
                  class="var-input"
                  type="text"
                  placeholder="KEY"
                  value={v.key}
                  oninput={(e) => updateVariable(i, 'key', (e.target as HTMLInputElement).value)}
                />
                <input
                  class="var-input"
                  type="text"
                  placeholder="value"
                  value={v.value}
                  oninput={(e) => updateVariable(i, 'value', (e.target as HTMLInputElement).value)}
                />
                <button
                  class="var-remove-btn"
                  onclick={() => removeVariable(i)}
                  type="button"
                  use:tooltip={'Remove'}
                >
                  <Trash2 size={11} />
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </FormField>

      <!-- Submit error -->
      {#if submitError}
        <div class="submit-error">
          <AlertCircle size={13} />
          {submitError}
        </div>
      {/if}

    </div>
  {/if}

  {#snippet footer()}
    {#if !loadingMeta && !githubDispatched}
      <Button variant="secondary" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        disabled={!canSubmit || (isGitHub && workflows.length === 0)}
        onclick={handleSubmit}
        loading={submitting}
      >
        {#snippet iconStart()}<Play size={13} />{/snippet}
        Run Pipeline
      </Button>
    {:else}
      <Button variant="secondary" onclick={onClose}>Close</Button>
    {/if}
  {/snippet}
</Modal>

<style>
  .provider-icon {
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }
  /* GitLab uses its absolute brand orange — themable text colour would
     misrepresent the brand. */
  .provider-icon-gitlab { color: var(--brand-gitlab); }

  .modal-repo {
    font-size: 11px;
    font-family: var(--font-code);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Center states ─────────────────────────────────────────────────────── */
  .center-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 32px 16px;
    color: var(--text-muted);
    font-size: 12px;
    text-align: center;
  }

  .error-state { color: var(--error); }

  .success-state { color: var(--success); }
  .success-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }
  .success-hint { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

  /* ── Form ──────────────────────────────────────────────────────────────── */
  .form {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .field-hint {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.5;
    margin: 0;
  }

  .warn-hint {
    background: rgba(251,191,36,0.08);
    border: 1px solid rgba(251,191,36,0.2);
    border-radius: var(--radius-sm);
    padding: 8px 10px;
    color: var(--text-secondary);
  }
  .warn-hint code {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--warning);
  }

  /* ── Select ────────────────────────────────────────────────────────────── */
  .select-wrap { display: flex; }
  .select-wrap :global(.dd-root) { width: 100%; }

  .field-select {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    box-sizing: border-box;
    padding: 7px 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .field-select:hover,
  .field-select[aria-expanded='true'] { border-color: var(--accent); }
  .field-select-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── Variable table ────────────────────────────────────────────────────── */
  .add-var-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 9px;
    border: 1px solid var(--border);
    background: var(--bg-base);
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: border-color var(--transition-fast), color var(--transition-fast);
  }
  .add-var-btn:hover { border-color: var(--accent); color: var(--accent); }

  .var-table {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .var-header {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--text-muted);
    padding: 0 0 2px 2px;
  }

  .var-row {
    display: grid;
    grid-template-columns: 1fr 1fr 26px;
    gap: 6px;
    align-items: center;
  }

  .var-input {
    padding: 5px 8px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 11px;
    outline: none;
    transition: border-color var(--transition-fast);
    min-width: 0;
  }
  .var-input:focus { border-color: var(--accent); }
  .var-input::placeholder { color: var(--text-disabled); }

  .var-remove-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color var(--transition-fast), background var(--transition-fast);
    flex-shrink: 0;
  }
  .var-remove-btn:hover { color: var(--error); background: var(--error-subtle); }

  /* ── Submit error ──────────────────────────────────────────────────────── */
  .submit-error {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--error-subtle);
    border: 1px solid color-mix(in srgb, var(--error) 25%, transparent);
    color: var(--error);
    font-size: 11px;
    line-height: 1.5;
    word-break: break-word;
  }

</style>
