<script lang="ts">
  /**
   * The fields of one schedule: what it does, when, and how much freedom a run has with nobody there.
   * The caller owns the form object and the buttons; `Ctrl+Enter` in any field calls `onsubmit`.
   */
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { WEEKDAYS, type ActionKind, type ScheduleForm, type WhenKind } from './imber-schedule';

  let { form = $bindable(), error = null, onsubmit }: {
    form: ScheduleForm;
    /** What is wrong with the form, shown under it. */
    error?: string | null;
    onsubmit: () => void;
  } = $props();

  const ACTIONS = [
    { value: 'awake', label: 'Wake this conversation' },
    { value: 'prompt', label: 'Ask Imber in a new conversation' },
    { value: 'fetch', label: 'Fetch (no Imber, no cost)' },
  ];
  const WHENS = [
    { value: 'once', label: 'Once' },
    { value: 'interval', label: 'Every N minutes' },
    { value: 'daily', label: 'Every day' },
    { value: 'weekly', label: 'On weekdays' },
    { value: 'cron', label: 'Cron expression' },
  ];
  const MISSED = [
    { value: 'skip', label: 'Skip it' },
    { value: 'run_once', label: 'Run it once on the next start' },
  ];

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); onsubmit(); }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="form" onkeydown={onKeydown}>
  <FormField label="Name" required>
    <Input bind:value={form.name} placeholder="Morning check" />
  </FormField>

  <FormField label="What it does">
    <Select value={form.action} options={ACTIONS} fill ariaLabel="What it does" onchange={(v) => (form.action = v as ActionKind)} />
  </FormField>

  {#if form.action === 'fetch'}
    <FormField label="Remote" optionalText="(default origin)">
      <Input bind:value={form.remote} placeholder="origin" />
    </FormField>
  {:else}
    <FormField label="Prompt" required hint="What Imber is asked at that time. Say what to check and what to report: nobody is there to answer questions.">
      <Textarea bind:value={form.prompt} framed minRows={3} maxRows={8} maxLength={4000} placeholder="Check whether main has new commits and summarise them." ariaLabel="Prompt" />
    </FormField>
  {/if}

  <FormField label="When" error={error}>
    <Select value={form.when} options={WHENS} fill ariaLabel="When" onchange={(v) => (form.when = v as WhenKind)} />
    <div class="when">
      {#if form.when === 'once'}
        <Input bind:value={form.at} placeholder="2026-11-05 09:00" ariaLabel="Date and time, local" />
      {:else if form.when === 'interval'}
        <NumberStepper value={form.minutes} min={1} step={5} narrow={false} ariaLabel="Minutes" onchange={(v) => (form.minutes = v ?? 60)} />
        <span class="unit">minutes</span>
      {:else if form.when === 'daily'}
        <Input bind:value={form.time} placeholder="09:00" ariaLabel="Time of day" />
      {:else if form.when === 'weekly'}
        <div class="days">
          {#each WEEKDAYS as day, i}
            <Checkbox bind:checked={form.days[i]} label={day} />
          {/each}
        </div>
        <Input bind:value={form.time} placeholder="09:00" ariaLabel="Time of day" />
      {:else}
        <Input bind:value={form.cron} placeholder="0 0 9 * * 1-5" ariaLabel="Cron expression (sec min hour day month weekday)" />
      {/if}
    </div>
  </FormField>

  <FormField label="If Corvus was closed at the time">
    <Select value={form.missed} options={MISSED} fill ariaLabel="Missed runs" onchange={(v) => (form.missed = v as 'skip' | 'run_once')} />
  </FormField>

  {#if form.action !== 'fetch'}
    <Toggle
      checked={form.mode === 'auto'}
      label="Let it make changes"
      description="Off: the run is read-only. On: stage, commit, pull, push and the like run without asking. Force pushes and deletions never run unattended."
      onchange={(on) => (form.mode = on ? 'auto' : 'manual')}
    />
  {/if}
  <Toggle checked={form.enabled} label="On" onchange={(on) => (form.enabled = on)} />
</div>

<style>
  .form { display: flex; flex-direction: column; gap: 12px; }
  .when { display: flex; align-items: center; gap: 8px; margin-top: 6px; flex-wrap: wrap; }
  .days { display: flex; gap: 10px; flex-wrap: wrap; }
  .unit { font-size: var(--font-size-sm); color: var(--text-muted); }
</style>
