<script lang="ts">
  /**
   * Merula settings — the typed `[merula]` config (`~/.config/arbor/config.toml`),
   * edited through `configStore` (never localStorage; Arbor hard rule #11).
   * Apply-on-change: every control persists immediately, so there's no Save
   * button — Esc / Done just closes. Same two-pane shell + card styling as
   * Arbor's settings (the shared `SettingsShell`).
   */
  import { onMount } from 'svelte';
  import { Settings, Music, FileAudio, Volume2, Boxes } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SettingsShell, { type SettingsNavGroup } from '$lib/components/shared/ui/SettingsShell.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import RenderFormatFields from './RenderFormatFields.svelte';
  import TranscriptionModels from './TranscriptionModels.svelte';
  import { configStore, LOG_LEVELS, type MerulaLogThreshold } from '../stores/config.svelte';
  import { merulaAudioDevices, type MerulaAudioDevice } from '$lib/ipc/merula/merula';

  let { onClose }: { onClose: () => void } = $props();

  // Grouped sidebar — new config groups slot in here as they appear.
  const groups: SettingsNavGroup[] = [
    { label: 'Editor', items: [{ id: 'general', label: 'General', icon: Music }] },
    { label: 'Audio',  items: [
      { id: 'audio',  label: 'Output', icon: Volume2 },
      { id: 'render', label: 'Render', icon: FileAudio },
    ] },
    { label: 'Import', items: [{ id: 'models', label: 'Transcription', icon: Boxes }] },
  ];
  let active = $state('general');

  // Audio output devices (queried once on open). The empty value means "system
  // default" (a null device — always reachable even if the saved name is gone).
  let devices = $state<MerulaAudioDevice[]>([]);
  onMount(async () => { try { devices = await merulaAudioDevices(); } catch { /* none */ } });
  const deviceOptions = $derived([
    { value: '', label: 'System default' },
    ...devices.map((d) => ({ value: d.name, label: d.is_default ? `${d.name} (default)` : d.name })),
  ]);

  const logOptions = LOG_LEVELS.map((l) => ({ value: l, label: l }));

  function setRate(v: number)  { configStore.setRender({ ...configStore.render, sample_rate: v }); }
  function setDepth(v: string) { configStore.setRender({ ...configStore.render, bit_depth: v }); }
  function setTail(v: number)  { configStore.setRender({ ...configStore.render, tail_max_secs: v }); }
</script>

<Modal {onClose} width="840px" height="540px" padBody={false} ariaLabel="Merula Settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings size={14} />
      <span class="modal-title">Settings</span>
    </ModalHeader>
  {/snippet}

  <SettingsShell {groups} bind:active>
    {#snippet content()}
      {#if active === 'general'}
        <div class="section-header">
          <h2>General</h2>
          <p>Language defaults applied when a <code>.merula</code> file doesn't set them itself.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Music size={12} /> Language defaults</div>
          <FormRow label="Default octave" description="Octave for bare note names (e.g. c → c4).">
            <NumberStepper value={configStore.defaultOctave} min={0} max={9} narrow onchange={(v) => configStore.setDefaultOctave(v)} ariaLabel="Default octave" />
          </FormRow>
          <FormRow label="Default tempo" description="Cycles per second when a file omits cps().">
            <NumberStepper value={configStore.defaultCps} min={0.05} step={0.05} narrow suffix="cps" onchange={(v) => configStore.setDefaultCps(v)} ariaLabel="Default cps" />
          </FormRow>
          <FormRow label="Log threshold" description="Lines below this level are never produced — no IPC flood.">
            <Select value={configStore.logThreshold} options={logOptions} onchange={(v) => configStore.setLogThreshold(v as MerulaLogThreshold)} />
          </FormRow>
        </div>
        <div class="card">
          <div class="card-section-title"><Music size={12} /> Transport</div>
          <FormRow label="Step distance" description="How far the step-back / step-forward buttons (Ctrl+[ / Ctrl+]) move the playhead.">
            <NumberStepper value={configStore.skipStep} min={0.25} max={16} step={0.25} narrow suffix="cyc" onchange={(v) => configStore.setSkipStep(v)} ariaLabel="Step distance in cycles" />
          </FormRow>
        </div>
      {:else if active === 'audio'}
        <div class="section-header">
          <h2>Audio output</h2>
          <p>Where live playback is sent.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><Volume2 size={12} /> Output</div>
          <FormRow label="Output device" description="Changing it switches a running session immediately.">
            <Select value={configStore.outputDevice ?? ''} options={deviceOptions} onchange={(v) => configStore.setOutputDevice(v === '' ? null : v)} />
          </FormRow>
        </div>
      {:else if active === 'render'}
        <div class="section-header">
          <h2>Offline render</h2>
          <p>Defaults for bouncing the arrangement to an audio file.</p>
        </div>
        <div class="card">
          <div class="card-section-title"><FileAudio size={12} /> Format</div>
          <RenderFormatFields
            sampleRate={configStore.render.sample_rate}
            bitDepth={configStore.render.bit_depth}
            tail={configStore.render.tail_max_secs}
            onSampleRate={setRate}
            onBitDepth={setDepth}
            onTail={setTail}
          />
        </div>
      {:else if active === 'models'}
        <div class="section-header">
          <h2>Transcription models</h2>
          <p>Optional ONNX models for audio import, downloaded on demand. basic-pitch gives polyphonic, chord-aware pitch; Demucs splits the mix into stems for cleaner notes. Once installed they're used automatically.</p>
        </div>
        <TranscriptionModels />
      {/if}
    {/snippet}
  </SettingsShell>
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
</style>
