<script lang="ts">
  /**
   * CaptureOptions — mode-aware capture settings, laid out as a clean card of
   * setting rows (tinted icon · label + hint · control) separated by dividers.
   * Record: audio, mic, frame rate, quality. Screenshot: clipboard, format. Both
   * share the output-folder row (shared file picker).
   */
  import { Volume2, Mic, Film, Sparkles, FolderOpen, Clipboard, FileImage, Timer } from 'lucide-svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { recorderStore, type Fps, type Quality, type ScreenshotFormat } from '$lib/stores/tyto/recorder.svelte';

  let folderPickerOpen = $state(false);
  // Screenshot-only, ephemeral in the mock: copy the grab to the clipboard.
  let copyToClipboard = $state(true);

  const fpsOptions = [
    { value: '30', label: '30' },
    { value: '60', label: '60' },
  ];
  const qualityOptions = [
    { value: 'high',     label: 'High' },
    { value: 'balanced', label: 'Balanced' },
    { value: 'compact',  label: 'Compact' },
  ];
  const countdownOptions = [
    { value: '0',  label: 'Off' },
    { value: '3',  label: '3s' },
    { value: '5',  label: '5s' },
    { value: '10', label: '10s' },
  ];
  const formatOptions = [
    { value: 'png',  label: 'PNG' },
    { value: 'jpg',  label: 'JPG' },
    { value: 'webp', label: 'WebP' },
  ];

  const micOptions = $derived([
    { value: '', label: 'No microphone' },
    ...recorderStore.mics.map((m) => ({ value: m.id, label: m.name + (m.default ? ' (default)' : '') })),
  ]);
</script>

<div class="opts">
  {#if recorderStore.mode === 'record'}
    <div class="opt">
      <span class="opt-ic"><Volume2 size={14} /></span>
      <div class="opt-text"><div class="opt-name">System audio</div><div class="opt-hint">WASAPI loopback</div></div>
      <Toggle checked={recorderStore.systemAudio} onchange={() => recorderStore.toggleSystemAudio()} ariaLabel="Capture system audio" />
    </div>

    <div class="opt">
      <span class="opt-ic"><Mic size={14} /></span>
      <div class="opt-text"><div class="opt-name">Microphone</div></div>
      <Select value={recorderStore.micId ?? ''} options={micOptions} onchange={(v) => recorderStore.setMic(v || null)} />
    </div>

    <div class="opt">
      <span class="opt-ic"><Film size={14} /></span>
      <div class="opt-text"><div class="opt-name">Frame rate</div><div class="opt-hint">frames per second</div></div>
      <RadioGroup appearance="segment" size="sm" value={String(recorderStore.fps)} options={fpsOptions} onchange={(v) => recorderStore.setFps(Number(v) as Fps)} />
    </div>

    <div class="opt">
      <span class="opt-ic"><Sparkles size={14} /></span>
      <div class="opt-text"><div class="opt-name">Quality</div><div class="opt-hint">≈ {(recorderStore.bitrateKbps / 1000).toFixed(0)} Mbps · H.264</div></div>
      <RadioGroup appearance="segment" size="sm" value={recorderStore.quality} options={qualityOptions} onchange={(v) => recorderStore.setQuality(v as Quality)} />
    </div>

    <div class="opt">
      <span class="opt-ic"><Timer size={14} /></span>
      <div class="opt-text"><div class="opt-name">Countdown</div><div class="opt-hint">3-2-1 on screen before recording</div></div>
      <RadioGroup appearance="segment" size="sm" value={String(recorderStore.countdownSecs)} options={countdownOptions} onchange={(v) => recorderStore.setCountdownSecs(Number(v))} />
    </div>
  {:else}
    <div class="opt">
      <span class="opt-ic"><Clipboard size={14} /></span>
      <div class="opt-text"><div class="opt-name">Copy to clipboard</div><div class="opt-hint">also keeps the file</div></div>
      <Toggle bind:checked={copyToClipboard} ariaLabel="Copy screenshot to clipboard" />
    </div>
    <div class="opt">
      <span class="opt-ic"><FileImage size={14} /></span>
      <div class="opt-text"><div class="opt-name">Format</div><div class="opt-hint">screenshot image format</div></div>
      <RadioGroup appearance="segment" size="sm" value={recorderStore.screenshotFormat} options={formatOptions} onchange={(v) => recorderStore.setScreenshotFormat(v as ScreenshotFormat)} />
    </div>
  {/if}

  <div class="opt">
    <span class="opt-ic"><FolderOpen size={14} /></span>
    <div class="opt-text">
      <div class="opt-name">Save to</div>
      <div class="opt-hint path" title={recorderStore.outputDir}>{recorderStore.outputDir}</div>
    </div>
    <Button variant="secondary" size="sm" tooltip={{ content: 'Choose the output folder' }} onclick={() => (folderPickerOpen = true)}>Change…</Button>
  </div>
</div>

{#if folderPickerOpen}
  <FileExplorerModal
    mode="folder"
    title="Choose the Tyto output folder"
    initialPath={recorderStore.outputDir}
    onConfirm={(path: string) => { recorderStore.setOutputDir(path); folderPickerOpen = false; }}
    onCancel={() => (folderPickerOpen = false)}
    onClose={() => (folderPickerOpen = false)}
  />
{/if}

<style>
  .opts {
    display: flex; flex-direction: column;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .opt {
    display: flex; align-items: center; gap: 11px;
    padding: 11px 14px;
    min-height: 46px;
  }
  .opt + .opt { border-top: 1px solid var(--border-subtle); }

  .opt-ic {
    display: flex; align-items: center; justify-content: center;
    width: 28px; height: 28px; flex-shrink: 0;
    border-radius: 8px;
    color: var(--accent);
    background: var(--accent-subtle);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 22%, transparent);
  }

  .opt-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .opt-name { font-size: 12.5px; color: var(--text-primary); font-weight: 500; }
  .opt-hint { font-size: 10.5px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .opt-hint.path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 240px; }
</style>
