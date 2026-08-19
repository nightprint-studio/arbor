<script lang="ts">
  /**
   * CaptureOptions — mode-aware capture settings.
   *
   * Rows come from the shared `FormRow` (icon variant) inside a boxed `FormSection`,
   * which is the same pair the Tyto settings dialog uses: these are the same
   * decisions in two places, and the moment they stop looking identical the reader
   * has to work out whether they *are* the same.
   *
   * Which rows appear follows from what the recording produces — a video has audio
   * and a bitrate, an image sequence has neither, and offering a microphone for a
   * folder of PNGs would be a control that quietly does nothing.
   */
  import { Volume2, Mic, Film, Sparkles, FolderOpen, Clipboard, FileImage, Timer, Images, Gauge, Scaling } from 'lucide-svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import {
    recorderStore, TYTO_FPS_OPTIONS, TYTO_QUALITY_OPTIONS, TYTO_COUNTDOWN_OPTIONS,
    TYTO_IMAGE_FORMAT_OPTIONS, TYTO_OUTPUT_OPTIONS, TYTO_FRAME_WIDTH_OPTIONS,
    type Fps, type Quality, type ScreenshotFormat, type FrameFormat, type RecordOutput,
  } from '$lib/stores/tyto/recorder.svelte';

  let folderPickerOpen = $state(false);

  const frames = $derived(recorderStore.recordOutput === 'frames');

  const micOptions = $derived([
    { value: '', label: 'No microphone' },
    ...recorderStore.mics.map((m) => ({ value: m.id, label: m.name + (m.default ? ' (default)' : '') })),
  ]);
</script>

<FormSection boxed first>
  {#if recorderStore.mode === 'record'}
    <FormRow label="Output" description={frames ? 'lossless stills, no audio' : 'H.264 video with audio'}>
      {#snippet icon()}<Images size={14} />{/snippet}
      <RadioGroup appearance="segment" size="sm" nowrap value={recorderStore.recordOutput} options={TYTO_OUTPUT_OPTIONS} onchange={(v) => recorderStore.setRecordOutput(v as RecordOutput)} />
    </FormRow>

    {#if frames}
      <FormRow label="Frame format" description="PNG keeps text crisp">
        {#snippet icon()}<FileImage size={14} />{/snippet}
        <RadioGroup appearance="segment" size="sm" value={recorderStore.frameFormat} options={TYTO_IMAGE_FORMAT_OPTIONS} onchange={(v) => recorderStore.setFrameFormat(v as FrameFormat)} />
      </FormRow>

      <FormRow label="Sample rate" description="at most — a still screen writes nothing">
        {#snippet icon()}<Gauge size={14} />{/snippet}
        <NumberStepper value={recorderStore.frameSampleFps} min={1} max={60} suffix="fps" size="sm" ariaLabel="Frames sampled per second" onchange={(v) => recorderStore.setFrameSampleFps(v)} />
      </FormRow>

      <FormRow label="Frame width" description="the biggest lever on size">
        {#snippet icon()}<Scaling size={14} />{/snippet}
        <Select value={String(recorderStore.frameMaxWidth)} options={TYTO_FRAME_WIDTH_OPTIONS} onchange={(v) => recorderStore.setFrameMaxWidth(Number(v))} />
      </FormRow>
    {:else}
      <FormRow label="System audio" description="WASAPI loopback">
        {#snippet icon()}<Volume2 size={14} />{/snippet}
        <Toggle checked={recorderStore.systemAudio} onchange={() => recorderStore.toggleSystemAudio()} ariaLabel="Capture system audio" />
      </FormRow>

      <FormRow label="Microphone">
        {#snippet icon()}<Mic size={14} />{/snippet}
        <Select value={recorderStore.micId ?? ''} options={micOptions} onchange={(v) => recorderStore.setMic(v || null)} />
      </FormRow>

      <FormRow label="Frame rate" description="frames per second">
        {#snippet icon()}<Film size={14} />{/snippet}
        <RadioGroup appearance="segment" size="sm" value={String(recorderStore.fps)} options={TYTO_FPS_OPTIONS} onchange={(v) => recorderStore.setFps(Number(v) as Fps)} />
      </FormRow>

      <FormRow label="Quality" description="{recorderStore.bitrateKbps ? `≈ ${(recorderStore.bitrateKbps / 1000).toFixed(0)} Mbps · ` : ''}H.264">
        {#snippet icon()}<Sparkles size={14} />{/snippet}
        <RadioGroup appearance="segment" size="sm" value={recorderStore.quality} options={TYTO_QUALITY_OPTIONS} onchange={(v) => recorderStore.setQuality(v as Quality)} />
      </FormRow>
    {/if}

    <FormRow label="Countdown" description="3-2-1 on screen before recording">
      {#snippet icon()}<Timer size={14} />{/snippet}
      <RadioGroup appearance="segment" size="sm" value={String(recorderStore.countdownSecs)} options={TYTO_COUNTDOWN_OPTIONS} onchange={(v) => recorderStore.setCountdownSecs(Number(v))} />
    </FormRow>
  {:else}
    <FormRow label="Copy to clipboard" description="also keeps the file">
      {#snippet icon()}<Clipboard size={14} />{/snippet}
      <Toggle checked={recorderStore.copyToClipboard} onchange={(v) => recorderStore.setCopyToClipboard(v)} ariaLabel="Copy screenshot to clipboard" />
    </FormRow>

    <FormRow label="Format" description="screenshot image format">
      {#snippet icon()}<FileImage size={14} />{/snippet}
      <RadioGroup appearance="segment" size="sm" value={recorderStore.screenshotFormat} options={TYTO_IMAGE_FORMAT_OPTIONS} onchange={(v) => recorderStore.setScreenshotFormat(v as ScreenshotFormat)} />
    </FormRow>
  {/if}

  <FormRow label="Save to" description={recorderStore.outputDir || 'resolving…'}>
    {#snippet icon()}<FolderOpen size={14} />{/snippet}
    <Button variant="secondary" size="sm" tooltip={{ content: 'Choose the output folder' }} onclick={() => (folderPickerOpen = true)}>Change…</Button>
  </FormRow>
</FormSection>

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
