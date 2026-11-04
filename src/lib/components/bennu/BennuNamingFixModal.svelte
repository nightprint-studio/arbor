<script lang="ts">
  /**
   * Review a bulk naming fix before it happens.
   *
   * The plan is already computed — this is the moment to see which rule produced it, how far it
   * reaches, disagree with part of it, and see what it will not touch. It is deliberately **not** a
   * diff view: a bulk rename produces thousands of one-word edits, and a thousand one-line diffs is
   * not something anyone reads. What matters is the shape — which names, grouped how you want them,
   * with the ones you do not want unticked or retyped — and a single Undo if the answer turns out to
   * be wrong.
   *
   * Refusals are the other reason this screen exists. A bulk fix that quietly skips things is worse
   * than one that fixes less: the count would say "done" while a dozen names stayed as they were, and
   * nobody would go looking. They have a tab of their own, grouped by reason.
   *
   * This file holds the frame, the outcome and the keyboard; `BennuNamingFixConvention` states and
   * changes the rule, `BennuNamingFixReview` renders the names, `BennuNamingFixRefused` the refusals,
   * and `naming-fix-selection` works out what the choices add up to. The ticks live in the store, so
   * the review can step aside (to look at a declaration) and come back as it was.
   */
  import { CaseSensitive } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { bennuNamingStore } from '$lib/stores/bennu/naming.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import type { ConventionSource } from '$lib/ipc/bennu/naming';
  import BennuNamingFixConvention from './BennuNamingFixConvention.svelte';
  import BennuNamingFixRefused from './BennuNamingFixRefused.svelte';
  import BennuNamingFixReview from './BennuNamingFixReview.svelte';
  import {
    selectedEdits,
    selectedFileMoves,
    selectionCounts,
    type FixFilter,
  } from './naming-fix-selection';

  let { onClose }: { onClose: () => void } = $props();

  /** Applying more names than this asks first: the footer says the count, the dialog makes you read it. */
  const CONFIRM_ABOVE = 50;

  const plan = $derived(bennuNamingStore.pendingFix);
  const scope = $derived(bennuNamingStore.fixScope);
  const planning = $derived(bennuNamingStore.planningFix);
  const progress = $derived(bennuNamingStore.fixProgress);
  const filter = $derived(bennuNamingStore.fixFilter);
  const hidden = $derived(bennuNamingStore.fixHidden);
  const root = $derived(projectStore.project?.root ?? null);

  let applying = $state(false);
  let confirming = $state(false);
  let tab = $state<'rename' | 'left'>('rename');

  /** What Apply would actually do, under the current filter — the footer counts this, and the
   *  button applies exactly it. */
  const chosen = $derived(selectionCounts(plan?.renamed ?? [], filter));
  const fileCount = $derived(plan?.files.length ?? 0);
  const refusedCount = $derived(plan?.refused.length ?? 0);
  const canApply = $derived(!!plan && chosen.names > 0 && !applying);

  /** The phase label, worded for someone watching rather than for the protocol. */
  const phaseLabel = $derived(
    progress?.phase === 'planning types'
      ? 'Planning type renames — scanning sources'
      : progress?.phase === 'detecting'
        ? 'Reading how this project names things'
        : 'Reading project files',
  );

  const tabs = $derived([
    { id: 'rename', label: 'Rename', badge: plan?.renamed.length ?? 0 },
    { id: 'left', label: 'Left alone', badge: refusedCount, disabled: refusedCount === 0 },
  ]);

  function setFilter(next: FixFilter) {
    bennuNamingStore.setFixFilter(next);
  }

  /** Plan again under another convention — the rules decide which names break. */
  function choose(source: ConventionSource) {
    if (source === plan?.convention.source) return;
    void bennuNamingStore.replanFix(source);
  }

  /** Open a declaration in the editor. The review steps aside — ticks, filter and typed names are kept —
   *  and a toast brings it back. */
  function goTo(file: string, line: number) {
    bennuNamingStore.hideFix();
    void projectStore.openFile(file).then(() => bennuUiStore.requestGoto(line));
    toastStore.show('Naming review is waiting', 'info', 60_000, {
      label: 'Back to review',
      onClick: () => bennuNamingStore.showFix(),
    });
  }

  async function apply() {
    if (!plan || !canApply) return;
    confirming = false;
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

  /** Apply, asking first when it is a lot of names. */
  function requestApply() {
    if (!canApply) return;
    if (chosen.names > CONFIRM_ABOVE) confirming = true;
    else void apply();
  }

  function cancel() {
    bennuNamingStore.dismissFix();
    onClose();
  }

  /** Ask the backend to stop. The modal stays open — what it planned so far still arrives. */
  function stopPlanning() {
    if (root) bennuNamingStore.cancelFix(root);
  }

  /** Ctrl/Cmd+Enter applies — the shortcut the footer button advertises. Esc is Modal's. */
  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      requestApply();
    }
  }
</script>

