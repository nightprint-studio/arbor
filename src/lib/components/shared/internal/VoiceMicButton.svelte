<script lang="ts">
  /**
   * VoiceMicButton — the microphone control: hold to talk, tap to start and tap again to stop.
   *
   * One gesture for both habits. A press that lasts at least a moment is push-to-talk (the
   * recording ends on release); a quick tap leaves the microphone open until the next tap. From
   * the keyboard, Enter / Space toggles. The transcript is handed to `onText` when the engine is
   * done; while it works the button shows a spinner, and Esc (or a click on the spinner) cancels a recording or a transcription in progress.
   */
  import { Mic, MicOff, Square } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { voiceStore } from '$lib/stores/voice.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { onMount } from 'svelte';

  let {
    onText,
    size = 24,
    shortcut,
    onSetup,
  }: {
    /** The recognised text, ready to insert. */
    onText: (text: string) => void;
    size?: number;
    /** Shown in the tooltip (the push-to-talk chord). */
    shortcut?: string;
    /** Where a press goes when dictation cannot work yet (no speech model, no engine): the voice setup. */
    onSetup?: () => void;
  } = $props();

  /** A press shorter than this is a tap (toggle); longer is push-to-talk. */
  const HOLD_MS = 350;
  let downAt = 0;
  let startedByPress = false;

  /** How much of the dictation's text has already been inserted as live pieces. */
  let insertedLive = '';

  // Live pieces: each announcement carries everything heard so far; only the new part is inserted.
  onMount(() => voiceStore.onPartial((p, target) => {
    if (target !== 'composer') return;
    const fresh = p.text.startsWith(insertedLive) ? p.text.slice(insertedLive.length).trim() : p.text.trim();
    if (!fresh) return;
    onText(insertedLive ? ` ${fresh}` : fresh);
    insertedLive = p.text;
  }));

  onMount(() => voiceStore.onTranscript((t, target) => {
    if (target !== 'composer') return;
    const alreadyShown = insertedLive;
    insertedLive = '';
    if (t.error) { if (t.error !== 'cancelled') toastStore.show(t.error, 'error'); return; }
    if (t.silent) { if (!alreadyShown) toastStore.show('No speech was heard.', 'info'); return; }
    // Text that went out live is not inserted twice: only what was not announced yet.
    const rest = t.streamed ? t.tail : t.text;
    if (rest) onText(alreadyShown && t.streamed ? ` ${rest}` : rest);
  }));

  // A new dictation starts from nothing inserted.
  $effect(() => { if (voiceStore.listening) insertedLive = ''; });

  // Whether dictation can work at all. Unknown until voice-be has answered: until then the button is
  // optimistic, and the first answer (or a model finishing its install) decides how it looks.
  const problem = $derived.by<string | null>(() => {
    const s = voiceStore.status;
    if (!s) return null;
    if (!s.engine) return 'This build has no speech engine';
    if (!s.model_ready) return 'No speech model installed';
    return null;
  });

  onMount(() => { if (!voiceStore.status) void voiceStore.loadConfig(); });
  $effect(() => {
    void voiceStore.installedTick;
    if (voiceStore.installedTick > 0) void voiceStore.loadConfig();
  });

  // Esc cancels while recording — but only if it is ours (composer target).
  $effect(() => {
    if (!(voiceStore.listening || voiceStore.transcribing) || voiceStore.target !== 'composer') return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      void voiceStore.cancel();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });

  /** Dictation cannot start: take the user to where it can be set up, instead of failing silently. */
  function setup(): boolean {
    if (!problem) return false;
    toastStore.show(`${problem} — opening the voice setup.`, 'info');
    onSetup?.();
    return true;
  }

  async function onDown(e: PointerEvent) {
    if (e.button !== 0) return;
    if (voiceStore.transcribing) { void voiceStore.cancel(); return; } // a click on the spinner stops it
    if (!voiceStore.listening && setup()) { startedByPress = false; return; }
    downAt = performance.now();
    if (voiceStore.listening) { startedByPress = false; return; } // a second tap stops it, on release
    startedByPress = await voiceStore.startListening('push_to_talk', 'composer');
  }

  async function onUp() {
    if (voiceStore.transcribing) return;
    const held = performance.now() - downAt;
    if (!startedByPress) {
      if (voiceStore.listening) await voiceStore.stopListening(); // the stopping tap
      return;
    }
    startedByPress = false;
    if (held >= HOLD_MS) await voiceStore.stopListening(); // push-to-talk released
    // else: a tap — the microphone stays open until the next tap.
  }

  async function onKeyboardClick(e: MouseEvent) {
    if (e.detail !== 0) return; // a real click is handled by the pointer events
    if (voiceStore.transcribing) { void voiceStore.cancel(); return; }
    if (!voiceStore.listening && setup()) return;
    await voiceStore.toggle('composer');
  }

  const tip = $derived(
    problem && !voiceStore.listening ? `${problem} — click to set it up`
      : voiceStore.transcribing ? 'Transcribing… click or press Esc to stop'
      : voiceStore.listening ? (voiceStore.live ? 'Text appears as you speak · release to finish · Esc cancels' : 'Release to finish, or tap to stop · Esc cancels')
      : 'Hold to talk, tap to toggle',
  );
</script>

<span class="mic" class:live={voiceStore.listening} class:needs-setup={!!problem && !voiceStore.listening} role="presentation" onpointerdown={onDown} onpointerup={onUp}>
  <IconButton tooltip={tip} {shortcut} {size} variant={voiceStore.listening ? 'accent' : 'ghost'} active={voiceStore.listening} onclick={onKeyboardClick}>
    {#if voiceStore.transcribing}<Spinner size="xs" />{:else if voiceStore.listening}<Square size={11} />{:else if problem}<MicOff size={14} />{:else}<Mic size={14} />{/if}
  </IconButton>
  {#if voiceStore.listening && voiceStore.target === 'composer'}
    <i class="halo" style="transform: scale({1 + voiceStore.level * 0.9})"></i>
  {/if}
</span>

<style>
  .mic { position: relative; display: inline-flex; touch-action: none; }
  /* Not usable yet: dimmed and struck through, still clickable — a press takes the user to the setup. */
  .mic.needs-setup { opacity: 0.55; }
  .halo {
    position: absolute; inset: 0; border-radius: 50%; pointer-events: none;
    box-shadow: 0 0 0 2px var(--accent-subtle); transition: transform 70ms linear;
  }
</style>
