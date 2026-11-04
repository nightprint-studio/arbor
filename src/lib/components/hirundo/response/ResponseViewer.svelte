<script lang="ts">
  /**
   * ResponseViewer — what came back (or why nothing was sent), in one row of tabs — Pretty · Raw · Headers ·
   * Tests — with the answer's status at the right end of the same row: a badge coloured by class of status,
   * the time and the size in words, and the detail in a tooltip. States are spoken, never blank. An XML answer
   * is indented and a SOAP Fault is lifted out of it into a card above, with its code, message and detail.
   */
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { fsWriteBytes } from '$lib/ipc/fs';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { soapInspect } from '$lib/ipc/hirundo';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import type { RequestTab } from '$lib/stores/hirundo/workspace.svelte';
  import type { SoapInspect } from '$lib/types/hirundo';
  import { formatBytes, formatDuration, statusTone } from './format';

  let { tab }: { tab: RequestTab } = $props();

  let view = $state('pretty');
  let saving = $state(false);

  const response = $derived(tab.response);
  const contentType = $derived(response?.headers.find(([k]) => k.toLowerCase() === 'content-type')?.[1] ?? '');
  const isImage = $derived(!!response?.body_base64 && contentType.startsWith('image/'));
  const isHtml = $derived(!!response && !response.body_base64 && contentType.toLowerCase().includes('html'));
  const isXml = $derived(!!response && !response.body_base64 && (contentType.includes('xml') || response.body.trimStart().startsWith('<')));

  // XML answers (SOAP above all) are read by the backend: the fault, and an indented copy.
  let inspected = $state<SoapInspect | null>(null);
  $effect(() => {
    const r = response;
    inspected = null;
    if (!r || !isXml) return;
    let current = true;
    void soapInspect(r.body).then((i) => { if (current) inspected = i; }).catch(() => {});
    return () => { current = false; };
  });

  /** JSON formatted when it parses, XML indented; otherwise the text as it came. */
  const pretty = $derived.by(() => {
    if (!response) return '';
    if (isXml && inspected) return inspected.pretty;
    try { return JSON.stringify(JSON.parse(response.body), null, 2); } catch { return response.body; }
  });
  const language = $derived(
    languageForPath(isXml ? 'r.xml' : contentType.includes('json') || pretty !== response?.body ? 'r.json' : contentType.includes('html') ? 'r.html' : 'r.txt'),
  );
  const rawLanguage = languageForPath('r.txt');

  /** A file name for "Save as…": the last path segment of the final URL, else a generic one with the type's extension. */
  const suggestedName = $derived.by(() => {
    const last = (response?.final_url ?? '').split('?')[0].split('/').filter(Boolean).pop() ?? '';
    if (last.includes('.')) return last;
    const ext = contentType.split(';')[0].split('/').pop()?.split('+')[0] ?? 'bin';
    return `response.${ext}`;
  });

  async function saveBody(file: string) {
    saving = false;
    if (!response?.body_base64) return;
    try {
      const bin = atob(response.body_base64);
      const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
      await fsWriteBytes(file, bytes);
      toastStore.show(`Saved to ${file}.`, 'success');
    } catch (e) { toastStore.show(String(e), 'error'); }
  }

  const tabs = $derived<TabItem[]>([
    { id: 'pretty', label: 'Pretty' },
    { id: 'raw', label: 'Raw' },
    ...(isHtml ? [{ id: 'preview', label: 'Preview' }] : []),
    { id: 'headers', label: 'Headers', badge: response?.headers.length },
    ...(tab.report ? [{ id: 'tests', label: 'Tests', badge: `${tab.report.checks.filter((c) => c.passed).length}/${tab.report.checks.length}`, tone: 'default' as const }] : []),
  ]);
</script>

