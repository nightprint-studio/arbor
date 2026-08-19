<!--
  Is anything listening, and which backends does it carry.

  These two belong on one page because neither is a decision on its own: an endpoint
  with no product reachable answers nothing, and a product switched on with no endpoint
  is a setting with no effect. Read together they say what an AI client can even see.
-->
<script lang="ts">
  import { Plug, RotateCw } from 'lucide-svelte';

  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { getMcpClients } from '$lib/ipc/mcp';
  import { mcpStore } from '$lib/stores/mcp.svelte';
  import { MCP_PRODUCTS } from '$lib/types/mcp';
  import type { McpClients } from '$lib/types/mcp';

  let { guard }: { guard: (action: () => Promise<void>) => Promise<void> } = $props();

  const cfg    = $derived(mcpStore.config);
  const status = $derived(mcpStore.status);

  /** The line the user pastes into their client. Everything it needs, in one string. */
  const addCommand = $derived(
    status.running
      ? `claude mcp add --transport http arbor ${status.url} --header "Authorization: Bearer ${status.token}"`
      : '',
  );

  /** Nothing on → say so once, here, rather than letting three pages look live. */
  const noProduct = $derived(MCP_PRODUCTS.every((p) => !(cfg.products[p.id] ?? false)));

  // ── Who is on the other end ───────────────────────────────────────────────
  //
  // Polled rather than pushed: a handshake is not an event the shell forwards, and one
  // more topic to carry a number that changes a handful of times a day would be a channel
  // built for a page that is usually not open.
  let connected = $state<McpClients | null>(null);

  $effect(() => {
    if (!cfg.enabled) { connected = null; return; }
    let live = true;
    const read = () => {
      void getMcpClients()
        .then((c) => { if (live) connected = c; })
        .catch(() => {});
    };
    read();
    const timer = setInterval(read, 5000);
    return () => { live = false; clearInterval(timer); };
  });

  /** "3 minutes ago" — the only form in which a handshake time is worth reading. */
  function since(ms: number): string {
    const secs = Math.max(0, Math.round((Date.now() - ms) / 1000));
    if (secs < 45) return 'just now';
    const mins = Math.round(secs / 60);
    if (mins < 60) return `${mins} min ago`;
    const hours = Math.round(mins / 60);
    if (hours < 24) return `${hours} h ago`;
    return new Date(ms).toLocaleDateString();
  }
</script>

