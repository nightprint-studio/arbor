<script lang="ts">
  /**
   * VoiceModelPicker — the guided choice of a speech model.
   *
   * Every model is a card that says what it costs (disk), how it will feel on THIS machine
   * (seconds of work per second of speech — estimated from the detected physical cores / RAM / GPU,
   * replaced by the real figure once "Test it" or a dictation has measured it), how well it
   * understands (accuracy) and which languages it covers. One card carries a "Recommended for your
   * PC" badge whose tooltip says why; the badge only goes to a model that keeps up with speech, and
   * moves when a measurement contradicts the estimate. "In use" and the "no model" notice both come
   * from the backend's single answer (`list.selected`), so they cannot disagree.
   *
   * "Test it" records a few seconds with that model, shows its phase (recording / transcribing) and
   * a Stop button, and ends with the measured speed. Downloads go through a consent dialog, report
   * progress, are verified against the checksum pinned in the app, and the first one installed
   * becomes the model in use.
   */
  import { onMount } from 'svelte';
  import { AudioLines, Download, Trash2, FlaskConical, Check, FolderOpen, Square, Zap } from 'lucide-svelte';
  import IconCard from '$lib/components/shared/ui/IconCard.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import DownloadProgress from '$lib/components/shared/ui/DownloadProgress.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { voiceApi, type VoiceModel, type VoiceModelList } from '$lib/ipc/voice';
  import { voiceStore } from '$lib/stores/voice.svelte';
  import { formatBytes } from '$lib/utils/format';
  import { accuracyLabel, rtfLabel, speedLabel, testVerdict } from '$lib/utils/voice-format';

  let { language }: { language: string } = $props();

  let list = $state<VoiceModelList | null>(null);
  let loadError = $state<string | null>(null);
  let consent = $state<VoiceModel | null>(null);
  let picking = $state(false);
  let showAll = $state(false);
  let now = $state(Date.now());

  async function refresh() {
    try { list = await voiceApi.models(language); loadError = null; }
    catch (e) { loadError = e instanceof Error ? e.message : String(e); }
  }

  onMount(() => { void refresh(); });
  // A finished download, a selection, a measurement or a language change changes the answer.
  $effect(() => { void voiceStore.installedTick; void language; void refresh(); });

  // Ticks the elapsed time of a running test.
  $effect(() => {
    if (!voiceStore.test) return;
    const id = setInterval(() => { now = Date.now(); }, 250);
    return () => clearInterval(id);
  });

  const hw = $derived(list?.hardware ?? { cores: 4, logical: 8, ram_gb: 8, gpu: false });
  const selectedId = $derived(list?.custom ? '' : (list?.selected ?? ''));
  const test = $derived(voiceStore.test);
  const result = $derived(voiceStore.testResult);
  /** Fewer rows by default: the recommendation, what is installed, and the usual steps up/down. */
  const visible = $derived.by(() => {
    const all = (list?.models ?? []).filter((m) => language === 'en' || language === 'auto' || !m.english_only);
    if (showAll) return all;
    return all.filter((m) => m.installed || m.recommended || m.id === selectedId || (!m.quantized && m.accuracy >= 2 && m.rtf <= 2));
  });

  function languages(m: VoiceModel): string {
    return m.english_only ? 'English only' : 'Multilingual';
  }

  async function install(m: VoiceModel) {
    consent = null;
    try { await voiceApi.installModel(m.id); } catch (e) { loadError = e instanceof Error ? e.message : String(e); }
    await refresh();
  }

  async function remove(m: VoiceModel) {
    try { await voiceApi.removeModel(m.id); } catch (e) { loadError = String(e); }
    await voiceStore.loadConfig();
    await refresh();
  }

  async function use(m: VoiceModel) {
    await voiceStore.selectModel(m.id);
    await refresh();
  }

  function testLabel(m: VoiceModel): string {
    if (test?.modelId !== m.id) return 'Test it';
    if (test.phase === 'transcribing') return 'Transcribing…';
    const left = Math.max(0, test.seconds - (now - test.startedAt) / 1000);
    return `Speak now… ${Math.ceil(left)} s`;
  }
</script>

