<script lang="ts">
  /**
   * GlobalShortcutCapture — records an OS-global accelerator chord and emits it
   * as a Tauri accelerator string (e.g. "Ctrl+Shift+E"). Click to record, press
   * a combo, Esc to cancel. A bare key (no modifier) is rejected unless it's a
   * function key, since global hotkeys without a modifier are hostile. The host
   * owns persistence/registration via `onChange` (which may reject — the widget
   * just reports the chord).
   */
  import { Pencil } from 'lucide-svelte';
  import Kbd from './Kbd.svelte';

  let { accel, disabled = false, onChange }: {
    accel: string;
    disabled?: boolean;
    onChange: (accel: string) => void | Promise<void>;
  } = $props();

  let recording = $state(false);
  let hint = $state('');
  let handler: ((e: KeyboardEvent) => void) | null = null;

  const parts = $derived(accel ? accel.split('+').map(s => s.trim()).filter(Boolean) : []);

  function start() {
    if (disabled || recording) return;
    recording = true;
    hint = '';
    handler = capture;
    window.addEventListener('keydown', handler, { capture: true });
  }
  function stop() {
    if (handler) { window.removeEventListener('keydown', handler, true); handler = null; }
    recording = false;
  }

  function capture(e: KeyboardEvent) {
    if (['Control', 'Shift', 'Alt', 'Meta', 'CapsLock'].includes(e.key)) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    if (e.key === 'Escape') { stop(); return; }

    const mods: string[] = [];
    if (e.ctrlKey)  mods.push('Ctrl');
    if (e.shiftKey) mods.push('Shift');
    if (e.altKey)   mods.push('Alt');
    if (e.metaKey)  mods.push('Super');

    let key = e.key;
    if (key.length === 1) key = key.toUpperCase();
    const isFn = /^F\d{1,2}$/.test(key);

    if (mods.length === 0 && !isFn) {
      hint = 'Use at least one modifier (Ctrl / Alt / Shift / Super).';
      return; // keep recording
    }

    const next = [...mods, key].join('+');
    stop();
    void Promise.resolve(onChange(next)).catch(() => {});
  }

  $effect(() => () => stop());
</script>

<button
  type="button"
  class="gsc"
  class:recording
  {disabled}
  onclick={() => recording ? stop() : start()}
  aria-label={recording ? 'Recording shortcut — press a combination' : `Rebind shortcut (currently ${accel})`}
>
  {#if recording}
    <span class="gsc-rec">Press a shortcut…</span>
  {:else}
    {#if parts.length}<Kbd keys={parts} size="sm" />{:else}<span class="gsc-none">unset</span>{/if}
    <Pencil size={11} class="gsc-pencil" />
  {/if}
</button>
{#if recording && hint}<span class="gsc-hint">{hint}</span>{/if}

<style>
  .gsc {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-primary);
    transition: border-color var(--transition-fast), color var(--transition-fast), background var(--transition-fast);
  }
  .gsc:hover:not(:disabled):not(.recording) { border-color: var(--accent); }
  .gsc:disabled { opacity: 0.5; cursor: not-allowed; }
  .gsc.recording { border-color: var(--accent); background: var(--accent-subtle); cursor: default; }
  .gsc :global(.gsc-pencil) { color: var(--text-muted); flex-shrink: 0; }
  .gsc-rec {
    font-size: var(--font-size-xs); font-style: italic; color: var(--accent);
    animation: gsc-pulse 1.1s ease-in-out infinite;
  }
  .gsc-none { font-size: var(--font-size-xs); color: var(--text-muted); font-style: italic; }
  .gsc-hint { margin-left: 8px; font-size: var(--font-size-xs); color: var(--warning); }
  @keyframes gsc-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.55; } }
</style>
