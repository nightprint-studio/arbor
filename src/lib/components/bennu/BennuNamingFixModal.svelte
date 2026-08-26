<script lang="ts">
  /**
   * Review a bulk naming fix before it happens.
   *
   * The plan is already computed — this is the moment to see how far it reaches, disagree with
   * part of it, and see what it will not touch. It is deliberately **not** a diff view: a bulk
   * rename produces thousands of one-word edits, and a thousand one-line diffs is not something
   * anyone reads. What matters is the shape — which names, grouped how you want them, with the
   * ones you do not want unticked — and a single Undo if the answer turns out to be wrong.
   *
   * Refusals are the other reason this screen exists. A bulk fix that quietly skips things is
   * worse than one that fixes less: the count would say "done" while a dozen names stayed as they
   * were, and nobody would go looking.
   *
   * This file holds the state and the outcome; `BennuNamingFixReview` renders the list, and
   * `naming-fix-selection` works out what the filters add up to.
   */
  import { CaseSensitive, TriangleAlert } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { bennuNamingStore } from '$lib/stores/bennu/naming.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import BennuNamingFixReview from './BennuNamingFixReview.svelte';
  import {
    noFilter,
    selectedEdits,
    selectedFileMoves,
    selectionCounts,
    type FixFilter,
  } from './naming-fix-selection';

  let { onClose }: { onClose: () => void } = $props();

  const plan = $derived(bennuNamingStore.pendingFix);
  const scope = $derived(bennuNamingStore.fixScope);
  const planning = $derived(bennuNamingStore.planningFix);
  const progress = $derived(bennuNamingStore.fixProgress);
  let applying = $state(false);

  /** The review's narrowing choices. Opens grouped by file, nothing excluded. */
  let filter = $state<FixFilter>(noFilter('file'));

  /** What Apply would actually do, under the current filter — the footer counts this, and the
   *  button applies exactly it. */
  const chosen = $derived(selectionCounts(plan?.renamed ?? [], filter));

  /** The phase label, worded for someone watching rather than for the protocol. */
  const phaseLabel = $derived(
    progress?.phase === 'planning types'
      ? 'Planning type renames — scanning sources'
      : 'Reading project files',
  );

  const fileCount = $derived(plan?.files.length ?? 0);
  const canApply = $derived(!!plan && chosen.names > 0 && !applying);

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  async function apply() {
    if (!plan || !canApply) return;
    applying = true;
    try {
      // The same path a single rename takes, so the whole bulk fix is one Undo in an open buffer
      // and a normal write everywhere else. Only the names still selected — the review is
      // permitted to disagree with the plan, and Apply has to mean what the footer says.
      const failed = await projectStore.applyEdits(selectedEdits(plan.renamed, filter));

      // The moves come AFTER the edits, which are addressed to the old paths. A type whose file is
      // named after it has to take the file with it or the code stops compiling.
      let movedCount = 0;
      let moveFailed = 0;
      for (const move of selectedFileMoves(plan.renamed, filter)) {
        const base = move.to.split('/').pop() ?? move.to;
        try {
          await projectStore.renameFile(move.from, base);
          movedCount += 1;
        } catch {
          moveFailed += 1;
        }
      }

      const moved = movedCount ? ` · moved ${movedCount} file${movedCount === 1 ? '' : 's'}` : '';
      if (failed || moveFailed) {
        const parts = [
          failed ? `${failed} file(s) could not be written` : '',
          moveFailed ? `${moveFailed} file(s) could not be moved` : '',
        ].filter(Boolean);
        toastStore.show(`Renamed, but ${parts.join(' and ')}`, 'error');
      } else {
        toastStore.show(
          `Renamed ${chosen.names} name${chosen.names === 1 ? '' : 's'} in ${chosen.files} file${chosen.files === 1 ? '' : 's'}${moved}`,
          'success',
        );
      }
      bennuNamingStore.dismissFix();
      onClose();
    } catch {
      toastStore.show('The fix could not be applied', 'error');
    } finally {
      applying = false;
    }
  }

  function cancel() {
    bennuNamingStore.dismissFix();
    onClose();
  }

  /** Ask the backend to stop. The modal stays open — what it planned so far still arrives. */
  function stopPlanning() {
    const root = projectStore.project?.root;
    if (root) bennuNamingStore.cancelFix(root);
  }

  /** Ctrl/Cmd+Enter applies — the shortcut the footer button advertises. Esc is Modal's. */
  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void apply();
    }
  }
</script>

<!-- Bigger than a confirmation: this is a working screen with a filter bar and a list that can run
     to thousands of rows. -->
