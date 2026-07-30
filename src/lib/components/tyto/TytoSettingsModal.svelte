<script lang="ts">
  /**
   * TytoSettingsModal — Tyto's settings. Today it owns the one real,
   * shell-persisted setting: the opt-in OS-global shortcut that opens the
   * recorder window (rebindable, live-reconciled by the backend). Capture/output
   * defaults will join here once the recorder backend exists.
   */
  import { Keyboard, FolderOpen } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tytoConfigStore } from '$lib/stores/tyto/config.svelte';
  import { recorderStore, type CaptureMode, type ScreenshotFormat } from '$lib/stores/tyto/recorder.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let capturing = $state(false);
  let folderPickerOpen = $state(false);

  const modeOptions = [
    { value: 'record',     label: 'Record' },
    { value: 'screenshot', label: 'Screenshot' },
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

  async function toggleShortcut(on: boolean) {
    try {
      await tytoConfigStore.setGlobalShortcut(on);
    } catch (err) {
      uiStore.showToast(`Couldn't ${on ? 'enable' : 'disable'} the shortcut: ${err}`, 'error');
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

<Modal {onClose} width="620px" height="440px" ariaLabel="Tyto settings">
  {#snippet header()}
    <ModalHeader title="Tyto settings" {onClose} />
  {/snippet}

  <div class="body">
    <section>
      <h3 class="sec-title">General</h3>
      <div class="row gen">
        <div class="gen-label">Default mode</div>
        <RadioGroup appearance="segment" size="sm" value={recorderStore.mode} options={modeOptions} onchange={(v) => recorderStore.setMode(v as CaptureMode)} />
      </div>
      <div class="row gen">
        <div class="gen-label">Countdown <span class="gen-hint">before recording</span></div>
        <RadioGroup appearance="segment" size="sm" value={String(recorderStore.countdownSecs)} options={countdownOptions} onchange={(v) => recorderStore.setCountdownSecs(Number(v))} />
      </div>
      <div class="row gen">
        <div class="gen-label">Screenshot format</div>
        <RadioGroup appearance="segment" size="sm" value={recorderStore.screenshotFormat} options={formatOptions} onchange={(v) => recorderStore.setScreenshotFormat(v as ScreenshotFormat)} />
      </div>
      <div class="row gen">
        <div class="gen-label">
          Copy screenshot to clipboard
          <span class="gen-hint">copies the image right after it's saved</span>
        </div>
        <Toggle checked={recorderStore.copyToClipboard} onchange={(v) => recorderStore.setCopyToClipboard(v)} />
      </div>
      <div class="row gen">
        <div class="gen-label">
          Save to
          <span class="gen-hint path" title={recorderStore.outputDir}>{recorderStore.outputDir}</span>
        </div>
        <Button variant="secondary" size="sm" onclick={() => (folderPickerOpen = true)}><FolderOpen size={13} /> Change…</Button>
      </div>
    </section>

    <section>
      <h3 class="sec-title">Opening shortcut</h3>
      <div class="row">
        <Toggle
          checked={tytoConfigStore.globalShortcut}
          label="Global shortcut"
          description="Open Tyto from anywhere — works even when Arbor isn't focused."
          onchange={toggleShortcut}
        />
      </div>

      {#if tytoConfigStore.globalShortcut}
        <div class="row accel">
          <div class="accel-left">
            <Keyboard size={14} />
            <span>Shortcut</span>
          </div>
          <div class="accel-right">
            {#if capturing}
              <span class="capturing">Press a combination… <span class="dim">(Esc to cancel)</span></span>
            {:else}
              <Kbd label={tytoConfigStore.accelerator} />
              <Button variant="secondary" size="sm" onclick={() => (capturing = true)}>Rebind</Button>
            {/if}
          </div>
        </div>
      {/if}
    </section>
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

<style>
  .body { display: flex; flex-direction: column; gap: 20px; }
  section { display: flex; flex-direction: column; gap: 12px; }
  .sec-title {
    margin: 0; font-size: var(--font-size-xs); font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.7px; color: var(--text-muted);
  }
  .row { display: flex; align-items: center; }

  /* General settings row: label (+ optional hint/path) left, control right. */
  .gen { justify-content: space-between; gap: 16px; }
  .gen-label { font-size: var(--font-size-sm); color: var(--text-primary); min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .gen-hint { font-size: var(--font-size-2xs); color: var(--text-muted); font-weight: 400; }
  .gen-hint.path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 300px; }

  .accel { justify-content: space-between; gap: 12px; padding-left: 2px; }
  .accel-left { display: flex; align-items: center; gap: 8px; font-size: var(--font-size-sm); color: var(--text-primary); }
  .accel-left :global(svg) { color: var(--text-muted); }
  .accel-right { display: flex; align-items: center; gap: 10px; }
  .capturing { font-size: var(--font-size-sm); color: var(--accent); }
  .capturing .dim { color: var(--text-muted); }
</style>
