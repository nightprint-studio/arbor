<script lang="ts">
  /**
   * AssistantRequestPreview — what the assistant would send, shown instead of sent.
   *
   * The dry run: the endpoint, the headers (the key masked, never shown), the JSON body exactly as it
   * would go, and where its size comes from — instructions, tool definitions, conversation — with a
   * rough cost. In the real assistant the body is built by the same code that builds the request, so
   * this is the request and not a description of it; the answer to "what are you telling the model?"
   * and the way to see what a change of context, mode or effort does to what is sent.
   *
   * Reachable from the composer (Dry run on, and the send button shows this instead) and from the panel
   * menu at any time. Esc closes.
   */
  import { Eye, Copy, ShieldOff } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import StackedBar from '$lib/components/shared/ui/StackedBar.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { highlightCode } from '$lib/utils/highlight';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { formatTokens } from './assistant-format';

  const store = bennuAssistantStore;

  let tab = $state<'summary' | 'body'>('summary');
  const TABS: TabItem[] = [
    { id: 'summary', label: 'Summary' },
    { id: 'body', label: 'Request body' },
  ];

  const p = $derived(store.preview);
  // Input is billed at the model's price per million; the cached share (the instructions and the tools)
  // is read back at a tenth of it on every later request, so the first one is the expensive one.
  // Worked out by the backend from the price list, cache rates and long-prompt tier included.

  const parts = $derived([
    { id: 'system', label: 'Instructions', value: p?.breakdown.system ?? 0, tone: 'muted' as const },
    { id: 'tools', label: 'Tool definitions', value: p?.breakdown.tools ?? 0, tone: 'info' as const },
    { id: 'messages', label: 'Conversation', value: p?.breakdown.messages ?? 0, tone: 'accent' as const },
  ]);

  const copy = () => p && copyToClipboard(p.body, { successToast: 'Request copied' });
  const close = () => store.closePreview();
</script>

{#if store.previewOpen}
  <Modal onClose={close} width="820px" height="620px" padBody={false} bodyBorder ariaLabel="Request preview">
    {#snippet header()}
      <ModalHeader onClose={close}>
        <Eye size={14} />
        <span class="modal-title">What would be sent</span>
        <Badge variant="tone" tone="info" size="sm">Nothing is sent</Badge>
      </ModalHeader>
    {/snippet}

    <div class="pv">
      <div class="bar"><Tabs items={TABS} value={tab} variant="underline" size="sm" onSelect={(id) => (tab = id as 'summary' | 'body')} /></div>

      {#if store.previewLoading}
        <div class="state"><Spinner /> Building the request…</div>
      {:else if store.previewError}
        <div class="summary"><Alert variant="error" text={store.previewError} /></div>
      {:else if !p}
        <div class="state">Nothing to show.</div>
      {:else if tab === 'summary'}
        <div class="summary">
          <section class="card">
            <h4>Endpoint</h4>
            <div class="endpoint"><b>{p.method}</b> <code>{p.url}</code></div>
            <dl class="headers">
              {#each p.headers as [k, v]}
                <div><dt>{k}</dt><dd class:masked={k === 'x-api-key'}>{v}{#if k === 'x-api-key'}<ShieldOff size={11} />{/if}</dd></div>
              {/each}
            </dl>
          </section>

          <section class="card">
            <h4>Where the size comes from <span class="est">estimate · ~4 characters a token</span></h4>
            <StackedBar segments={parts} max={p.breakdown.total} height={8} ariaLabel="Tokens by part" />
            <ul class="legend">
              {#each parts as part (part.id)}
                <li><i class="dot t-{part.tone}"></i><span class="l">{part.label}</span><span class="v">~{formatTokens(part.value)}</span></li>
              {/each}
              <li class="total"><span class="l">Total input</span><span class="v">~{formatTokens(p.breakdown.total)}</span></li>
            </ul>
            <p class="cost">
              {#if p.estimate}
                About <b>${p.estimate.first.toFixed(3)}</b> for this request on {store.model.name} — then about
                <b>${p.estimate.next.toFixed(4)}</b> a request once the instructions and tools are cached.
              {:else}
                No price is listed for {store.model.name}, so no cost is shown.
              {/if}
            </p>
          </section>
        </div>
      {:else}
        <pre class="body"><code>{@html highlightCode(p.body, 'json')}</code></pre>
      {/if}
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <span class="note">This is the request as it would go, built from the open conversation. The key is never shown.</span>
        <div class="actions">
          <Button variant="ghost" size="sm" onclick={copy}><Copy size={12} /> Copy request</Button>
          <Button variant="secondary" size="sm" onclick={close}>Close</Button>
        </div>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .pv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bar { padding: 0 12px; border-bottom: 1px solid var(--border-subtle); }

  .summary { display: flex; flex-direction: column; gap: 12px; padding: 14px; overflow-y: auto; }
  .card { display: flex; flex-direction: column; gap: 9px; padding: 12px 14px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-elevated); }
  h4 { margin: 0; display: flex; align-items: baseline; gap: 8px; font-size: var(--font-size-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-secondary); }
  .est { font-weight: 400; text-transform: none; letter-spacing: 0; font-size: var(--font-size-2xs); color: var(--text-muted); }

  .endpoint { font-size: var(--font-size-sm); }
  .endpoint b { color: var(--accent); margin-right: 6px; }
  code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
  .headers { margin: 0; display: flex; flex-direction: column; gap: 3px; }
  .headers div { display: flex; gap: 10px; font-size: var(--font-size-xs); }
  dt { width: 150px; flex-shrink: 0; font-family: var(--font-code); color: var(--text-muted); }
  dd { margin: 0; display: inline-flex; align-items: center; gap: 6px; font-family: var(--font-code); color: var(--text-primary); }
  dd.masked { color: var(--warning); }

  .legend { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 5px; }
  .legend li { display: flex; align-items: center; gap: 8px; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .legend .total { padding-top: 5px; border-top: 1px solid var(--border-subtle); font-weight: 600; color: var(--text-primary); }
  .l { flex: 1; }
  .v { font-family: var(--font-code); font-variant-numeric: tabular-nums; }
  .dot { width: 8px; height: 8px; border-radius: 50%; }
  .t-muted { background: var(--text-muted); } .t-info { background: var(--info); } .t-accent { background: var(--accent); }
  .cost { margin: 0; font-size: var(--font-size-xs); line-height: 1.55; color: var(--text-muted); }
  .cost b { color: var(--text-primary); font-weight: 600; }

  .body { flex: 1; margin: 0; padding: 12px 14px; overflow: auto; font-family: var(--font-code); font-size: var(--font-size-xs); line-height: 1.55; background: var(--bg-base); }
  .note { font-size: var(--font-size-xs); color: var(--text-muted); }
  .actions { display: flex; gap: 8px; }
  .state { flex: 1; display: flex; align-items: center; justify-content: center; gap: 8px; font-size: var(--font-size-sm); color: var(--text-muted); }
</style>