<section>
  <SectionHeader
    title="Endpoint"
    description="Lets an AI client (Claude Code and anything else that speaks MCP) call Arbor's backends on this machine. Nothing is reachable from outside it." />
  <Toggle
    checked={cfg.enabled}
    label="Accept connections from AI clients"
    onchange={(v) => guard(() => mcpStore.patch({ enabled: v }))} />

  {#if cfg.enabled}
    <div class="row">
      <label for="mcp-port">Port</label>
      <Input
        id="mcp-port"
        type="number"
        block={false}
        narrow
        value={String(cfg.port)}
        onchange={(v: string) => guard(() => mcpStore.patch({ port: Number(v) || 8787 }))} />
    </div>
  {/if}

  {#if status.detail}
    <Alert variant="warning">{status.detail}</Alert>
  {/if}

  {#if status.running}
    <div class="command">
      <code>{addCommand}</code>
      <CopyButton value={addCommand} variant="inline" label="Copy" />
    </div>
    <p class="hint">
      Run that once to connect Claude Code — the registration survives restarts, so it is
      genuinely once. If your client has no CLI, the same three values (URL, header name,
      token) go into its own MCP configuration by hand.
    </p>
    <div class="rotate">
      <Button variant="ghost" size="sm" onclick={() => guard(() => mcpStore.regenerateToken())}>
        {#snippet iconStart()}<RotateCw size={13} />{/snippet}
        Regenerate token
      </Button>
      <span>Every client configured with the current token stops working and must be re-registered.</span>
    </div>
  {/if}
</section>

{#if cfg.enabled}
  <section>
    <SectionHeader
      title="Clients"
      description="Who has introduced themselves since Arbor started, and whether anything is calling. Connections are not kept, so a client that met an earlier run of Arbor never says hello again — its calls are counted here even though they name nobody.">
      {#snippet actions()}
        {#if connected && connected.open_streams > 0}
          <Badge
            variant="tone"
            tone="success"
            size="sm"
            label={`${connected.open_streams} listening`} />
        {/if}
      {/snippet}
    </SectionHeader>

    {#if !connected?.running}
      <EmptyState compact message="The endpoint is not up, so nothing can be connected." />
    {:else if connected.clients.length === 0 && connected.requests > 0}
      <!-- Calls without a handshake. Saying "nobody has connected" here would be plainly
           false to anyone who can see the assistant working, and it is the normal state
           after Arbor restarts under a live client. -->
      <EmptyState
        compact
        message={`${connected.requests} calls this run, none of them naming a client.`}
        description="Something is talking to Arbor, but it introduced itself to an earlier run — a client only says hello when it first connects, and a restart gives it no reason to do it again." />
    {:else if connected.clients.length === 0}
      <EmptyState
        compact
        message="Nothing has called yet."
        description="Run the line above in your client, then come back." />
    {:else}
      <div class="clients">
        {#each connected.clients as client (client.name + client.version)}
          <div class="client">
            <Plug size={13} />
            <span class="who">
              <strong>{client.name}</strong>
              {#if client.version}<code>{client.version}</code>{/if}
            </span>
            <span class="when">
              {since(client.last_seen_ms)}
              {#if client.handshakes > 1}· {client.handshakes} handshakes{/if}
            </span>
          </div>
        {/each}
      </div>
      <p class="hint">
        {connected.requests} call{connected.requests === 1 ? '' : 's'} this run{connected.last_request_ms
          ? `, last ${since(connected.last_request_ms)}`
          : ''}.
        {#if connected.open_streams === 0}
          Nothing is listening for updates, so a client here will not learn about a change
          to the tool list until it reconnects.
        {/if}
      </p>
    {/if}
  </section>
{/if}

<section>
  <SectionHeader
    title="Products"
    description="Each product exposes its own tools. A product that is off contributes none — the AI client is not told it exists." />
  {#each MCP_PRODUCTS as product (product.id)}
    <Toggle
      checked={cfg.products[product.id] ?? false}
      label={product.name}
      description={product.blurb}
      onchange={(v) => guard(() => mcpStore.setProduct(product.id, v))} />
  {/each}

  {#if cfg.enabled && noProduct}
    <Alert variant="info">
      No product is on, so a connected client sees an empty tool list. Everything on the other
      pages still applies — it just has nothing to apply to yet.
    </Alert>
  {/if}
</section>

<style>
  section   { display: flex; flex-direction: column; gap: 10px; }
  .row      { display: flex; align-items: center; gap: 10px; }
  .row label{ font-size: 12px; color: var(--text-secondary); }
  .command  { display: flex; align-items: center; gap: 8px; padding: 8px 10px;
              background: var(--bg-base); border: 1px solid var(--border-subtle);
              border-radius: var(--radius-md); }
  .command code { flex: 1; overflow-x: auto; white-space: nowrap;
              font-family: var(--font-mono); font-size: 11px; color: var(--text-primary); }
  .hint     { margin: 0; font-size: 11.5px; line-height: 1.5; color: var(--text-tertiary); }
  .rotate   { display: flex; align-items: center; gap: 10px; }
  .rotate span { font-size: 11.5px; line-height: 1.45; color: var(--text-tertiary); }

  .clients  { display: flex; flex-direction: column; gap: 4px; }
  .client   { display: flex; align-items: center; gap: 8px; padding: 6px 10px;
              background: var(--bg-base); border: 1px solid var(--border-subtle);
              border-radius: var(--radius-md); color: var(--text-muted); }
  .who      { flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 6px; }
  .who strong { font-size: 12.5px; color: var(--text-primary); }
  .who code { font-family: var(--font-mono); font-size: 11px; color: var(--text-tertiary); }
  .when     { flex: none; font-size: 11px; color: var(--text-tertiary); }
</style>
