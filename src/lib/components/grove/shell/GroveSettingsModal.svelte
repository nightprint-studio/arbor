<script lang="ts">
  /**
   * Grove settings — the typed `[grove]` config (`~/.config/arbor/config.toml`),
   * edited through `configStore` (never localStorage; Arbor hard rule #11).
   * Apply-on-change: every control persists immediately, so there's no Save
   * button — Esc / Done just closes. Keyboard-first: Tab cycles the fields,
   * the first field auto-focuses (Modal), Esc cancels.
   */
  import { Settings, Music, Gauge, ScrollText, FileAudio } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { configStore, LOG_LEVELS, type GroveLogThreshold } from '../stores/config.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const logOptions = LOG_LEVELS.map((l) => ({ value: l, label: l }));
  const rateOptions = [44_100, 48_000, 88_200, 96_000].map((r) => ({ value: r, label: `${r / 1000} kHz` }));
  const depthOptions = [
    { value: 'int24',   label: '24-bit integer' },
    { value: 'float32', label: '32-bit float' },
  ];

  function setRate(v: string)  { configStore.setRender({ ...configStore.render, sample_rate: Number(v) }); }
  function setDepth(v: string) { configStore.setRender({ ...configStore.render, bit_depth: v }); }
  function setTail(v: number)  { configStore.setRender({ ...configStore.render, tail_max_secs: v }); }
</script>

<Modal {onClose} width="560px" height="540px" ariaLabel="Grove Settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings size={14} />
      <span class="modal-title">Settings</span>
    </ModalHeader>
  {/snippet}

  <div class="gs">
  <div class="grp-label">Language defaults</div>

  <div class="row">
    <FormField label="Default octave" hint="Octave for bare note names (e.g. c → c4).">
      {#snippet icon()}<Music size={12} />{/snippet}
      <NumberStepper value={configStore.defaultOctave} min={0} max={9} narrow onchange={(v) => configStore.setDefaultOctave(v)} ariaLabel="Default octave" />
    </FormField>

    <FormField label="Default tempo" hint="Cycles per second when a file omits cps().">
      {#snippet icon()}<Gauge size={12} />{/snippet}
      <NumberStepper value={configStore.defaultCps} min={0.05} step={0.05} narrow suffix="cps" onchange={(v) => configStore.setDefaultCps(v)} ariaLabel="Default cps" />
    </FormField>
  </div>

  <FormField label="Log threshold" hint="Lines below this level are never produced — no IPC flood.">
    {#snippet icon()}<ScrollText size={12} />{/snippet}
    <Select value={configStore.logThreshold} options={logOptions} onchange={(v) => configStore.setLogThreshold(v as GroveLogThreshold)} />
  </FormField>

  <div class="grp-label">Offline render</div>

  <div class="row">
    <FormField label="Sample rate">
      {#snippet icon()}<FileAudio size={12} />{/snippet}
      <Select value={configStore.render.sample_rate} options={rateOptions} onchange={setRate} />
    </FormField>

    <FormField label="Bit depth">
      <Select value={configStore.render.bit_depth} options={depthOptions} onchange={setDepth} />
    </FormField>
  </div>

  <FormField label="Reverb tail" hint="Extra seconds rendered after the last event so reverb / delay tails aren't cut.">
    <NumberStepper value={configStore.render.tail_max_secs} min={0} step={0.5} narrow suffix="s" onchange={setTail} ariaLabel="Reverb tail seconds" />
  </FormField>
  </div>

  {#snippet footer()}
    <Button variant="primary" onclick={onClose}>Done</Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .grp-label {
    font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.6px;
    color: var(--text-muted); margin: 4px 0 10px;
  }
  .grp-label:not(:first-child) { margin-top: 22px; }
  .row { display: flex; gap: 16px; margin-bottom: 14px; }
  .row > :global(.form-field) { flex: 1; }
  /* Standalone fields (Log threshold, Reverb tail) keep their own rhythm. */
  .gs > :global(.form-field) { margin-bottom: 14px; }
</style>
