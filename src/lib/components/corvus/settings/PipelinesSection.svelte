<script lang="ts">
  /**
   * Settings → Tools → Pipelines
   *
   * Single live-editable knob: the global concurrency cap on local pipeline
   * runs. Backend writes are immediate (no save button) — the orchestrator
   * picks up the new cap within ~250 ms, parked runs are woken via
   * `pipeline_cv.notify_all()` from the Tauri command.
   */
  import { onMount } from 'svelte';
  import { Workflow, Info, AlertTriangle, RotateCcw, Infinity as InfinityIcon } from 'lucide-svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { getPipelinesConfig, setPipelinesConfig } from '$lib/ipc/config';
  import type { PipelinesConfig } from '$lib/types/config';

  const DEFAULT_CAP = 4;

  let cfg     = $state<PipelinesConfig | null>(null);
  let loading = $state(true);
  let saving  = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let saved   = $state(false);

  // Local input copy — bound to the numeric input so the user can clear /
  // retype without each keystroke firing a backend write. Persisted on
  // blur and when the value differs from the loaded config.
  let cap = $state<number>(DEFAULT_CAP);

  onMount(async () => {
    try {
      cfg = await getPipelinesConfig();
      cap = clamp(cfg.max_concurrent_runs);
    } catch (e) {
      uiStore.showToast(`Failed to load pipelines config: ${e}`, 'error');
    } finally {
      loading = false;
    }
  });

  /** UI clamp — backend accepts u32, but the input is capped at a sane
   *  upper bound so the user can't accidentally key in a 9-digit value. */
  function clamp(n: number): number {
    if (!Number.isFinite(n) || n < 0) return DEFAULT_CAP;
    return Math.min(64, Math.trunc(n));
  }

  async function persist() {
    if (!cfg) return;
    const next: PipelinesConfig = { max_concurrent_runs: clamp(cap) };
    if (next.max_concurrent_runs === cfg.max_concurrent_runs) return;
    saving = true;
    try {
      await setPipelinesConfig(next);
      cfg = next;
      saved = true;
      if (saveTimer) clearTimeout(saveTimer);
      saveTimer = setTimeout(() => { saved = false; }, 1800);
    } catch (e) {
      uiStore.showToast(`Save failed: ${e}`, 'error');
    } finally {
      saving = false;
    }
  }

  // Enter → commit by blurring the field, which fires the input's
  // native `change` event and routes through onStepperChange below.
  function onCapKey(e: KeyboardEvent) {
    if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur();
  }

  // Single commit hook — fires for BOTH the typed-then-blurred path
  // (input's native `change` event) AND the +/- stepper clicks
  // (NumberStepper.nudge → onchange). Persist short-circuits when the
  // value is unchanged, so callers can fire it freely without churning
  // the backend on no-op edits.
  function onStepperChange(v: number) {
    cap = v;
    persist();
  }

  function resetDefault() {
    cap = DEFAULT_CAP;
    persist();
  }

  // True when the cap is in effect ⇒ extra runs queue. False when 0
  // (unlimited) ⇒ no queueing happens, the warning below applies.
  const unlimited = $derived(clamp(cap) === 0);
</script>

<SectionHeader
  title="Pipelines"
  description="Tune the local pipeline orchestrator. CI/CD runs on GitHub Actions / GitLab CI are scheduled by the provider and ignore these settings."
/>