<div class="rv">
  {#if tab.sending}
    <div class="rv-center"><Spinner label="Waiting for the server…" delay={0} /></div>
  {:else if tab.unresolved.length || tab.missingSecrets.length}
    <div class="rv-pad">
      <Alert variant="warning" title="Not sent — a variable has no value">
        {#snippet actions()}<Button size="sm" variant="outline" onclick={() => hirundoUiStore.open({ kind: 'environments' })}>Environments…</Button>{/snippet}
        {#if tab.unresolved.length}<p>Undefined: {#each tab.unresolved as n, i (n)}<code>{'{{'}{n}{'}}'}</code>{i < tab.unresolved.length - 1 ? ', ' : ''}{/each}</p>{/if}
        {#if tab.missingSecrets.length}<p>Secret not in the keychain: {tab.missingSecrets.join(', ')}</p>{/if}
      </Alert>
    </div>
  {:else if tab.error}
    <div class="rv-pad"><Alert variant="error" title="The request failed" text={tab.error} /></div>
  {:else if !response}
    <div class="rv-center">
      <EmptyState message="No response yet" description="Send the request to see the answer here — its status, time and size, the body formatted, the headers and the result of your tests." />
      <div class="rv-keys">
        <span><Kbd label="Ctrl+Enter" variant="inline" /> send</span>
        <span><Kbd label="Ctrl+Shift+V" variant="inline" /> resolved variables</span>
        <span><Kbd label="Ctrl+Shift+U" variant="inline" /> cURL</span>
      </div>
    </div>
  {:else}
    <div class="rv-bar">
      <div class="rv-tabs"><Tabs items={tabs} value={view} variant="underline" size="sm" ariaLabel="Response views" onSelect={(id) => (view = id)} /></div>
      <div class="rv-status">
        <Badge variant="status" size="sm" tone={statusTone(response.status)} label={`${response.status} ${response.status_text}`.trim()} tooltip={`${response.http_version} · ${response.final_url}`} />
        <span class="rv-fig" use:tooltip={{ content: `Total time ${response.time_ms} ms`, description: 'From sending to the last byte of the body. Connection, wait and download are not timed apart.' }}>{formatDuration(response.time_ms)}</span>
        <span class="rv-fig" use:tooltip={{ content: `${response.size_bytes.toLocaleString()} bytes`, description: response.truncated ? 'The body is larger than what is kept (8 MB).' : 'Size of the body.' }}>{formatBytes(response.size_bytes)}</span>
        <CopyButton value={response.body} title="Copy the body" toastSuccess="Body copied" />
      </div>
    </div>

    {#if inspected?.fault}
      <div class="rv-pad">
        <Alert variant="error" title={`SOAP Fault${inspected.fault.code ? ` · ${inspected.fault.code}` : ''}`} text={inspected.fault.message || 'The service answered with a fault and no message.'}>
          {#if inspected.fault.detail}<p class="rv-detail">{inspected.fault.detail}</p>{/if}
        </Alert>
      </div>
    {/if}
    {#if response.truncated}
      <div class="rv-pad"><Alert variant="info" compact text="The body is larger than what is kept (8 MB): the rest was dropped." /></div>
    {/if}

    <div class="rv-body">
      {#if view === 'headers'}
        <table class="rv-headers">
          <tbody>
            {#each response.headers as [k, v], i (i)}<tr><th>{k}</th><td>{v}</td></tr>{/each}
          </tbody>
        </table>
      {:else if view === 'tests' && tab.report}
        <ul class="rv-checks">
          {#each tab.report.checks as c, i (i)}
            <li><Badge variant="status" size="sm" tone={c.passed ? 'success' : 'error'} label={c.passed ? 'PASS' : 'FAIL'} /><code>{c.line}</code>{#if c.detail}<span class="rv-muted">{c.detail}</span>{/if}</li>
          {/each}
        </ul>
      {:else if isImage}
        <div class="rv-image"><img alt="Response" src={`data:${contentType};base64,${response.body_base64}`} /></div>
      {:else if response.body_base64}
        <div class="rv-binary">
          <EmptyState message="Binary body" description={`${formatBytes(response.size_bytes)} · ${contentType || 'unknown type'}. A ${contentType.split('/')[0] || 'binary'} file is not shown as text — save it to open it.`} />
          <Button variant="primary" onclick={() => (saving = true)}>Save as…</Button>
        </div>
      {:else if view === 'preview' && isHtml}
        <!-- No scripts, no forms, no same-origin: the page is only looked at. -->
        <iframe class="rv-preview" title="Rendered response" sandbox="" srcdoc={response.body}></iframe>
      {:else}
        <CodeEditor value={view === 'raw' ? response.body : pretty} language={view === 'raw' ? rawLanguage : language} readOnly wrap lineNumbers={false} />
      {/if}
    </div>
  {/if}
</div>

{#if saving}
  <FileExplorerModal mode="save" title="Save the response body" initialFilename={suggestedName} onConfirm={(f: string) => void saveBody(f)} onCancel={() => (saving = false)} onClose={() => (saving = false)} />
{/if}

<style>
  .rv { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .rv-center { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; }
  .rv-keys { display: flex; gap: 16px; flex-wrap: wrap; justify-content: center; font-size: var(--font-size-xs); color: var(--text-muted); }
  .rv-keys span { display: inline-flex; align-items: center; gap: 6px; }
  .rv-pad { padding: 8px 12px 0; }
  .rv-bar { display: flex; align-items: center; gap: 12px; padding: 0 8px; border-bottom: 1px solid var(--border-subtle); }
  .rv-tabs { flex: 1; min-width: 0; }
  .rv-status { display: flex; align-items: center; gap: 12px; flex: none; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .rv-fig { font-variant-numeric: tabular-nums; cursor: default; }
  .rv-detail { margin: 4px 0 0; font-family: var(--font-mono); font-size: var(--font-size-xs); white-space: pre-wrap; }
  .rv-body { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; }
  .rv-body > :global(.code-editor), .rv-body > :global(div:only-child) { flex: 1; min-height: 0; }
  .rv-headers { border-collapse: collapse; width: 100%; font-size: var(--font-size-xs); }
  .rv-headers th, .rv-headers td { text-align: left; padding: 4px 12px; border-bottom: 1px solid var(--border-subtle); vertical-align: top; }
  .rv-headers th { color: var(--text-muted); font-weight: 500; white-space: nowrap; width: 1%; }
  .rv-headers td { font-family: var(--font-mono); word-break: break-all; }
  .rv-checks { list-style: none; margin: 0; padding: 8px 12px; display: flex; flex-direction: column; gap: 6px; font-size: var(--font-size-xs); }
  .rv-checks li { display: flex; align-items: center; gap: 8px; }
  .rv-checks code { font-family: var(--font-mono); }
  .rv-muted { color: var(--text-muted); }
  .rv-binary { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; padding: 24px; }
  .rv-preview { flex: 1; width: 100%; border: 0; background: #fff; }
  .rv-image { padding: 12px; }
  .rv-image img { max-width: 100%; }
</style>
