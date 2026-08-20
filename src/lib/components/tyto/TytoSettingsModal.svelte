<script lang="ts">
  /**
   * TytoSettingsModal — Tyto's settings.
   *
   * Built on the same `FormSection` + `FormRow` pair the capture panel uses, and for
   * the same reason: most of what is here IS what is there, persisted to the same
   * file, so the two must read as one product rather than as two dialogs that happen
   * to change the same values.
   *
   * What only lives here is what has no place beside a capture button: the OS
   * screen-recording permission (where you go when it was refused) and the opt-in
   * global shortcut.
   */
  import {
    Keyboard, FolderOpen, Clapperboard, Timer, Images, FileImage, Gauge, Scaling,
    Clipboard, Camera, ExternalLink,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tytoConfigStore } from '$lib/stores/tyto/config.svelte';
  import { openScreenRecordingSettings } from '$lib/ipc/tyto/main-window';
  import {
    recorderStore, TYTO_MODE_OPTIONS, TYTO_COUNTDOWN_OPTIONS, TYTO_IMAGE_FORMAT_OPTIONS,
    TYTO_OUTPUT_OPTIONS, TYTO_FRAME_WIDTH_OPTIONS,
    type CaptureMode, type ScreenshotFormat, type FrameFormat, type RecordOutput,
  } from '$lib/stores/tyto/recorder.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import AppearanceSettings from '$lib/components/shared/internal/AppearanceSettings.svelte';
  import AnimationsSettings from '$lib/components/shared/internal/AnimationsSettings.svelte';
  import ThemeEditorModal from '$lib/components/shared/ThemeEditorModal.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let capturing = $state(false);
  /** The shared theme editor, reachable from the Interface section. */
  let themeEditorOpen = $state(false);
  let folderPickerOpen = $state(false);

  const frames = $derived(recorderStore.recordOutput === 'frames');
  const blocked = $derived(recorderStore.captureUnavailable);
  // "Granted" is only true if something actually checked. With the backend down
  // nothing has, and claiming a permission we never verified is worse than saying so.
  const permissionState = $derived(
    blocked ? blocked
      : recorderStore.backendUp ? 'Granted — Arbor can capture your screen.'
      : "Not checked — the recorder backend isn't running.",
  );
  // The shell's diagnosis, when it has one, is a paragraph rather than a label — so
  // this row uses the wrapping FormRow instead of the compact one its neighbours use.
  // The compact variant ellipsizes, which is right for a file path and wrong for an
  // explanation whose whole value is the part that would be cut off.
  const permissionDetail = $derived(
    [permissionState, recorderStore.captureDiagnosis].filter(Boolean).join(' '),
  );

  async function toggleShortcut(on: boolean) {
    try {
      await tytoConfigStore.setGlobalShortcut(on);
    } catch (err) {
      uiStore.showToast(`Couldn't ${on ? 'enable' : 'disable'} the shortcut: ${err}`, 'error');
    }
  }

  /** Send the user to the OS pane where the permission lives. After a refusal this is
   *  the only way out — the system never asks a second time. */
  async function openPrivacySettings() {
    const opened = await openScreenRecordingSettings().catch(() => false);
    if (!opened) {
      uiStore.showToast('Open your system privacy settings and allow Arbor to record the screen.', 'info');
    }
  }

  /** Format a keydown into a Tauri accelerator string (needs ≥1 modifier). */
  function toAccelerator(e: KeyboardEvent): string | null {
    if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) return null;
    const mods: string[] = [];
    if (e.ctrlKey) mods.push('Ctrl');
    if (e.altKey) mods.push('Alt');
    if (e.shiftKey) mods.push('Shift');
    if (e.metaKey) mods.push('Super');
    if (mods.length === 0) return null; // a bare key is a poor global hotkey
    const key = e.key === ' ' ? 'Space' : (e.key.length === 1 ? e.key.toUpperCase() : e.key);
    return [...mods, key].join('+');
  }

  async function onCaptureKey(e: KeyboardEvent) {
    if (!capturing) return;
    // Capture phase: keep this keystroke from reaching Modal's Esc-to-close.
    e.preventDefault();
    e.stopImmediatePropagation();
    if (e.key === 'Escape') { capturing = false; return; }
    const accel = toAccelerator(e);
    if (!accel) return;
    capturing = false;
    try {
      await tytoConfigStore.setAccelerator(accel);
    } catch (err) {
      uiStore.showToast(`“${accel}” couldn't be registered: ${err}`, 'error');
    }
  }
</script>

<svelte:window onkeydowncapture={onCaptureKey} />