{#if loading}
  <div class="state-msg">Loading…</div>
{:else if cfg}
  <div class="card">
    <div class="card-section-title"><Workflow size={12} /> Concurrency</div>

    <FormRow
      label="Max concurrent runs"
      description="Cap on the number of pipeline runs that may execute simultaneously across all plugins. Additional runs queue up with a 'Queued' badge in the Pipelines panel and start as soon as a slot frees up. Changes apply within ~250 ms — no app restart needed."
    >
      <div class="num-stack">
        <!-- Input + custom inline ▲▼ stepper: the action button used to
             sit beside the input in a flex row, but pressing ↑/↓ on the
             native input toggles the button's `disabled` state per
             keystroke (cap changes → "Reset to 4" enables/disables) and
             the resulting jitter looked broken. The Reset affordance
             now lives below the field as a subtle text button. -->
        <NumberStepper
          bind:value={cap}
          min={0}
          max={64}
          step={1}
          ariaLabel="Max concurrent pipeline runs"
          onkeydown={onCapKey}
          onchange={onStepperChange}
        />
        <div class="num-meta">
          {#if clamp(cap) !== DEFAULT_CAP}
            <button
              type="button"
              class="reset-link"
              onclick={resetDefault}
              disabled={saving}
            >
              <RotateCcw size={11} /> Reset to {DEFAULT_CAP}
            </button>
          {/if}
          {#if saving}
            <span class="status-text">Saving…</span>
          {:else if saved}
            <span class="status-text status-saved">Saved</span>
          {/if}
        </div>
      </div>
    </FormRow>

    <!-- Always-visible info: clarifies the meaning of `0`. Pinned here
         (not behind a `{#if unlimited}`) so users discovering the feature
         understand the contract before flipping the value. -->
    <div class="info-row">
      <Info size={12} />
      <span>
        <code>0</code> means <strong>unlimited</strong> — the orchestrator never queues, every
        run starts immediately.
      </span>
    </div>

    {#if unlimited}
      <!-- Loud warning when 0 is actually selected: heavy parallel runs
           contend on disk + network + libgit2 packfile readers and can
           starve the UI thread. -->
      <div class="warn-row">
        <AlertTriangle size={12} />
        <span>
          With <InfinityIcon size={11} /> unlimited concurrency, a burst of pipelines (e.g.
          <code>group-security-dashboard</code> fan-out, sequence runs) can saturate disk I/O,
          libgit2 packfile readers, and the network — leading to noticeable slowdowns and a
          jittery UI. Keep a cap unless you specifically need the parallelism.
        </span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .state-msg {
    padding: 12px 14px;
    color: var(--text-muted);
    font-size: 12px;
  }

  /* Vertical stack: stepper on top, meta row (Reset link + save status)
     below.  Replaces the previous flex row where the Reset button sat
     directly beside the input — that layout caused two problems:
       1. Pressing ↑/↓ on the input toggled `disabled` on the button
          per-keystroke, producing visual jitter.
       2. With a `narrow` input, the input + button + status text
          competed for the same flex track and looked cramped. */
  .num-stack {
    display: inline-flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  /* Meta row sits below the field — non-interactive when no override
     is in effect (the link is conditionally rendered) so the column
     collapses to just the stepper.  Subtle text-button styling because
     this is a recovery affordance, not the primary action. */
  .num-meta {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    min-height: 16px;
  }

  .reset-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    transition: color var(--transition-fast);
  }
  .reset-link:hover:not(:disabled) { color: var(--accent); }
  .reset-link:disabled { opacity: 0.4; cursor: default; }
  .reset-link :global(svg) { flex-shrink: 0; }

  .status-text {
    font-size: 11px;
    color: var(--text-muted);
  }
  .status-saved { color: var(--success); }

  .info-row,
  .warn-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: 12px;
    padding: 9px 11px;
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    line-height: 1.45;
  }
  .info-row {
    background: color-mix(in srgb, var(--accent) 7%, transparent);
    color: var(--text-secondary);
    border-left: 2px solid var(--accent);
  }
  .info-row :global(svg) { color: var(--accent); flex-shrink: 0; margin-top: 2px; }

  .warn-row {
    background: color-mix(in srgb, var(--warning) 10%, transparent);
    color: var(--text-secondary);
    border-left: 2px solid var(--warning);
  }
  .warn-row :global(svg) { color: var(--warning); flex-shrink: 0; margin-top: 2px; }

  .info-row code,
  .warn-row code {
    font-family: var(--font-code);
    font-size: 10.5px;
    padding: 1px 4px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 3px;
  }
</style>
