<script lang="ts">
  /**
   * TytoSettingsModal — Tyto's settings. Today it owns the one real,
   * shell-persisted setting: the opt-in OS-global shortcut that opens the
   * recorder window (rebindable, live-reconciled by the backend). Capture/output
   * defaults will join here once the recorder backend exists.
   */
  import { Keyboard, Info } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { tytoConfigStore } from '$lib/stores/tyto/config.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let capturing = $state(false);

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

    <div class="note">
      <Info size={15} />
      <div>
        <strong>The recorder backend is still in progress.</strong>
        The capture UI here is a preview: recordings and screenshots are simulated.
        Source, audio, quality and output defaults will become persistent settings
        once the backend lands.
      </div>
    </div>
  </div>

  {#snippet footer()}
    <Button variant="primary" size="sm" onclick={onClose}>Done</Button>
  {/snippet}
</Modal>

<style>
  .body { display: flex; flex-direction: column; gap: 20px; }
  section { display: flex; flex-direction: column; gap: 12px; }
  .sec-title {
    margin: 0; font-size: 11px; font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.7px; color: var(--text-muted);
  }
  .row { display: flex; align-items: center; }

  .accel { justify-content: space-between; gap: 12px; padding-left: 2px; }
  .accel-left { display: flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--text-primary); }
  .accel-left :global(svg) { color: var(--text-muted); }
  .accel-right { display: flex; align-items: center; gap: 10px; }
  .capturing { font-size: 12px; color: var(--accent); }
  .capturing .dim { color: var(--text-muted); }

  .note {
    display: flex; gap: 10px;
    padding: 12px 14px;
    background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: var(--radius-md);
    font-size: 12px; line-height: 1.5; color: var(--text-secondary);
  }
  .note :global(svg) { flex-shrink: 0; margin-top: 1px; color: var(--accent); }
  .note strong { color: var(--text-primary); font-weight: 600; }
</style>
