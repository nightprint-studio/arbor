<script lang="ts">
  /**
   * The overlays that belong to EVERY Arbor window, whatever product it hosts.
   *
   * Mounted once by `+page.svelte` next to the product shell (overlay surfaces
   * excluded — the recording HUD and the drag ghost own their whole window).
   * Anything here must be genuinely cross-product: it is paid for by the
   * launcher, the Git window, the File Explorer, Merula, Bennu and Tyto alike.
   *
   *  • the window switcher — leaving the window you are in can't depend on
   *    which product you happen to be in;
   *  • the credentials dialog — the broker is process-wide, so re-authenticating
   *    must be reachable from any window that touches a remote.
   *
   * The switcher is always mounted (it owns the keybinding listener). The
   * credentials dialog is heavier — provider cards, OAuth forms — so its chunk
   * is pulled only the first time something raises it.
   */
  import type { Component } from 'svelte';
  import WindowSwitcher from '$lib/components/shared/WindowSwitcher.svelte';
  import { credentialsStore } from '$lib/stores/credentials.svelte';

  let Credentials = $state<Component | null>(null);
  let requested = false;

  $effect(() => {
    if (!credentialsStore.open || requested) return;
    requested = true;
    void import('$lib/components/shared/CredentialsModal.svelte')
      .then((m) => { Credentials = m.default; })
      .catch(() => { requested = false; });
  });
</script>

<WindowSwitcher />

{#if Credentials}
  <Credentials />
{/if}
