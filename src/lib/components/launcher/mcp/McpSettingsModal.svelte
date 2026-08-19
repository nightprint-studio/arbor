<!--
  The AI tool surface's settings.

  Four pages rather than one scroll, because the sections are not steps in a single
  decision — they are four independent gates that happen to compose, and stacked
  vertically the last of them (what a tool may do, and what one actually did) sat below
  the fold at every window height. The order is the order the decision is made in: is it
  on, what can it reach, what may it do, what did it do.

  The header carries the endpoint's state, so whichever page you are on says whether any
  of it is currently in force — a settings screen that looks live while nothing listens
  is the fastest way to conclude the feature is broken.
-->
<script lang="ts">
  import { Bot, FolderTree, ShieldCheck } from 'lucide-svelte';

  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { mcpStore } from '$lib/stores/mcp.svelte';
  import McpEndpointSection from './McpEndpointSection.svelte';
  import McpPermissionsSection from './McpPermissionsSection.svelte';
  import McpProjectsSection from './McpProjectsSection.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const cfg    = $derived(mcpStore.config);
  const status = $derived(mcpStore.status);

  let page  = $state('endpoint');
  let error = $state<string | null>(null);

  /**
   * Every write goes through here: report the failure in place and keep the modal open.
   *
   * Passed down to the pages rather than re-implemented on each, so a save that fails on
   * the Projects page cannot end up silently swallowed while the Endpoint page toasts.
   */
  async function guard(action: () => Promise<void>) {
    error = null;
    try { await action(); } catch (e) { error = String(e); }
  }

  const PAGES = $derived([
    { id: 'endpoint',    label: 'Endpoint',    icon: Bot },
    { id: 'projects',    label: 'Projects',    icon: FolderTree,
      badge: cfg.projects.length || undefined },
    { id: 'permissions', label: 'Permissions', icon: ShieldCheck },
  ]);
</script>

<Modal {onClose} width="760px" height="660px" ariaLabel="AI tool access settings">
  {#snippet header()}
    <ModalHeader title="AI tool access" {onClose}>
      {#snippet actions()}
        <Badge tone={status.running ? 'success' : 'neutral'}>
          {status.running ? `Listening on ${status.port}` : 'Off'}
        </Badge>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="shell">
    <Tabs
      variant="underline"
      value={page}
      items={PAGES}
      ariaLabel="Settings pages"
      onSelect={(id: string) => (page = id)} />

    {#if error}
      <Alert variant="error">{error}</Alert>
    {/if}

    <!-- Said once, at the top of whichever page you are on: the other three pages
         describe rules that are real but currently unenforced, and a reader who does
         not know that will read them as active. -->
    {#if !cfg.enabled && page !== 'endpoint'}
      <Alert variant="info">
        The endpoint is off, so nothing here is in force yet. These settings are kept and
        apply the moment you turn it on.
      </Alert>
    {/if}

    <div class="page">
      {#if page === 'endpoint'}
        <McpEndpointSection {guard} />
      {:else if page === 'projects'}
        <McpProjectsSection {guard} />
      {:else}
        <McpPermissionsSection {guard} />
      {/if}
    </div>
  </div>
</Modal>

<style>
  /* The modal body scrolls by default; the shell claims its full height and hands the
     scrolling to `.page` instead, so the strip stays put. `overflow: hidden` keeps the
     body's own scrollbar from ever waking up and giving the dialog two. */
  .shell { display: flex; flex-direction: column; gap: 14px;
           height: 100%; min-height: 0; overflow: hidden; }
  /* The pages scroll, the tab strip does not — losing the nav on the way down a long
     project list is how a four-page dialog reads as a broken one-page dialog. */
  .page  { display: flex; flex-direction: column; gap: 22px; flex: 1; min-height: 0;
           overflow-y: auto; }
</style>
