<script lang="ts">
  /**
   * VoiceSettings — Settings › Voice: the model, the language, the microphone, the vocabulary
   * hints, how long the engine stays loaded, the overlay, and the wake word.
   *
   * Everything is saved to `voice-be`'s config file in the profile directory as soon as it
   * changes (no Save button). Opening this page starts the voice backend if it is not running.
   */
  import { onMount } from 'svelte';
  import { Mic, Ear, Keyboard } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import LevelMeter from '$lib/components/shared/ui/LevelMeter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import VoiceModelPicker from './VoiceModelPicker.svelte';
  import { voiceApi, type VoiceDevice } from '$lib/ipc/voice';
  import { voiceStore } from '$lib/stores/voice.svelte';

  const LANGUAGES = [
    { value: 'auto', label: 'Detect automatically' },
    { value: 'it', label: 'Italiano' }, { value: 'en', label: 'English' },
    { value: 'de', label: 'Deutsch' }, { value: 'fr', label: 'Français' },
    { value: 'es', label: 'Español' }, { value: 'pt', label: 'Português' },
    { value: 'nl', label: 'Nederlands' }, { value: 'pl', label: 'Polski' },
    { value: 'ru', label: 'Русский' }, { value: 'ja', label: '日本語' }, { value: 'zh', label: '中文' },
  ];

  let devices = $state<VoiceDevice[]>([]);
  let micTest = $state(false);
  const cfg = $derived(voiceStore.config);
  const micOptions = $derived([
    { value: '', label: 'System default' },
    ...devices.map((d) => ({ value: d.name, label: d.default ? `${d.name} (default)` : d.name })),
  ]);
  const phrases = $derived((cfg?.wake.phrases ?? []).join(', '));

  onMount(async () => {
    await voiceStore.loadConfig();
    try { devices = await voiceApi.devices(); } catch (e) { voiceStore.clearError(); }
  });

  /** Listen for a few seconds just to watch the level meter move — nothing is transcribed. */
  async function testMicrophone() {
    micTest = true;
    if (await voiceStore.startListening('test', 'composer')) {
      await new Promise((r) => setTimeout(r, 4000));
      await voiceStore.cancel();
    }
    micTest = false;
  }

  async function setWake(on: boolean) {
    if (!cfg) return;
    await voiceStore.saveConfig({ wake: { ...cfg.wake, enabled: on } });
    if (on) await voiceStore.arm(); else await voiceStore.disarm();
  }

  async function setPhrases(text: string) {
    if (!cfg) return;
    const list = text.split(',').map((p) => p.trim()).filter(Boolean);
    await voiceStore.saveConfig({ wake: { ...cfg.wake, phrases: list.length ? list : cfg.wake.phrases } });
  }
</script>

<SectionHeader title="Voice" description="Dictate with a speech model that runs on this computer. Audio is never recorded to disk and never leaves your machine." />

{#if voiceStore.error}<Alert variant="error" text={voiceStore.error} dismissible onclose={() => voiceStore.clearError()} />{/if}

{#if cfg}
  <h3 class="sub">Speech model</h3>
  <VoiceModelPicker language={cfg.language} />

  <h3 class="sub">Recognition</h3>
  <FormRow label="Language" description="Italian and other non-English languages work best with Small or larger.">
    <Select value={cfg.language} options={LANGUAGES} onchange={(v) => voiceStore.saveConfig({ language: String(v) })} />
  </FormRow>
  <FormRow label="Microphone" description="If the chosen device is unplugged, the system default is used.">
    <Select value={cfg.microphone} options={micOptions} onchange={(v) => voiceStore.saveConfig({ microphone: String(v) })} />
    <Button size="xs" variant="ghost" disabled={micTest || voiceStore.busy} onclick={testMicrophone}>
      {#snippet iconStart()}<Mic size={12} />{/snippet}
      {micTest ? 'Listening…' : 'Test microphone'}
    </Button>
    <LevelMeter level={voiceStore.level} active={micTest} />
  </FormRow>
  <FormRow label="Live text while you speak" description="A pause of about half a second finishes a piece, which is transcribed in the background and appears while you keep talking. Models that are too slow for this PC wait for you to finish instead.">
    <Toggle checked={cfg.live_text} onchange={(v) => voiceStore.saveConfig({ live_text: v })} />
  </FormRow>
  <FormRow label="Vocabulary hints"description="Tell the recogniser the project, class and symbol names you are working with, so code words come out spelled right.">
    <Toggle checked={cfg.vocabulary_hints} onchange={(v) => voiceStore.saveConfig({ vocabulary_hints: v })} />
  </FormRow>
  <FormRow label="Voice overlay" description="Dims the window and shows Imber, what you say and its answer when you use the push-to-talk shortcut or the wake word.">
    <Toggle checked={cfg.overlay} onchange={(v) => voiceStore.saveConfig({ overlay: v })} />
  </FormRow>
  <FormRow label="Free memory when idle" description="Unload the model and stop the voice process after this many minutes without use (it starts again on demand).">
    <NumberStepper value={cfg.idle_minutes} min={1} max={240} onchange={(v) => voiceStore.saveConfig({ idle_minutes: Number(v) })} />
  </FormRow>

  <h3 class="sub"><Keyboard size={14} /> Shortcuts</h3>
  <FormRow label="Talk shortcut" description="Hold to speak and release to send, or tap once to start and again to stop. Change it in Settings › Keyboard shortcuts."><Kbd action="voice_push_to_talk" /></FormRow>

  <h3 class="sub"><Ear size={14} /> Wake word</h3>
  <Alert variant="warning" title="Keeps the microphone open and uses CPU" text="While armed, Arbor listens continuously for the phrase (a cheap loudness check, and a small speech model only when someone speaks). Nothing is recorded to disk or sent anywhere. It is off by default and never re-arms by itself after a restart." />
  <FormRow label="Listen for the wake word" description="Say “Ehi Imber” or “Hey Imber”, then your request; a pause ends it.">
    <Toggle checked={voiceStore.armed} onchange={setWake} />
  </FormRow>
  <FormRow label="Phrases" description="Comma-separated. Similar-sounding variants are accepted." wideControl>
    <Input value={phrases} onchange={setPhrases} />
  </FormRow>
  <FormRow label="Also when the window is in the background" description="Off: the microphone is released whenever Arbor is not the focused application.">
    <Toggle checked={cfg.wake.in_background} onchange={(v) => voiceStore.saveConfig({ wake: { ...cfg.wake, in_background: v } })} />
  </FormRow>
  <FormRow label="Disarm after (minutes)" description="Automatically stop listening after this long without a wake. 0 keeps it armed.">
    <NumberStepper value={cfg.wake.auto_disarm_minutes} min={0} max={480} onchange={(v) => voiceStore.saveConfig({ wake: { ...cfg.wake, auto_disarm_minutes: Number(v) } })} />
  </FormRow>
  <FormRow label="Silence that ends a request (ms)" description="How long a pause means you are done speaking.">
    <NumberStepper value={cfg.wake.silence_ms} min={400} max={5000} step={100} onchange={(v) => voiceStore.saveConfig({ wake: { ...cfg.wake, silence_ms: Number(v) } })} />
  </FormRow>
{/if}

<style>
  .sub { display: flex; align-items: center; gap: 6px; margin: 22px 0 8px; font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
</style>