<Modal onClose={cancel} width="760px" height="640px" ariaLabel="Fix naming conventions">
  {#snippet header()}
    <ModalHeader onClose={cancel}>
      <CaseSensitive size={14} />
      <span class="modal-title">
        Fix naming {scope === 'project' ? 'in project' : 'in file'}
      </span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKeydown}>
    {#if planning}
      <!-- The modal opens HERE, before the work, not after it: on a real project this phase runs
           for a while, and a command that shows nothing until it finishes is one you assume has
           hung. -->
      <div class="working">
        <div class="working-head">
          <Spinner size={14} />
          <span>{phaseLabel}</span>
        </div>
        {#if progress && progress.total > 0}
          <ProgressBar value={progress.done} max={progress.total} ariaLabel={phaseLabel} />
          <!-- Both phases count FILES. Saying so is the difference between a number that tells you
               it is moving and one you have to guess the unit of. -->
          <span class="working-count">{progress.done} / {progress.total} files</span>
        {:else}
          <ProgressBar indeterminate ariaLabel={phaseLabel} />
        {/if}
      </div>
    {:else if !plan || (plan.renamed.length === 0 && plan.refused.length === 0)}
      <EmptyState message="No naming issues to fix." />
    {:else}
      {#if plan.cancelled}
        <Alert variant="warning">
          Stopped early — this is what had been planned so far, not everything.
        </Alert>
      {/if}
      {#if plan.renamed.length === 0}
        <EmptyState message="Every name that breaks the convention was refused — see below." />
      {:else}
        <p class="lead">
          <strong>{plan.renamed.length}</strong>
          name{plan.renamed.length === 1 ? '' : 's'} in
          <strong>{fileCount}</strong>
          file{fileCount === 1 ? '' : 's'}.
          {#if scope === 'file' && fileCount > 1}
            <!-- Said plainly: asking to fix "this file" and editing six is a surprise, even though
                 it is the correct behaviour — a method's callers live elsewhere. -->
            Renaming a member edits whoever uses it, so this reaches beyond the open file.
          {/if}
        </p>
      {/if}

      {#if plan.capped}
        <Alert variant="warning">
          The scan stopped at its file limit, so this is not every file in the project. Run it again
          after applying to catch the rest.
        </Alert>
      {/if}

      {#if plan.renamed.length > 0}
        <!-- Rows are identified by their INDEX in the plan, never by file+name: one file
             legitimately holds several declarations with the same name — a local called
             `source_directory` in five different methods is five distinct renames. -->
        <BennuNamingFixReview renamed={plan.renamed} bind:filter />
      {/if}

      {#if plan.refused.length > 0}
        <div class="refused">
          <div class="refused-head">
            <TriangleAlert size={13} />
            <span>{plan.refused.length} left alone</span>
          </div>
          <ul class="rows">
            {#each plan.refused as r, i (i)}
              <li class="row">
                <span class="from">{r.name}</span>
                <span class="reason">{r.reason}</span>
                <span class="file" title={r.file}>{baseName(r.file)}:{r.line}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      {#if planning}
        <!-- Stopping is a real answer while this runs, and the backend still returns what it had —
             so the partial plan is reviewable rather than thrown away. -->
        <Button variant="secondary" size="sm" onclick={stopPlanning}>Stop</Button>
      {:else}
        <!-- What Apply will do, in the words of the current filter — so unticking something is
             visibly the same act as reducing this number. -->
        <span class="tally">
          {chosen.names} of {plan?.renamed.length ?? 0} name{(plan?.renamed.length ?? 0) === 1 ? '' : 's'}
          · {chosen.files} file{chosen.files === 1 ? '' : 's'}
        </span>
        <Button variant="secondary" size="sm" onclick={cancel}>Cancel</Button>
        <Button
          variant="primary"
          size="sm"
          onclick={() => void apply()}
          disabled={!canApply}
          tooltip={{ content: 'Apply', shortcut: 'Ctrl+Enter' }}
        >
          {applying ? 'Applying…' : 'Apply'}
        </Button>
      {/if}
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  /* `hidden`, not `auto`: the review windows its own list and needs a real height to do it. A
     scrolling body would leave it content-sized, which for a windowed list means zero. */
  .body { display: flex; flex-direction: column; gap: 10px; padding: 12px; overflow: hidden; min-height: 0; }

  .working { display: flex; flex-direction: column; gap: 8px; margin: auto 0; }
  .working-head { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-secondary); }
  .working-count { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .lead { margin: 0; font-size: 12px; color: var(--text-secondary); }

  .rows { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .row {
    display: flex; align-items: baseline; gap: 8px;
    padding: 3px 6px;
    font-size: 12px;
    border-radius: var(--radius-sm);
  }
  .row:nth-child(odd) { background: var(--bg-elevated); }
  .from { font-family: var(--font-mono); color: var(--text-secondary); text-decoration: line-through; }
  .reason { flex: 1; color: var(--text-muted); font-size: 11px; }
  .file { margin-left: auto; font-size: 10px; color: var(--text-muted); }

  .tally { margin-right: auto; font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }

  /* Refusals are secondary to the list above, and capped so a long tail of them cannot push the
     thing being reviewed off the screen. */
  .refused { display: flex; flex-direction: column; gap: 4px; flex: none; max-height: 30%; overflow-y: auto; }
  .refused-head {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; font-weight: 600; color: var(--warning);
  }
</style>
