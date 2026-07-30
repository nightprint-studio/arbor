<script lang="ts">
  /**
   * One large object, read on demand and shown in full.
   *
   * A modal rather than an expanded cell, because a blob is not a cell-sized thing:
   * it is a scanned invoice, a PDF, a serialised document. Putting it back into a
   * 24-pixel row would be pretending it is a value like the others, which is exactly
   * the pretence that made the grid slow before the column was masked at all.
   *
   * ## Three answers, and the modal says which one it is showing
   *
   *  * **text** (`clob`, `text`, `xml`) — shown as text, because that is what it is;
   *  * **bytes** — shown as a hex dump of the first kilobyte. Not base64: base64 is
   *    how it crossed the wire, and nobody reads it. A hex dump with its ASCII
   *    column is how you recognise a PNG header or a zip in three seconds;
   *  * **too big** — the first four megabytes, and it says so rather than letting the
   *    reader believe they are looking at the whole thing.
   *
   * Saving writes the **decoded bytes**, never the base64 — the file that comes out
   * has to be the file that went in.
   */
  import { Save, Copy, FileDigit } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { fsWriteBytes, fsWriteTextFile } from '$lib/ipc/fs';
  import { readLob, type LobValue } from '$lib/ipc/picus/db';

  interface Props {
    connectionId: string;
    table: string;
    column: string;
    /** The row's key columns and the values it was read with. */
    keys: Record<string, string | null>;
    onClose: () => void;
  }

  let { connectionId, table, column, keys, onClose }: Props = $props();

  let value = $state<LobValue | null>(null);
  let error = $state('');
  let saving = $state(false);

  $effect(() => {
    let cancelled = false;
    readLob(connectionId, table, keys, column)
      .then((v) => { if (!cancelled) value = v; })
      .catch((e) => { if (!cancelled) error = String(e); });
    return () => { cancelled = true; };
  });

  /** The bytes behind a base64 payload. */
  function decoded(base64: string): Uint8Array {
    const binary = atob(base64.replace(/\s+/g, ''));
    const out = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
    return out;
  }

  /** How many bytes the reader is shown as hex before it stops being useful. */
  const HEX_LIMIT = 1024;

  /**
   * A classic hex dump: offset, sixteen bytes, then the printable ASCII.
   *
   * The ASCII column is the point. It is what turns "some bytes" into "that is a
   * PNG" or "that is a zip" without leaving the modal.
   */
  function hexDump(bytes: Uint8Array): string {
    const lines: string[] = [];
    for (let at = 0; at < bytes.length; at += 16) {
      const slice = bytes.subarray(at, at + 16);
      const hex = [...slice].map((b) => b.toString(16).padStart(2, '0')).join(' ');
      const ascii = [...slice].map((b) => (b >= 32 && b < 127 ? String.fromCharCode(b) : '·')).join('');
      lines.push(`${at.toString(16).padStart(8, '0')}  ${hex.padEnd(47)}  ${ascii}`);
    }
    return lines.join('\n');
  }

  const bytes = $derived(value?.base64 ? decoded(value.base64) : null);
  const isText = $derived(value?.text !== undefined && value?.text !== null);

  /** What the modal actually renders. */
  const shown = $derived.by(() => {
    if (isText) return value?.text ?? '';
    if (!bytes) return '';
    return hexDump(bytes.subarray(0, HEX_LIMIT));
  });

  /** `1.2 MB`, `840 bytes` — the size in the unit a person reads. */
  function size(n: number): string {
    if (n < 1024) return `${n} byte${n === 1 ? '' : 's'}`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} kB`;
    return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(shown);
      toastStore.show(isText ? 'Value copied.' : 'Hex dump copied.', 'success');
    } catch (e) {
      toastStore.show(`Nothing was copied — ${e}`, 'error');
    }
  }

  async function save(path: string) {
    saving = false;
    if (!value) return;
    try {
      // Text goes out as text and bytes go out as bytes: a blob written as its
      // base64 would be a file that is not the file that was stored.
      if (isText) await fsWriteTextFile(path, value.text ?? '');
      else if (bytes) await fsWriteBytes(path, bytes);
      toastStore.show(`${column} written to ${path.split(/[\\/]/).pop()}.`, 'success');
    } catch (e) {
      toastStore.show(`${path} could not be written — ${e}`, 'error');
    }
  }
</script>

<Modal {onClose} width="760px" height="560px" ariaLabel={`${column} — the stored value`}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <FileDigit size={14} />
      <span class="lv-title">{table}.{column}</span>
      {#if value}
        <Badge variant="tone" tone="neutral" size="sm" label={size(value.bytes)} />
        {#if !isText}<Badge variant="tone" tone="info" size="sm" label="binary" />{/if}
      {/if}
    </ModalHeader>
  {/snippet}

  {#snippet children()}
    <div class="lv-body">
      {#if error}
        <StateBlock tone="error" label={error} />
      {:else if !value}
        <div class="lv-wait"><Spinner size={16} /> <span>Reading the value…</span></div>
      {:else}
        {#if value.truncated}
          <Alert variant="warning" compact>
            Only the first {size(4 * 1024 * 1024)} of {size(value.bytes)} were read. Save it to a
            file to get the whole thing — the rest is still on the server.
          </Alert>
        {/if}
        {#if !isText && bytes && bytes.length > HEX_LIMIT}
          <Alert variant="info" compact>
            Showing the first {HEX_LIMIT} bytes as hex. Saving writes all {size(bytes.length)}.
          </Alert>
        {/if}
        <pre class="lv-text" class:lv-hex={!isText}>{shown}</pre>
      {/if}
    </div>
  {/snippet}

  {#snippet footer()}
    <ModalFooter>
      <Button variant="secondary" size="sm" disabled={!value} onclick={() => void copy()}>
        {#snippet iconStart()}<Copy size={13} />{/snippet}
        Copy
      </Button>
      <Button variant="primary" size="sm" disabled={!value} onclick={() => (saving = true)}>
        {#snippet iconStart()}<Save size={13} />{/snippet}
        Save to a file…
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if saving}
  <FileExplorerModal
    mode="save"
    title={`Save ${column}`}
    initialFilename={`${table}-${column}`.toLowerCase()}
    onConfirm={(path) => void save(String(path))}
    onClose={() => (saving = false)}
  />
{/if}

<style>
  .lv-title { font-family: var(--font-code); font-size: var(--font-size-sm); }

  .lv-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    min-height: 0;
    padding: 10px 12px;
  }

  .lv-wait {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }

  .lv-text {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 8px 10px;
    overflow: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
  }
  /* A dump only lines up if nothing wraps. */
  .lv-hex { white-space: pre; word-break: normal; }
</style>
