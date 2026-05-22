<!--
  Lazy — defers a heavy component's module evaluation until the gate first
  flips true. Once loaded the module stays cached, so subsequent open/close
  cycles incur no further import cost.

  Used in AppShell to keep big panels (DocsPanel, SettingsPanel) and Studio
  modals out of the initial V8 heap. Dev warmup pre-fires the same loaders
  in onMount so hot paths feel instant during local development; the `dev`
  branch is dead-code-eliminated by Rollup in release builds.

  Loading affordance: when the gate flips true and the module hasn't
  resolved yet (cold first open — Vite has to fetch + evaluate dozens of
  sub-components), a minimal backdrop + spinner shell is rendered after a
  short delay so the click is acknowledged immediately instead of feeling
  dropped. The delay avoids flashing the shell on subsequent opens, where
  the module is cached in V8 and the import resolves in the same microtask.

  Loose typing: each call site already passes a known loader + matching
  props, so a strict generic here would force every caller to annotate.
  The `any` shape on `Comp` keeps the spread `<C {...rest} />` callable
  whether the loaded component takes no props (Studio modals) or several
  (SettingsPanel, DocsPanel).
-->
<script lang="ts">
  import Spinner from './ui/Spinner.svelte';

  type Props = {
    gate?: boolean;
    loader: () => Promise<{ default: any }>;
    [key: string]: any;
  };

  let { gate = true, loader, ...rest }: Props = $props();

  let Comp: any = $state(null);
  let started = false;
  let showFallback = $state(false);
  let fallbackTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    if (gate && !started) {
      started = true;
      fallbackTimer = setTimeout(() => { showFallback = true; }, 120);
      loader().then((m) => {
        Comp = m.default;
      }).catch((err) => {
        started = false;
        console.error('Lazy: failed to load component', err);
      }).finally(() => {
        if (fallbackTimer) { clearTimeout(fallbackTimer); fallbackTimer = null; }
        showFallback = false;
      });
    }
  });
</script>

{#if Comp && gate}
  {@const C = Comp}
  <C {...rest} />
{:else if gate && showFallback}
  <div class="lazy-loading-backdrop" role="status" aria-label="Loading">
    <div class="lazy-loading-card">
      <Spinner size="lg" />
    </div>
  </div>
{/if}

<style>
  .lazy-loading-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: var(--z-modal-bg);
    padding: 24px;
  }
  .lazy-loading-card {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
    padding: 28px 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }
</style>
