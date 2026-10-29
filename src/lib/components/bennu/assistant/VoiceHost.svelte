<script lang="ts">
  /**
   * VoiceHost — mounted once in Bennu's window: the push-to-talk / toggle shortcuts and the voice
   * overlay. No markup of its own beyond the overlay, and nothing talks to the voice backend until
   * a shortcut, a button or the wake word asks.
   *
   * Push-to-talk needs key-down AND key-up, which the window's shortcut map (down only) cannot
   * express, so this listens itself, but through `keybindingsStore`, so the chord is the one the
   * user configured. Releasing the key, or a modifier that was part of the chord, ends the
   * recording. Auto-repeat is ignored.
   */
  import { onMount } from 'svelte';
  import VoiceOverlay from './VoiceOverlay.svelte';
  import { bennuVoiceStore as voice } from '$lib/stores/bennu/voice.svelte';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { matchesBinding, isKey } from '$lib/utils/keybindings';

  /** A press shorter than this is a tap: the microphone stays open until the next tap (like the mic button). */
  const TAP_MS = 350;
  let holding = false;
  let downAt = 0;
  /** A tap opened the microphone; the next press of the chord closes it. */
  let openByTap = false;

  function onKeydown(e: KeyboardEvent) {
    if (!matchesBinding(e, keybindingsStore.getBinding('voice_push_to_talk'))) return;
    e.preventDefault();
    if (e.repeat || holding) return;
    if (openByTap) { openByTap = false; void voice.end(); return; } // the second tap stops
    holding = true;
    downAt = performance.now();
    void voice.begin('push_to_talk');
  }

  function onKeyup(e: KeyboardEvent) {
    if (!holding) return;
    const b = keybindingsStore.getBinding('voice_push_to_talk');
    const modifierReleased =
      (b.ctrl && (e.key === 'Control' || e.key === 'Meta')) || (b.shift && e.key === 'Shift') || (b.alt && e.key === 'Alt');
    if (!(isKey(e, b.key) || modifierReleased)) return;
    holding = false;
    if (performance.now() - downAt < TAP_MS) openByTap = true; // a tap: leave it listening
    else void voice.end(); // a hold: released, so send
  }

  // Losing focus while the key is down would never deliver the key-up.
  function onBlur() { if (holding) { holding = false; void voice.end(); } }

  onMount(() => {
    voice.wire();
    window.addEventListener('keydown', onKeydown, true);
    window.addEventListener('keyup', onKeyup, true);
    window.addEventListener('blur', onBlur);
    return () => {
      voice.unwire();
      window.removeEventListener('keydown', onKeydown, true);
      window.removeEventListener('keyup', onKeyup, true);
      window.removeEventListener('blur', onBlur);
    };
  });
</script>

{#if voice.overlayOpen}
  <VoiceOverlay
    request={voice.request}
    problem={voice.problem}
    onClose={() => voice.closeOverlay()}
    onSend={() => void voice.end()}
    onRetry={() => void voice.retry()}
    onSettings={() => voice.openVoiceSettings()}
    onContinue={() => voice.continueInPanel()}
    onFollowUp={(t) => voice.followUp(t)}
  />
{/if}
