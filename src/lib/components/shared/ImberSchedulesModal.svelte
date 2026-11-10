<script lang="ts">
  /**
   * Imber › Schedules — every schedule of Imber, visible and manageable: what runs when, whether it is on, the
   * next run and how the last one ended; create, edit, turn on or off, run now, delete. Shared by Corvus and
   * Bennu: the product hands over its store (which carries its adapter), nothing here names a product.
   *
   * Schedules fire only while the app is open; a run missed while it was closed is skipped or caught up, per
   * schedule. A scheduled run has nobody to approve anything, so what it may do is the product's rule.
   *
   * Keyboard: ↑↓ move between rows, `Enter` edits, `Space` turns a schedule on or off, `R` runs it now, `Del`
   * deletes (after a confirmation), `N` creates one. In the editor `Ctrl+Enter` saves and `Esc` goes back.
   */
  import { CalendarClock, MessageSquare, Pencil, Play, Plus, Trash2 } from 'lucide-svelte';
  import { tick } from 'svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ImberScheduleForm from '$lib/components/shared/internal/ImberScheduleForm.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import type { ImberSchedulesStore } from '$lib/stores/imber-schedules.svelte';
  import type { ImberJob } from '$lib/types/imber-schedule';
  import {
    blankForm, describeRecurrence, formFromJob, inputFromForm, lastRunLine, lastRunTone, nextRunLine, type ScheduleForm,
  } from '$lib/utils/imber-schedule';

  let { store, onClose }: { store: ImberSchedulesStore; onClose: () => void } = $props();

  const adapter = $derived(store.adapter);
  const jobs = $derived(store.jobs);
  const editing = $derived(store.editing);

  // Re-read "in 3 h" every half minute while the window is open.
  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(t);
  });

  let form = $state<ScheduleForm>(blankForm(Date.now()));
  let formError = $state<string | null>(null);
  let saving = $state(false);
  let list = $state<HTMLElement | undefined>(undefined);

  // The form is (re)loaded whenever the editor opens on something.
  $effect(() => {
    const e = editing;
    if (e === null) return;
    if (e === 'new') {
      form = blankForm(Date.now(), store.seedForNew);
      if (!adapter.actions.includes(form.action)) form.action = adapter.actions[0];
    } else {
      form = formFromJob(e);
    }
    formError = null;
  });

  async function save() {
    if (saving) return;
    const existing = editing !== 'new' && editing !== null ? editing : null;
    const root = existing?.root ?? adapter.defaultRoot();
    if (!root) { formError = 'Open a project first: a schedule works on one.'; return; }
    const keep = existing?.action.kind === 'awake' ? existing.action.conversation_id : undefined;
    const input = inputFromForm(form, root, adapter.currentConversationId(), existing?.id, keep);
    if ('error' in input) { formError = input.error; return; }
    // A product with no write access for scheduled runs never sends anything but read-only.
    if (!adapter.allowModeChoice) input.mode = 'manual';
    saving = true;
    try {
      if (!(await store.save(input))) formError = 'It could not be saved — see the message.';
    } finally {
      saving = false;
    }
  }

  function rows(): HTMLElement[] {
    return list ? Array.from(list.querySelectorAll<HTMLElement>('[data-row]')) : [];
  }

  function onListKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    if (target.closest('input, textarea, select, [role=switch]')) return;
    const all = rows();
    const at = all.findIndex((r) => r === target.closest('[data-row]'));
    const job = at >= 0 ? jobs[at] : undefined;
    const go = (i: number) => { e.preventDefault(); all[Math.max(0, Math.min(all.length - 1, i))]?.focus(); };
    if (e.key === 'ArrowDown') go(at + 1);
    else if (e.key === 'ArrowUp') go(at <= 0 ? 0 : at - 1);
    else if (e.key === 'n' || e.key === 'N') { e.preventDefault(); store.edit('new'); }
    else if (job && e.key === 'Enter') { e.preventDefault(); store.edit(job); }
    else if (job && e.key === ' ') { e.preventDefault(); void store.setEnabled(job, !job.enabled); }
    else if (job && (e.key === 'r' || e.key === 'R')) { e.preventDefault(); void store.runNow(job); }
    else if (job && e.key === 'Delete') { e.preventDefault(); store.askDelete(job); }
  }

  // Esc in the editor goes back to the list (the Modal's own Esc closes the window from the list).
  function onWindowKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && editing !== null) { e.preventDefault(); e.stopPropagation(); store.closeEditor(); void tick().then(() => rows()[0]?.focus()); }
  }

  // The list takes the focus when it appears, so the arrows work without a click.
  // `listShown` is a boolean derived, so reloading the same jobs does not pull the focus back to the first row.
  const listShown = $derived(editing === null && jobs.length > 0 && !store.error);
  $effect(() => { if (listShown) void tick().then(() => rows()[0]?.focus()); });

  const modeBadge = (j: ImberJob) => (!adapter.allowModeChoice || j.action.kind === 'fetch' ? null : j.mode === 'auto' ? 'can change things' : 'read-only');
  const actionLabel = (j: ImberJob) => (j.action.kind === 'awake' ? 'wakes a conversation' : j.action.kind === 'prompt' ? 'new conversation' : 'fetch');
  const canOpenConversation = (j: ImberJob) => !!adapter.openConversation && j.action.kind === 'awake' && !!j.action.conversation_id;
