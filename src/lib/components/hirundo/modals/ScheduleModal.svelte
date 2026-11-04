<script lang="ts">
  /**
   * ScheduleModal — create or edit a schedule: what to run (a collection or folder), in which environment,
   * and when. The recurrence is the calendar of `arbor-scheduler`, the one the Imber schedules use: once,
   * every N minutes or hours, daily, on chosen weekdays, or a cron expression. The backend validates it and
   * the modal shows when the next run would be, so a wrong time is seen before it is saved.
   */
  import { onMount } from 'svelte';
  import { CalendarClock } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import ToggleButton from '$lib/components/shared/ui/ToggleButton.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { hirundoSchedules } from '$lib/stores/hirundo/schedules.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { Node, Recurrence, Schedule } from '$lib/types/hirundo';
  import { blankSchedule, describe, fromLocalInput, recurrenceOf, toLocalInput, WEEKDAYS, type Kind } from '../schedule-model';

  let { id, onClose }: { id: string | null; onClose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state<Schedule>(structuredClone($state.snapshot(hirundoSchedules.items.find((s) => s.id === id)) ?? blankSchedule(hirundoStore.tree[0]?.path ?? '')) as Schedule);
  let error = $state('');
  let saving = $state(false);
  let nameEl = $state<HTMLInputElement | undefined>(undefined);
  onMount(() => nameEl?.focus());

  const targets = $derived.by(() => {
    const out: { value: string; label: string }[] = [];
    const walk = (nodes: Node[], trail: string) => {
      for (const n of nodes) if (n.kind === 'folder') { out.push({ value: n.path, label: `${trail} / ${n.name}` }); walk(n.children, `${trail} / ${n.name}`); }
    };
    for (const c of hirundoStore.tree) { out.push({ value: c.path, label: c.name }); walk(c.children, c.name); }
    return out;
  });
  const envs = $derived([{ value: '', label: 'The active environment' }, ...hirundoStore.envs.environments.map((e) => ({ value: e.id, label: e.name }))]);

  const KINDS: { value: Kind; label: string }[] = [
    { value: 'daily', label: 'Daily' }, { value: 'weekly', label: 'Weekdays' }, { value: 'interval', label: 'Every…' }, { value: 'once', label: 'Once' }, { value: 'cron', label: 'Cron' },
  ];

  const r = $derived(draft.recurrence);
  const time = $derived(r.kind === 'daily' || r.kind === 'weekly' ? `${String(r.hour).padStart(2, '0')}:${String(r.minute).padStart(2, '0')}` : '');
  const protectedEnv = $derived(!!hirundoStore.envs.types.find((t) => t.id === hirundoStore.envs.environments.find((e) => e.id === (draft.env_id || hirundoStore.envs.local.active_env))?.type_id)?.confirm_mutating);

  function setKind(kind: string) {
    draft.recurrence = recurrenceOf(kind as Kind);
  }

  function setTime(text: string) {
    const [h, m] = text.split(':').map(Number);
    if (Number.isFinite(h) && Number.isFinite(m) && (draft.recurrence.kind === 'daily' || draft.recurrence.kind === 'weekly')) {
      draft.recurrence = { ...draft.recurrence, hour: Math.min(23, Math.max(0, h)), minute: Math.min(59, Math.max(0, m)) } as Recurrence;
    }
  }

  function toggleDay(d: number) {
    if (draft.recurrence.kind !== 'weekly') return;
    const days = draft.recurrence.days.includes(d) ? draft.recurrence.days.filter((x) => x !== d) : [...draft.recurrence.days, d];
    draft.recurrence = { ...draft.recurrence, days };
  }

  async function save() {
    saving = true;
    error = '';
    try {
      if (!draft.name.trim()) draft.name = `${targets.find((t) => t.value === draft.target)?.label ?? draft.target} · ${describe(draft.recurrence)}`;
      await hirundoSchedules.save($state.snapshot(draft) as Schedule);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void save(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="600px" height="640px" ariaLabel="Schedule">
  {#snippet header()}
    <ModalHeader {onClose}>
      <CalendarClock size={14} />
      <span class="modal-title">{id ? 'Edit schedule' : 'New schedule'}</span>
    </ModalHeader>
  {/snippet}

  <div class="sm">
    {#if error}<Alert variant="error" compact text={error} dismissible onclose={() => (error = '')} />{/if}

    <FormSection label="What to run" first>
      <FormField label="Name" hint="Leave empty to name it after what it runs.">
        <Input value={draft.name} bind:element={nameEl} placeholder="Smoke staging" oninput={(v) => (draft.name = v)} />
      </FormField>
      <FormField label="Collection or folder"><Select value={draft.target} options={targets} onchange={(v) => (draft.target = v)} /></FormField>
      <FormField label="Environment"><Select value={draft.env_id} options={envs} onchange={(v) => (draft.env_id = v)} /></FormField>
      {#if protectedEnv}
        <Alert variant="warning" compact text="This environment asks before POST, PUT, PATCH and DELETE. A scheduled run cannot ask, so those requests are reported as not sent." />
      {/if}
    </FormSection>

    <FormSection label="When">
      <RadioGroup value={r.kind} options={KINDS} appearance="segment" size="sm" onchange={setKind} />
      {#if r.kind === 'daily' || r.kind === 'weekly'}
        <FormField label="At"><Input type="time" value={time} narrow oninput={setTime} /></FormField>
      {/if}
      {#if r.kind === 'weekly'}
        <div class="sm-days">
          {#each WEEKDAYS as d, i (d)}<ToggleButton pressed={r.days.includes(i)} label={d} title={d} onclick={() => toggleDay(i)} />{/each}
        </div>
      {:else if r.kind === 'interval'}
        <FormRow label="Every" description="Minutes between runs (at least one).">
          <NumberStepper value={Math.round(r.secs / 60)} min={1} max={10080} step={5} onchange={(v: number) => (draft.recurrence = { ...r, secs: Math.max(1, v) * 60, anchor_ms: Date.now() })} />
        </FormRow>
      {:else if r.kind === 'once'}
        <FormField label="At"><Input type="datetime-local" value={toLocalInput(r.at_ms)} oninput={(v) => { const ms = fromLocalInput(v); if (ms !== null) draft.recurrence = { kind: 'once', at_ms: ms }; }} /></FormField>
      {:else if r.kind === 'cron'}
        <FormField label="Expression" hint="Six fields: second minute hour day-of-month month day-of-week."><Input value={r.expr} oninput={(v) => (draft.recurrence = { kind: 'cron', expr: v })} /></FormField>
      {/if}
      <p class="sm-sum">Runs {describe(draft.recurrence)}.</p>
    </FormSection>

    <FormSection label="Options">
      <FormRow label="Catch up a missed run" description="If Hirundo was closed at the time, run once when it opens. Off: skip it and wait for the next.">
        <Toggle checked={draft.missed === 'run_once'} ariaLabel="Catch up a missed run" onchange={(v) => (draft.missed = v ? 'run_once' : 'skip')} />
      </FormRow>
      <FormRow label="Enabled">
        <Toggle checked={draft.enabled} ariaLabel="Enabled" onchange={(v) => (draft.enabled = v)} />
      </FormRow>
    </FormSection>
    <p class="sm-note">Schedules run while the Hirundo window is open.</p>
  </div>

  {#snippet footer()}
    <ModalFooter hint="to save">
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" loading={saving} disabled={!draft.target} onclick={() => void save()}>Save</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .sm { display: flex; flex-direction: column; gap: 4px; padding: 8px 16px 12px; overflow: auto; height: 100%; }
  .sm-days { display: flex; gap: 4px; flex-wrap: wrap; }
  .sm-sum { margin: 4px 0 0; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .sm-note { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