<Modal {onClose} width="660px" height="620px" ariaLabel="Tyto settings">
  {#snippet header()}
    <ModalHeader title="Tyto settings" {onClose} />
  {/snippet}

  <div class="body">
    <FormSection label="Capture" boxed first>
      <FormRow label="Default mode" description="what a fresh window opens in">
        {#snippet icon()}<Clapperboard size={14} />{/snippet}
        <RadioGroup appearance="segment" size="sm" nowrap value={recorderStore.mode} options={TYTO_MODE_OPTIONS} onchange={(v) => recorderStore.setMode(v as CaptureMode)} />
      </FormRow>

      <FormRow label="Countdown" description="3-2-1 on screen before recording">
        {#snippet icon()}<Timer size={14} />{/snippet}
        <RadioGroup appearance="segment" size="sm" nowrap value={String(recorderStore.countdownSecs)} options={TYTO_COUNTDOWN_OPTIONS} onchange={(v) => recorderStore.setCountdownSecs(Number(v))} />
      </FormRow>

      <FormRow label="Save to" description={recorderStore.outputDir || 'resolving…'}>
        {#snippet icon()}<FolderOpen size={14} />{/snippet}
        <Button variant="secondary" size="sm" onclick={() => (folderPickerOpen = true)}>Change…</Button>
      </FormRow>
    </FormSection>

    <FormSection label="Recording" boxed>
      <FormRow label="Output" description={frames ? 'lossless stills, no audio' : 'H.264 video with audio'}>
        {#snippet icon()}<Images size={14} />{/snippet}
        <RadioGroup appearance="segment" size="sm" nowrap value={recorderStore.recordOutput} options={TYTO_OUTPUT_OPTIONS} onchange={(v) => recorderStore.setRecordOutput(v as RecordOutput)} />
      </FormRow>

      {#if frames}
        <FormRow label="Frame format" description="PNG keeps text crisp">
          {#snippet icon()}<FileImage size={14} />{/snippet}
          <RadioGroup appearance="segment" size="sm" nowrap value={recorderStore.frameFormat} options={TYTO_IMAGE_FORMAT_OPTIONS} onchange={(v) => recorderStore.setFrameFormat(v as FrameFormat)} />
        </FormRow>

        <FormRow label="Sample rate" description="at most — identical frames are never written">
          {#snippet icon()}<Gauge size={14} />{/snippet}
          <NumberStepper value={recorderStore.frameSampleFps} min={1} max={60} suffix="fps" size="sm" ariaLabel="Frames sampled per second" onchange={(v) => recorderStore.setFrameSampleFps(v)} />
        </FormRow>

        <FormRow label="Frame width" description="downscaling is the biggest lever on size">
          {#snippet icon()}<Scaling size={14} />{/snippet}
          <Select value={String(recorderStore.frameMaxWidth)} options={TYTO_FRAME_WIDTH_OPTIONS} onchange={(v) => recorderStore.setFrameMaxWidth(Number(v))} />
        </FormRow>
      {/if}
    </FormSection>

    <FormSection label="Screenshots" boxed>
      <FormRow label="Format" description="the image format stills are saved in">
        {#snippet icon()}<Camera size={14} />{/snippet}
        <RadioGroup appearance="segment" size="sm" nowrap value={recorderStore.screenshotFormat} options={TYTO_IMAGE_FORMAT_OPTIONS} onchange={(v) => recorderStore.setScreenshotFormat(v as ScreenshotFormat)} />
      </FormRow>

      <FormRow label="Copy to clipboard" description="copies the image right after it's saved">
        {#snippet icon()}<Clipboard size={14} />{/snippet}
        <Toggle checked={recorderStore.copyToClipboard} onchange={(v) => recorderStore.setCopyToClipboard(v)} ariaLabel="Copy screenshot to clipboard" />
      </FormRow>
    </FormSection>

    <!-- The reason this section exists: when the OS has refused, there is nothing to
         do inside Arbor, and this is where someone comes looking for the way out. -->
    <FormSection label="Permissions" boxed>
      <FormRow label="Screen recording" description={permissionDetail}>
        <Button variant={blocked ? 'primary' : 'secondary'} size="sm" onclick={openPrivacySettings}>
          {#snippet iconStart()}<ExternalLink size={13} />{/snippet}
          Open system settings
        </Button>
      </FormRow>
    </FormSection>

    <FormSection label="Opening shortcut" boxed>
      <FormRow label="Global shortcut" description="Open Tyto from anywhere — works even when Arbor isn't focused.">
        {#snippet icon()}<Keyboard size={14} />{/snippet}
        <Toggle checked={tytoConfigStore.globalShortcut} onchange={toggleShortcut} ariaLabel="Enable the global Tyto shortcut" />
      </FormRow>

      {#if tytoConfigStore.globalShortcut}
        <FormRow label="Shortcut" description="press a combination to rebind it">
          {#snippet icon()}<Keyboard size={14} />{/snippet}
          {#if capturing}
            <span class="capturing">Press a combination… <span class="dim">(Esc to cancel)</span></span>
          {:else}
            <Kbd label={tytoConfigStore.accelerator} />
            <Button variant="secondary" size="sm" onclick={() => (capturing = true)}>Rebind</Button>
          {/if}
        </FormRow>
      {/if}
    </FormSection>

    <!-- The shell's appearance settings, not Tyto's. They already govern this window — every
         product loads them on mount — and until now could only be changed from Corvus. The
         section header is the FormSection's, so the component does not draw its own. -->
    <FormSection label="Interface" boxed>
      <AppearanceSettings showHeader={false} onOpenThemeEditor={() => { themeEditorOpen = true; }} />
    </FormSection>

    <FormSection label="Animations" boxed>
      <AnimationsSettings showHeader={false} />
    </FormSection>
  </div>

  {#snippet footer()}
    <Button variant="primary" size="sm" onclick={onClose}>Done</Button>
  {/snippet}
</Modal>

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

{#if themeEditorOpen}
  <ThemeEditorModal onClose={() => (themeEditorOpen = false)} />
{/if}

<style>
  .body { display: flex; flex-direction: column; }
  .capturing { font-size: var(--font-size-sm); color: var(--accent); white-space: nowrap; }
  .capturing .dim { color: var(--text-muted); }
</style>
