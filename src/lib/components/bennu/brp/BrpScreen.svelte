<script lang="ts">
  /**
   * The game's window as a picture. "Live" re-captures about once a second; with "Click through" on, a
   * click on the picture is a click in the game at the matching spot — the quickest way to poke a menu
   * without leaving the editor. Needs the `arbor-bevy-remote` plugin in the game.
   */
  import { onMount } from 'svelte';
  import { Camera } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { brpDriveStore as drive } from '$lib/stores/bennu/brp-drive.svelte';

  const WIDTHS = [
    { value: 640, label: '640 px' },
    { value: 900, label: '900 px' },
    { value: 1280, label: '1280 px' },
    { value: 0, label: 'Full size' },
  ];
  const LIVE_EVERY_MS = 1000;

  let width = $state('900');
  let live = $state(false);
  let clickThrough = $state(false);
  let img = $state<HTMLImageElement | undefined>();

  const src = $derived(drive.shot ? `data:${drive.shot.mime_type};base64,${drive.shot.data}` : null);

  function capture() {
    void drive.capture(Number(width) || undefined);
  }

  onMount(capture);

  $effect(() => {
    if (!live) return;
    const timer = setInterval(capture, LIVE_EVERY_MS);
    return () => clearInterval(timer);
  });

  async function onPicture(e: MouseEvent) {
    if (!clickThrough || !img) return;
    // The game's window size in logical pixels, from its own stats (cached after the first).
    if (!drive.stats?.window) await drive.pollStats();
    const win = drive.stats?.window;
    if (!win) return;
    const rect = img.getBoundingClientRect();
    const x = Math.round(((e.clientX - rect.left) / rect.width) * win.width);
    const y = Math.round(((e.clientY - rect.top) / rect.height) * win.height);
    await drive.send('mouse', { action: 'click', x, y });
    // The game needs a few frames to react before the picture shows it.
    setTimeout(capture, 250);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      capture();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="sc" role="region" aria-label="Game screen" onkeydown={onKey}>
  <PaneHeader title="Screen" loading={drive.shooting}>
    {#snippet end()}
      <Select value={width} options={WIDTHS} size="sm" onchange={(v) => { width = v; capture(); }} />
      <Toggle checked={live} size="sm" label="Live" onchange={(v) => (live = v)} />
      <Toggle checked={clickThrough} size="sm" label="Click through" onchange={(v) => (clickThrough = v)} />
      <Button variant="primary" size="sm" onclick={capture}>
        {#snippet iconStart()}<Camera size={12} />{/snippet}
        Capture
      </Button>
    {/snippet}
  </PaneHeader>
  <div class="sc-body">
    {#if drive.unavailable}
      <Alert variant="warning" compact text={drive.unavailable} />
    {/if}
    {#if src}
      <div class="sc-frame">
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
        <img bind:this={img} class="sc-img" class:click={clickThrough} {src} alt="The running game's window" onclick={onPicture} />
      </div>
      <div class="sc-meta">{drive.shot?.width}×{drive.shot?.height}</div>
    {:else if !drive.unavailable}
      <EmptyState message="No picture yet" description="Capture takes a screenshot of the game's window. Ctrl+Enter captures from anywhere in this view." />
    {/if}
  </div>
</div>

<style>
  .sc { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .sc-body { flex: 1; min-height: 0; overflow: auto; padding: 8px 10px; display: flex; flex-direction: column; gap: 6px; }
  .sc-frame { display: flex; justify-content: center; }
  .sc-img { max-width: 100%; height: auto; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-overlay); }
  .sc-img.click { cursor: crosshair; }
  .sc-meta { font-size: var(--font-size-2xs); color: var(--text-muted); text-align: center; font-variant-numeric: tabular-nums; }
</style>
