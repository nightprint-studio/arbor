<script lang="ts">
  /** DiffModal — the previous response beside the current one, both formatted as JSON when they parse. */
  import { GitCompare } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { ResponseData } from '$lib/types/hirundo';

  let { onClose }: { onClose: () => void } = $props();

  const tab = hirundoStore.active;
  const language = languageForPath('r.json');

  function pretty(text: string): string {
    try { return JSON.stringify(JSON.parse(text), null, 2); } catch { return text; }
  }

  const same = $derived(!!tab?.previous && !!tab.response && pretty(tab.previous.body) === pretty(tab.response.body));
  const tone = (r: ResponseData) => (r.status >= 500 ? 'error' : r.status >= 400 ? 'warning' : 'success');

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); onClose(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="1060px" height="640px" ariaLabel="Compare responses">
  {#snippet header()}
    <ModalHeader {onClose}>
      <GitCompare size={14} />
      <span class="modal-title">Previous answer ↔ current answer</span>
    </ModalHeader>
  {/snippet}

  {#if tab?.previous && tab.response}
    <div class="df">
      {#if same}<div class="df-note"><Alert variant="info" compact text="The two answers are identical (formatted alike)." /></div>{/if}
      <div class="df-cols">
        {#each [{ title: 'Previous', r: tab.previous }, { title: 'Current', r: tab.response }] as side (side.title)}
          <div class="df-side">
            <div class="df-h">
              <b>{side.title}</b>
              <Badge variant="status" size="sm" tone={tone(side.r)} label={`${side.r.status} ${side.r.status_text}`} />
              <span>{side.r.time_ms} ms</span><span>{side.r.size_bytes} B</span>
            </div>
            <div class="df-code"><CodeEditor value={pretty(side.r.body)} {language} readOnly lineNumbers /></div>
          </div>
        {/each}
      </div>
    </div>
  {:else}
    <EmptyState message="Nothing to compare" description="Send the request twice: the diff sets the last answer against the one before it." />
  {/if}

  {#snippet footer()}<ModalFooter hint="to close"><Button variant="primary" onclick={onClose}>Close</Button></ModalFooter>{/snippet}
</Modal>

<style>
  .df { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .df-note { padding: 8px 12px 0; }
  .df-cols { flex: 1; min-height: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 1px; background: var(--border-subtle); margin-top: 8px; }
  .df-side { display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--bg-base); }
  .df-h { display: flex; align-items: center; gap: 10px; padding: 6px 12px; font-size: var(--font-size-xs); color: var(--text-secondary); border-bottom: 1px solid var(--border-subtle); }
  .df-code { flex: 1; min-height: 0; display: flex; }
  .df-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
