<script lang="ts">
  /**
   * Fatal "git backend stopped" overlay for the Corvus (git) window.
   *
   * `corvus-be` is spawned once at app startup and has no live respawn yet, so
   * if it dies (crash / kill) git operations are gone and every in-flight call
   * fails. The shell detects the dead stream (the `ChildClient` reader hits EOF)
   * and emits `arbor://corvus-be-down`; this self-subscribing overlay then takes
   * over the whole window with a blocking, non-dismissible state — matching the
   * other full-window states (`BootSplash`, `MissingRepoState`) rather than a
   * dismissible `Modal`, because there is nothing to dismiss to. The only action
   * is a full app restart.
   */
  import { onMount } from 'svelte';
  import { AlertTriangle } from 'lucide-svelte';
  import { onCorvusBeDown, restartApp } from '$lib/ipc/app';
  import Button from '$lib/components/shared/ui/Button.svelte';

  let down = $state(false);
  let restarting = $state(false);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    void onCorvusBeDown(() => { down = true; }).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  });

  function restart() {
    restarting = true;
    void restartApp();
  }
</script>

{#if down}
  <div
    class="be-down"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="be-down-title"
    tabindex="-1"
  >
    <div class="card">
      <div class="icon"><AlertTriangle size={36} /></div>
      <h2 id="be-down-title">Git engine stopped</h2>
      <p>
        Corvus's git backend process (<code>corvus-be</code>) has terminated, so
        git operations are no longer available. Restart Arbor to recover.
      </p>
      <Button variant="primary" onclick={restart} disabled={restarting}>
        {restarting ? 'Restarting…' : 'Restart Arbor'}
      </Button>
    </div>
  </div>
{/if}

<style>
  .be-down {
    position: fixed;
    inset: 0;
    z-index: 99999;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    /* Opaque-ish scrim, no blur (WebView2 compositor hard rule). */
    background: color-mix(in srgb, var(--bg-base) 84%, transparent);
  }
  .card {
    width: 100%;
    max-width: 420px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    text-align: center;
    padding: 28px 24px;
    border-radius: var(--radius-lg);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    box-shadow: 0 24px 60px -20px rgba(0, 0, 0, 0.6);
  }
  .icon {
    color: var(--error);
    display: flex;
  }
  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
  }
  p {
    margin: 0;
    font-size: 13.5px;
    line-height: 1.5;
    color: var(--text-muted);
  }
  code {
    font-family: var(--font-code);
    font-size: 0.92em;
    padding: 1px 5px;
    border-radius: 5px;
    background: var(--bg-hover);
  }
</style>