{#if !hidden}
  <!-- A working screen: a rule, two tabs and a list that can run to thousands of rows. Sized to the
       window, since the shared Modal has no resize handle. -->
  <Modal onClose={cancel} width="min(1040px, 94vw)" height="min(780px, 90vh)" ariaLabel="Fix naming conventions">
    {#snippet header()}
      <ModalHeader onClose={cancel}>
        <CaseSensitive size={14} />
        <span class="modal-title">Fix naming {scope === 'project' ? 'in project' : 'in file'}</span>
        {#if plan && !planning}
          <span class="head-badges">
            <Badge size="sm" tone="accent" variant="tone">
              {plan.renamed.length} name{plan.renamed.length === 1 ? '' : 's'} · {fileCount} file{fileCount === 1 ? '' : 's'}
            </Badge>
            {#if refusedCount > 0}
              <Badge size="sm" tone="warning" variant="tone">{refusedCount} left alone</Badge>
            {/if}
            {#if plan.renamed.length > chosen.names}
              <Badge size="sm" tone="neutral" variant="tone">{plan.renamed.length - chosen.names} unticked</Badge>
            {/if}
          </span>
        {/if}
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
            <!-- Every phase counts FILES. Saying so is the difference between a number that tells you
                 it is moving and one you have to guess the unit of. -->
            <span class="working-count">{progress.done} / {progress.total} files</span>
          {:else}
            <ProgressBar indeterminate ariaLabel={phaseLabel} />
          {/if}
        </div>
      {:else if !plan || (plan.renamed.length === 0 && plan.refused.length === 0)}
        {#if plan}
          <BennuNamingFixConvention report={plan.convention} onChoose={choose} busy={planning} />
        {/if}
        <EmptyState message="No naming issues to fix." />
      {:else}
        <BennuNamingFixConvention report={plan.convention} onChoose={choose} busy={planning} />

        {#if plan.cancelled}
          <Alert variant="warning" compact>
            Stopped early — this is what had been planned so far, not everything.
          </Alert>
        {/if}
        {#if plan.capped}
          <Alert variant="warning" compact>
            The scan stopped at its file limit, so this is not every file in the project. Run it again after applying to catch the rest.
          </Alert>
        {/if}
        {#if scope === 'file' && fileCount > 1}
          <!-- Said plainly: asking to fix "this file" and editing six is a surprise, even though it is
               the correct behaviour — a method's callers live elsewhere. -->
          <Alert variant="info" compact>Renaming a member edits whoever uses it, so this reaches beyond the open file.</Alert>
        {/if}

        <Tabs items={tabs} value={tab} variant="underline" size="sm" onSelect={(id) => (tab = id as 'rename' | 'left')} />

        {#if tab === 'rename'}
          {#if plan.renamed.length === 0}
            <EmptyState message="Every name that breaks the convention was left alone — see the other tab." />
          {:else}
            <BennuNamingFixReview renamed={plan.renamed} {root} {filter} onFilter={setFilter} onGoTo={goTo} />
          {/if}
        {:else}
          <BennuNamingFixRefused refused={plan.refused} onGoTo={goTo} />
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
          <span class="hints">
            <Kbd keys={['Space']} size="sm" /> tick
            <Kbd keys={['F2']} size="sm" /> rename
            <Kbd keys={['F']} size="sm" /> filter
            <Kbd keys={['A']} size="sm" />/<Kbd keys={['N']} size="sm" />/<Kbd keys={['I']} size="sm" /> all · none · invert
          </span>
          <!-- What Apply will do, in the words of the current filter — so unticking something is
               visibly the same act as reducing this number. -->
          <span class="tally">
            {chosen.names} of {plan?.renamed.length ?? 0} name{(plan?.renamed.length ?? 0) === 1 ? '' : 's'}
            · {chosen.files} file{chosen.files === 1 ? '' : 's'} · {chosen.edits} edit{chosen.edits === 1 ? '' : 's'}
          </span>
          <Button variant="secondary" size="sm" onclick={cancel}>Cancel</Button>
          <Button
            variant="primary"
            size="sm"
            onclick={requestApply}
            disabled={!canApply}
            tooltip={{ content: 'Apply — one Undo takes it all back', shortcut: 'Ctrl+Enter' }}
          >
            {applying ? 'Applying…' : `Apply ${chosen.names} name${chosen.names === 1 ? '' : 's'}`}
          </Button>
        {/if}
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

{#if confirming}
  <ConfirmModal
    title="Apply this fix?"
    message={`${chosen.names} names will be renamed in ${chosen.files} files.`}
    detail="Declarations and every use are rewritten together, and one Undo in an open file takes the whole thing back."
    confirmLabel={`Rename ${chosen.names}`}
    onConfirm={() => void apply()}
    onCancel={() => (confirming = false)}
  />
{/if}

<style>
  /* `hidden`, not `auto`: the review windows its own list and needs a real height to do it. A
     scrolling body would leave it content-sized, which for a windowed list means zero. */
  .body { display: flex; flex-direction: column; gap: 10px; padding: 12px; overflow: hidden; min-height: 0; }

  .head-badges { display: inline-flex; gap: 6px; margin-left: 10px; }

  .working { display: flex; flex-direction: column; gap: 8px; margin: auto 0; }
  .working-head { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-secondary); }
  .working-count { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }

  .hints { display: inline-flex; align-items: center; gap: 5px; margin-right: auto; font-size: 11px; color: var(--text-muted); }
  .tally { margin-right: 10px; font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
</style>
