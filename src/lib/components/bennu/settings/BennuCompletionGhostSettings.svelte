<script lang="ts">
  /** Settings › Completion › Ghost text — the grey line drawn ahead of the caret, from the project's patterns or the local model. */
  import { onMount } from 'svelte';
  import { Cpu, FolderOpen, RefreshCw } from 'lucide-svelte';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { flcStatus, type FlcStatus } from '$lib/ipc/bennu/flc';
  import { CONFIDENCE_STEP, MAX_CONFIDENCE, MIN_CONFIDENCE } from '$lib/stores/bennu/flc-settings';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

  const s = bennuSettingsStore;

  let picking = $state(false);
  let status = $state<FlcStatus | null>(null);
  async function refreshStatus() {
    status = await flcStatus().catch(() => null);
  }
  onMount(refreshStatus);
  const statusLine = $derived(
    status
      ? `Model: ${status.model}. Log statements counted: ${status.log_calls} in ${status.log_patterns} patterns${status.log_scanning ? ' (counting…)' : ''}.`
      : 'Status unavailable.',
  );
</script>

<div class="section-header">
  <h2>Ghost text</h2>
  <p>The grey continuation drawn after the caret: Tab takes it, Esc dismisses it.</p>
</div>
<div class="card">
  <div class="card-section-title"><Cpu size={12} /> Model suggestions (experimental)</div>
  <Callout variant="info" title="A guess, not a fact">
    A small local model finishes the line you are typing and the editor draws it in grey: Tab takes it, Esc dismisses it. It runs on your machine, in the background, and answers nothing while it is busy. Changes apply from the next suggestion.
  </Callout>
  <FormRow label="Finish the line with the local model" description="Off until you ask for it. Java only. The model never leaves your machine.">
    <Toggle checked={s.flc.enabled} onchange={(v) => s.setFlc('enabled', v)} ariaLabel="Finish the line with the local model" />
  </FormRow>
  <FormRow label="Model folder" description="The folder with flc_prefill.onnx, flc.onnx, vocab.txt and flc.meta. Point it at a personal copy to use a model adapted to your own code.">
    <div class="model-dir">
      <Input value={s.flc.modelDir} placeholder="No folder chosen" disabled={!s.flc.enabled}
             onchange={(v) => s.setFlc('modelDir', v)} ariaLabel="Model folder" />
      <Button variant="secondary" size="sm" disabled={!s.flc.enabled} onclick={() => (picking = true)}>
        <FolderOpen size={13} /> Browse…
      </Button>
    </div>
  </FormRow>
  <FormRow label="Confidence needed" description="The model's line is drawn only when it is at least this sure. Higher shows less and is right more often.">
    <NumberStepper value={s.flc.minConfidence} min={MIN_CONFIDENCE} max={MAX_CONFIDENCE} step={CONFIDENCE_STEP} narrow suffix="%"
                   disabled={!s.flc.enabled}
                   onchange={(v) => s.setFlc('minConfidence', v)} ariaLabel="Confidence needed in percent" />
  </FormRow>
  <FormRow label="Complete log statements" description="Finish a log call from the pattern this project writes most in that place — the entry line of a method, a catch block. Counted once in the background, kept current as files change; needs no model.">
    <Toggle checked={s.flc.logs} onchange={(v) => s.setFlc('logs', v)} ariaLabel="Complete log statements" />
  </FormRow>
  <FormRow label="Check with Bennu" description="Run the model's line through Bennu's code checks and drop it when it names a method the receiver does not have, passes the wrong arguments or uses a variable that does not exist. A class you have not imported yet is not counted against it. Costs a validation of the file per suggestion.">
    <Toggle checked={s.flc.check} disabled={!s.flc.enabled} onchange={(v) => s.setFlc('check', v)} ariaLabel="Check with Bennu" />
  </FormRow>
  <FormRow label="Ignore commented-out code" description="Leaves blocks of code in // comments out of what the model reads, so it does not copy a version you commented out into what you write under it. Explanations in comments are still read. Off by default: old code above can also be what you want repeated.">
    <Toggle checked={s.flc.ignoreCommentedCode} disabled={!s.flc.enabled} onchange={(v) => s.setFlc('ignoreCommentedCode', v)} ariaLabel="Ignore commented-out code" />
  </FormRow>
  <FormRow label="Try three candidates" description="Ask the model for three answers and show the first one that passes the checks, instead of showing nothing when its only answer does not. Several times slower per suggestion (about 280 ms instead of 60), so it is off by default.">
    <Toggle checked={s.flc.alternatives} disabled={!s.flc.enabled} onchange={(v) => s.setFlc('alternatives', v)} ariaLabel="Try three candidates" />
  </FormRow>
  <FormRow label="Status" description={statusLine}>
    <Button variant="secondary" size="sm" onclick={refreshStatus}><RefreshCw size={13} /> Refresh</Button>
  </FormRow>
</div>

{#if picking}
  <FileExplorerModal
    mode="folder"
    title="Select the model folder"
    initialPath={s.flc.modelDir || undefined}
    onConfirm={(dir) => { s.setFlc('modelDir', dir); picking = false; }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  .model-dir { display: flex; align-items: center; gap: 6px; min-width: 0; }
</style>
