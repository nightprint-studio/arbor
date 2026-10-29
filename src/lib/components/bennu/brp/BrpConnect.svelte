<script lang="ts">
  /**
   * The disconnected state of the inspector: where to connect, and how to make a game connectable.
   *
   * A game answering nothing on the port looks exactly like a game without the plugin, and the user
   * cannot tell which from here — so the page says what the game needs instead of leaving the error
   * to be decoded. `Enter` connects.
   */
  import { untrack } from 'svelte';
  import { Plug } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';

  // The field's starting value; the user edits it from here.
  let endpoint = $state(untrack(() => brp.endpoint));
  let field = $state<HTMLInputElement | undefined>();
  $effect(() => { field?.focus(); });

  function connect() {
    void brp.connect(endpoint);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      connect();
    }
  }
</script>

<div class="bc">
  <div class="bc-card">
    <h3 class="bc-title">Connect to a running Bevy game</h3>
    <p class="bc-text">
      Inspect and edit the live world over the Bevy Remote Protocol. The game has to run with the
      remote plugins added:
    </p>
    <pre class="bc-code">app.add_plugins((RemotePlugin::default(), RemoteHttpPlugin::default()));</pre>

    <div class="bc-row">
      <Input
        bind:value={endpoint}
        bind:element={field}
        size="sm"
        placeholder="http://127.0.0.1:15702"
        ariaLabel="Remote Protocol endpoint"
        onkeydown={onKey}
      />
      <Button variant="primary" size="sm" loading={brp.connecting} onclick={connect}>
        {#snippet iconStart()}<Plug size={12} />{/snippet}
        Connect
      </Button>
    </div>

    {#if brp.connectError}
      <Alert variant="error" compact title="Cannot connect" text={brp.connectError} />
    {/if}
  </div>
</div>

<style>
  .bc { flex: 1; min-height: 0; display: flex; align-items: center; justify-content: center; padding: 16px; overflow-y: auto; }
  .bc-card { width: min(520px, 100%); display: flex; flex-direction: column; gap: 10px; }
  .bc-title { margin: 0; font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .bc-text { margin: 0; font-size: var(--font-size-sm); color: var(--text-secondary); line-height: 1.5; }
  .bc-code {
    margin: 0; padding: 8px 10px; border-radius: var(--radius-md);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary);
    overflow-x: auto;
  }
  .bc-row { display: flex; gap: 8px; align-items: center; }
</style>
