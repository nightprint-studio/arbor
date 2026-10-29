<script lang="ts">
  /**
   * VoiceArmedIndicator — the always-visible "the microphone is open" mark for the status strip.
   *
   * It exists only while the wake word is armed (and while a recording is in progress), so a
   * listening microphone can never be a surprise. One click disarms; the tooltip says how long
   * until it does so by itself is configured in Settings › Voice.
   */
  import { Mic } from 'lucide-svelte';
  import StatusBarItem from '$lib/components/shared/ui/StatusBarItem.svelte';
  import { voiceStore } from '$lib/stores/voice.svelte';

  const label = $derived(voiceStore.listening ? 'Listening…' : voiceStore.transcribing ? 'Transcribing…' : 'Mic on');
</script>

{#if voiceStore.armed || voiceStore.listening}
  <StatusBarItem
    tone="warning"
    tooltip={voiceStore.armed ? 'The wake word is armed - the microphone is open. Click to turn it off.' : 'Recording'}
    ariaLabel="Microphone is open. Disarm the wake word."
    onclick={() => { if (voiceStore.armed) void voiceStore.disarm(); }}
  >
    <span class="rec"><Mic size={12} /></span>{label}
  </StatusBarItem>
{/if}

<style>
  .rec { display: inline-flex; margin-right: 4px; animation: pulse 1.6s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.35; } }
  @media (prefers-reduced-motion: reduce) { .rec { animation: none; } }
</style>