{#if loadError}<Alert variant="error" text={loadError} dismissible onclose={() => (loadError = null)} />{/if}
{#if list && !list.engine}
  <Alert variant="warning" title="Speech engine not included" text="This build of Arbor was made without the speech engine, so models can be managed but nothing can be transcribed." />
{/if}
{#if list && list.engine && !list.custom && !list.selected}
  <Alert variant="info" compact text="No model is in use yet. Download one, or pick an installed model with Use." />
{/if}

<div class="head">
  <span class="hw">Detected: {hw.cores} cores{hw.logical > hw.cores ? ` (${hw.logical} threads)` : ''} · {hw.ram_gb.toFixed(0)} GB RAM{hw.gpu ? ' · GPU acceleration' : ' · no GPU acceleration'}</span>
  <Button variant="ghost" size="xs" onclick={() => (showAll = !showAll)}>{showAll ? 'Show fewer' : 'Show all models'}</Button>
</div>

<div class="models" role="radiogroup" aria-label="Speech model">
  {#each visible as m (m.id)}
    {@const sel = selectedId === m.id}
    {@const dl = voiceStore.installs[m.id]}
    {@const mine = test?.modelId === m.id}
    <IconCard
      title={m.label}
      layout="row"
      size="sm"
      tone={sel ? 'accent' : 'default'}
      description={`${formatBytes(m.size_bytes)} · Speed: ${speedLabel(m.rtf)} · Accuracy: ${accuracyLabel(m.accuracy)} · ${languages(m)}`}
    >
      {#snippet icon()}<AudioLines size={18} />{/snippet}
      {#snippet titleExtra()}
        {#if m.recommended}<Badge variant="tone" tone="success" size="sm" label="Recommended for your PC" tooltip={list?.recommendation.reason} />{/if}
        {#if sel}<Badge variant="tone" tone="accent" size="sm" label="In use" />{/if}
        {#if m.live_ok}
          <Badge variant="tone" tone="info" size="sm" tooltip="Fast enough on this PC to show text while you are still speaking.">
            {#snippet icon()}<Zap size={9} />{/snippet}
            Live text
          </Badge>
        {/if}
      {/snippet}
      {#snippet trailing()}
        <div class="actions">
          {#if m.installed}
            {#if !sel}<Button size="xs" variant="secondary" disabled={!!test} onclick={() => use(m)}>Use</Button>{/if}
            {#if mine}
              <Button size="xs" variant="danger" onclick={() => voiceStore.stopTest()}>
                {#snippet iconStart()}<Square size={11} />{/snippet}
                Stop
              </Button>
            {/if}
            <Button size="xs" variant="ghost" disabled={(!!test && !mine) || (voiceStore.busy && !mine) || mine} onclick={() => voiceStore.runTest(m.id, 5)}>
              {#snippet iconStart()}{#if mine}<Spinner size="xs" />{:else}<FlaskConical size={12} />{/if}{/snippet}
              {testLabel(m)}
            </Button>
            <Button size="xs" variant="ghost" disabled={!!test} ariaLabel="Remove {m.label}" onclick={() => remove(m)}>
              {#snippet iconStart()}<Trash2 size={12} />{/snippet}
            </Button>
          {:else if m.installing || dl}
            <span class="dl-state"><Spinner size="xs" /> Downloading…</span>
          {:else}
            <Button size="xs" variant="secondary" onclick={() => (consent = m)}>
              {#snippet iconStart()}<Download size={12} />{/snippet}
              Download
            </Button>
          {/if}
        </div>
      {/snippet}
      {#snippet extra()}
        {#if dl?.error}<Alert variant="error" compact text={dl.error} />{/if}
        {#if !m.installed && !dl?.error && (m.installing || dl)}
          <DownloadProgress stats={dl ?? null} ariaLabel="Downloading {m.label}" />
        {/if}
        {#if m.installed}<span class="speed">{rtfLabel(m.rtf, m.rtf_measured)}</span>{/if}
        {#if result?.modelId === m.id && !test}
          {#if result.transcript.error}
            <Alert variant="error" compact text={result.transcript.error} />
          {:else if result.transcript.silent}
            <Alert variant="warning" compact text="No speech was heard. Check the microphone in the settings below, then try again." />
          {:else}
            <Alert variant="success" compact>
              <strong>{result.transcript.text || '(nothing recognised)'}</strong><br />
              {testVerdict(result.transcript.audio_secs, result.transcript.took_secs, result.transcript.comfort)}
              Took {result.transcript.took_secs.toFixed(1)} s for {result.transcript.audio_secs.toFixed(1)} s of speech.
            </Alert>
          {/if}
        {/if}
      {/snippet}
    </IconCard>
  {/each}
</div>

<div class="foot">
  <Button size="xs" variant="ghost" onclick={() => (picking = true)}>
    {#snippet iconStart()}<FolderOpen size={12} />{/snippet}
    Use my own model file…
  </Button>
  {#if voiceStore.config?.custom_model_path}<span class="own"><Check size={12} /> {voiceStore.config.custom_model_path}</span>{/if}
</div>

{#if consent}
  <ConfirmModal
    title="Download {consent.label}?"
    message="{consent.label} ({formatBytes(consent.size_bytes)}) will be downloaded from the official whisper.cpp model repository on Hugging Face and checked against its published SHA-256 before use."
    detail="Source: {consent.url} · Free disk space: {list?.free_disk_bytes != null ? formatBytes(list.free_disk_bytes) : 'unknown'} · Saved in: {list?.models_dir}"
    confirmLabel="Download"
    onConfirm={() => install(consent!)}
    onCancel={() => (consent = null)}
  />
{/if}

{#if picking}
  <FileExplorerModal
    mode="file"
    extensions={['bin', 'gguf']}
    title="Choose a Whisper ggml model file"
    onConfirm={async (path) => { picking = false; await voiceStore.saveConfig({ custom_model_path: path }); await refresh(); }}
    onCancel={() => (picking = false)}
  />
{/if}

<style>
  .head { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin: 4px 0 8px; }
  .hw { font-size: var(--font-size-xs); color: var(--text-muted); }
  .models { display: flex; flex-direction: column; gap: 6px; }
  .actions { display: flex; align-items: center; gap: 4px; }
  .dl-state { display: inline-flex; align-items: center; gap: 6px; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .speed { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .foot { display: flex; align-items: center; gap: 8px; margin-top: 8px; }
  .own { display: inline-flex; align-items: center; gap: 4px; font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
