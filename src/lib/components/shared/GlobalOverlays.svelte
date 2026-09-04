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
   *    must be reachable from any window that touches a remote;
   *  • the AI consent prompt — same shape of problem. The endpoint is
   *    process-wide and a tool call parks on the answer with its timeout already
   *    running, so the prompt cannot live in a window that may not be mounted.
   *    It used to sit in the launcher, which is exactly such a window: opening
   *    any product in tabbed mode closes the Welcome tab, and every prompt after
   *    that was answered by nobody and denied on timeout. The backend picks ONE
   *    window to ask in (see `mcp::consent::prompt_window`) and emits only there,
   *    so mounting this everywhere does not mean prompting everywhere.
   *  • the AI tools reference and the call log — the endpoint is process-wide,
   *    so both have the same answer in every window, and both are asked for from
   *    each window's own command palette. Their chunks are pulled on first open.
   *    The log's LISTENER is not lazy: a window that only started collecting when
   *    someone opened the panel would show a log that begins when you looked at
   *    it, which is the opposite of what a record is for.
   *
   * The switcher is always mounted (it owns the keybinding listener). The
   * credentials dialog is heavier — provider cards, OAuth forms — so its chunk
   * is pulled only the first time something raises it.
   */
  import { onMount } from 'svelte';
  import type { Component } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import WindowSwitcher from '$lib/components/shared/WindowSwitcher.svelte';
  import McpConsentModal from '$lib/components/shared/McpConsentModal.svelte';
  import { credentialsStore } from '$lib/stores/credentials.svelte';
  import { mcpStore } from '$lib/stores/mcp.svelte';
  import type { McpAuditEntry, McpConsentRequest } from '$lib/types/mcp';

  let Credentials = $state<Component | null>(null);
  let requested = false;

  let McpTools = $state<Component<{ onClose: () => void }> | null>(null);
  let toolsOpen = $state(false);

  let McpActivity = $state<Component<{ onClose: () => void }> | null>(null);
  let activityOpen = $state(false);

  $effect(() => {
    if (!credentialsStore.open || requested) return;
    requested = true;
    void import('$lib/components/shared/CredentialsModal.svelte')
      .then((m) => { Credentials = m.default; })
      .catch(() => { requested = false; });
  });

  // Opened from every product's command palette through one window event, rather than
  // through a store: the palettes are per-product and a shared store for a modal nobody
  // owns would be a fourth place to keep in step.
  onMount(() => {
    const open = () => {
      toolsOpen = true;
      if (!McpTools) {
        void import('$lib/components/shared/McpToolsModal.svelte')
          .then((m) => { McpTools = m.default; })
          .catch(() => { toolsOpen = false; });
      }
    };
    window.addEventListener('arbor:open-mcp-tools', open);
    return () => window.removeEventListener('arbor:open-mcp-tools', open);
  });

  onMount(() => {
    const open = () => {
      activityOpen = true;
      if (!McpActivity) {
        void import('$lib/components/shared/McpActivityModal.svelte')
          .then((m) => { McpActivity = m.default; })
          .catch(() => { activityOpen = false; });
      }
    };
    window.addEventListener('arbor:open-mcp-activity', open);
    return () => window.removeEventListener('arbor:open-mcp-activity', open);
  });

  // Collected in every window from the moment it mounts, panel open or not — see above.
  onMount(() => {
    const unlisten = listen<McpAuditEntry>('arbor://mcp-call', (e) => mcpStore.record(e.payload));
    return () => { void unlisten.then((f) => f()); };
  });

  // Consent only. The config and the call log are the home surface's business —
  // this listener exists so a prompt is never delivered to a window that has
  // nothing listening, and every window is a candidate.
  onMount(() => {
    const unlisten = listen<McpConsentRequest>('arbor://mcp-consent', (e) =>
      mcpStore.enqueue(e.payload),
    );
    return () => { void unlisten.then((f) => f()); };
  });
</script>

<WindowSwitcher />

<McpConsentModal />

{#if Credentials}
  <Credentials />
{/if}

{#if toolsOpen && McpTools}
  <McpTools onClose={() => (toolsOpen = false)} />
{/if}

{#if activityOpen && McpActivity}
  <McpActivity onClose={() => (activityOpen = false)} />
{/if}