</script>

<svelte:window onkeydowncapture={onWindowKeydown} />

<Modal {onClose} width="720px" height="min(620px, 88vh)" ariaLabel="Imber schedules">
  {#snippet header()}
    <ModalHeader onClose={onClose}>
      <CalendarClock size={14} class="header-icon" />
      <span class="modal-title">{editing === null ? 'Imber › Schedules' : editing === 'new' ? 'New schedule' : 'Edit schedule'}</span>
      {#if editing === null && jobs.length > 0}<Badge variant="count" size="sm" label={String(jobs.length)} />{/if}
      {#snippet actions()}
        {#if editing === null}
          <Button variant="primary" size="sm" onclick={() => store.edit('new')}><Plus size={12} /> New</Button>
        {/if}
      {/snippet}
    </ModalHeader>
  {/snippet}

  {#if editing !== null}
    <ImberScheduleForm bind:form actions={adapter.actions} allowModeChoice={adapter.allowModeChoice} productName={adapter.productName} error={formError} onsubmit={save} />
  {:else}
    <div class="view">
      <Alert variant="info" compact
        text={`Schedules run only while ${adapter.productName} is open: a run missed while it was closed is skipped or caught up once, according to the schedule.`} />
      {#if store.error}
        <EmptyState message="Could not load the schedules" description={store.error} />
      {:else if store.loading && jobs.length === 0}
        <div class="loading"><Spinner size="md" /></div>
      {:else if jobs.length === 0}
        <EmptyState message="No schedules yet" description="Schedule a prompt for Imber, or ask Imber to: “every weekday at 9, check what changed on main”. Press N to create one." />
      {:else}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="list" bind:this={list} onkeydown={onListKeydown} role="list" aria-label="Schedules">
          {#each jobs as job (job.id)}
            <div class="row" class:off={!job.enabled} data-row tabindex="0" role="listitem">
              <Toggle checked={job.enabled} size="sm" ariaLabel={`${job.name}: on or off`} onchange={(on) => void store.setEnabled(job, on)} />
              <div class="main">
                <div class="title">
                  <strong>{job.name}</strong>
                  <Badge variant="chip" size="sm" label={actionLabel(job)} />
                  {#if modeBadge(job)}<Badge variant="tone" tone={job.mode === 'auto' ? 'warning' : 'neutral'} size="sm" label={modeBadge(job) ?? ''} />{/if}
                </div>
                <div class="line">{describeRecurrence(job.recurrence)} · {nextRunLine(job, now)}</div>
                <div class="line" class:failed={lastRunTone(job) === 'failed'}>
                  {lastRunLine(job, now)}{#if job.last_run?.detail} — {job.last_run.detail}{/if}
                </div>
              </div>
              <div class="acts">
                {#if canOpenConversation(job)}
                  <IconButton size={24} tooltip="Open the conversation" onclick={() => void store.openConversation(job)}><MessageSquare size={13} /></IconButton>
                {/if}
                <IconButton size={24} tooltip="Run now (R)" onclick={() => void store.runNow(job)}><Play size={13} /></IconButton>
                <IconButton size={24} tooltip="Edit (Enter)" onclick={() => store.edit(job)}><Pencil size={13} /></IconButton>
                <IconButton size={24} tooltip="Delete (Del)" onclick={() => store.askDelete(job)}><Trash2 size={13} /></IconButton>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    {#if editing !== null}
      <ModalFooter hint="Ctrl+Enter to save · Esc to go back">
        <Button variant="ghost" onclick={() => store.closeEditor()}>Back</Button>
        <Button variant="primary" loading={saving} onclick={save}>Save</Button>
      </ModalFooter>
    {:else}
      <ModalFooter hint="↑↓ select · Enter edit · Space on/off · R run now · Del delete · N new">
        <Button variant="ghost" onclick={onClose}>Close</Button>
      </ModalFooter>
    {/if}
  {/snippet}
</Modal>

{#if store.deleting}
  <ConfirmModal
    title="Delete schedule"
    message={`Delete “${store.deleting.name}”?`}
    detail="Nothing it already did is undone."
    variant="danger"
    confirmLabel="Delete"
    onConfirm={() => void store.confirmDelete()}
    onCancel={() => store.cancelDelete()}
  />
{/if}

<style>
  .view { display: flex; flex-direction: column; gap: 10px; }
  .loading { display: flex; justify-content: center; padding: 40px; }
  .list { display: flex; flex-direction: column; gap: 6px; }
  .row {
    display: flex; align-items: flex-start; gap: 12px; padding: 10px 12px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-overlay);
    transition: border-color var(--transition-fast);
  }
  .row:hover, .row:focus-visible { border-color: var(--border); outline: none; }
  .row:focus-visible { border-color: var(--border-focus); }
  .row.off .main { opacity: 0.6; }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .title { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; font-size: var(--font-size-sm); }
  .line { font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .line.failed { color: var(--danger); }
  .acts { display: flex; gap: 2px; flex-shrink: 0; }
</style>
