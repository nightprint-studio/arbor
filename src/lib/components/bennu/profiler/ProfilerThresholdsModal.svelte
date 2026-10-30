<script lang="ts">
  /**
   * The values at which the profiler turns a reading yellow, orange and red — one scale per kind of
   * reading, because their units differ. Saved with the project, so a team shares one idea of "hot".
   */
  import { SlidersHorizontal } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import type { ProfilerThresholds, SeverityLevels } from '$lib/ipc/bennu/profiler';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import { DEFAULT_THRESHOLDS, severityColor } from './profiler-filter';

  let { onClose }: { onClose: () => void } = $props();

  type Key = keyof ProfilerThresholds;
  const SCALES: { key: Key; label: string; description: string; unit: string; step: number }[] = [
    { key: 'cpu_self_pct', label: 'Method self time', description: 'Share of the recording spent in the method itself.', unit: '%', step: 1 },
    { key: 'gc_pause_pct', label: 'GC pauses', description: 'Share of the recording the program was stopped by a collector.', unit: '%', step: 0.5 },
    { key: 'alloc_share_pct', label: 'Allocation share', description: 'Share of everything allocated that one type or site accounts for.', unit: '%', step: 1 },
    { key: 'lock_total_ms', label: 'Lock wait', description: 'Total time threads waited on one monitor.', unit: 'ms', step: 10 },
  ];
  const STEPS: { field: keyof SeverityLevels; tone: 1 | 2 | 3; name: string }[] = [
    { field: 'yellow', tone: 1, name: 'Elevated' },
    { field: 'orange', tone: 2, name: 'High' },
    { field: 'red', tone: 3, name: 'Critical' },
  ];

  let draft = $state<ProfilerThresholds>(structuredClone($state.snapshot(store.thresholds)));

  const ordered = (l: SeverityLevels) => l.yellow <= l.orange && l.orange <= l.red;
  const valid = $derived(SCALES.every((s) => ordered(draft[s.key])));

  function save() {
    if (!valid) return;
    store.setThresholds($state.snapshot(draft));
    onClose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      save();
    }
  }
</script>

<Modal {onClose} width="620px" height="500px" ariaLabel="Profiler thresholds">
  {#snippet header()}
    <ModalHeader {onClose}>
      <SlidersHorizontal size={14} />
      <span class="modal-title">Profiler colour thresholds</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKey}>
    {#each SCALES as s (s.key)}
      <FormRow label={s.label} description={s.description} stacked>
        <div class="steps">
          {#each STEPS as step (step.field)}
            <label class="step">
              <span class="cap"><span class="dot" style:background={severityColor(step.tone)}></span>{step.name}</span>
              <span class="num">
                <NumberStepper
                  value={draft[s.key][step.field]}
                  min={0}
                  step={s.step}
                  ariaLabel={`${s.label}: ${step.name} from`}
                  onchange={(v) => (draft[s.key][step.field] = v)}
                />
                <span class="unit">{s.unit}</span>
              </span>
            </label>
          {/each}
        </div>
        {#if !ordered(draft[s.key])}
          <span class="bad">Each level must be at least the one before.</span>
        {/if}
      </FormRow>
    {/each}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" size="sm" onclick={() => (draft = structuredClone(DEFAULT_THRESHOLDS))}>Reset to defaults</Button>
      <div class="footer-actions">
        <Button variant="secondary" size="sm" onclick={onClose}>Cancel</Button>
        <Button variant="primary" size="sm" onclick={save} disabled={!valid} tooltip={{ content: 'Save', shortcut: 'Ctrl+Enter' }}>
          Save
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; gap: 14px; }
  .steps { display: flex; gap: 14px; }
  .step { display: flex; flex-direction: column; gap: 4px; }
  .cap { display: inline-flex; align-items: center; gap: 5px; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .dot { width: 8px; height: 8px; border-radius: 50%; }
  .num { display: inline-flex; align-items: center; gap: 4px; }
  .unit { font-size: var(--font-size-xs); color: var(--text-muted); }
  .bad { font-size: var(--font-size-xs); color: var(--error); }
  .footer-actions { display: flex; gap: 8px; }
</style>
